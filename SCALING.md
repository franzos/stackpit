# Ingest throughput scaling notes

Working notes for the ingest-throughput effort on `feat/ingest-throughput`.
All numbers from one machine: AMD Ryzen 5 7640U (6c/12t), 64 GB RAM, NVMe on
LUKS ext4, Linux 6.19, load generator competing for the same cores. PostgreSQL
17 in a local Podman container. Payload: ~2.9 KiB error events, 100 distinct
issues (the `stackpit-bench` default).

## 1. Original approach and stats

Architecture (master, up to e21b003):

- One writer task consumes a single mpsc queue (capacity 50,000), drains up to
  2,000 events per batch (`src/writer/mod.rs`).
- The writer zstd-compresses every payload (level 3, one frame per event) in
  `compress_batch`, then runs bulk INSERTs plus (when due) aggregation upserts
  in one transaction on a single connection. `create_write_pool` hardcoded
  `max_connections = 1` for both backends (`src/db/pool.rs`).
- Backpressure: 512 MiB queued-bytes budget plus channel capacity; overflow
  returns HTTP 503.

Benchmark results (README, reproduced 2026-07-03):

| Backend | Knee | Sustained soak (5 min) | Persisted avg |
|---|---|---|---|
| SQLite | 10,000/s | 9,000/s, zero 503s | 9,138 rows/s |
| PostgreSQL | 10,000/s | 9,000/s, zero 503s | 9,123 rows/s |

Both backends landed on the same number because the serial writer, not the
database, was the governor.

Per-batch timing instrumentation (added this round, `src/writer/flush.rs`,
debug log `batch flush timings`) showed where the writer second went during
the SQLite soak: writer 96.8% busy, commit 61.6%, insert 19.1%, compress
15.7%, aggregation 0.3%. The SQLite commit cost is mostly the
`wal_autocheckpoint=1000` checkpoint running inline in the committing
connection, not the commit itself (WAL + synchronous=NORMAL does not fsync on
commit).

An intermediate experiment (2 full writer loops on PostgreSQL, round-robin,
aggregation in both) gave knee 12,000/s, soak 10,800/s, persisted 10,650
rows/s. Per-writer profile: insert 55-65%, aggregation ~17%, compress ~10%,
commit 5-15%. Lesson: duplicating aggregation across writers doubles that work
and makes the writers contend on the same issue rows.

## 2. Changes Round #1 and stats

Uncommitted on `feat/ingest-throughput` as of 2026-07-03:

1. **Per-batch timing instrumentation** (`src/writer/flush.rs`): one debug
   line per batch with `items`, `agg`, `compress_us`, `insert_us`, `agg_us`,
   `commit_us`. Enable with `RUST_LOG='stackpit=info,stackpit::writer=debug'`.
2. **Compression on the accept path** (`src/writer/handle.rs`,
   `StorableEvent::compress_payload` in `src/ingest/models.rs`): the identical
   per-event zstd call, relocated from the writer task to the request tasks,
   so it parallelizes across cores. Payloads over 64 KiB go through
   `block_in_place`. The writer's `compress_batch` remains as an idempotent
   fallback. Side effect: the queue byte budget now counts compressed bytes.
3. **`storage.ingest_writers` config** (default 1, clamped 1..=16, PostgreSQL
   only; SQLite warns and forces 1).
4. **PostgreSQL split pipeline** (`src/writer/mod.rs`): N insert-only workers
   (round-robin sharded via `WriterHandle`) run bulk-insert transactions on a
   dedicated pool (`create_ingest_pool`, N+1 connections), and forward
   per-batch scratch `Accumulators` (built from INSERT ... RETURNING) to one
   aggregation task. That task solely owns aggregation state and SQL, so the
   HLL read-modify-write blobs have a single owner and writers never contend
   on aggregation rows. SQLite keeps the original combined loop.
5. **Deterministic lock ordering** (`src/writer/aggregation.rs`,
   `bulk_upsert_tag_counts`): upsert batches sorted by key so any two
   concurrent transactions acquire row locks in the same order.

Results:

| Backend | Config | Knee | Sustained | Persisted |
|---|---|---|---|---|
| SQLite | (single writer, as always) | 14,000/s | 12,600/s, 5 min, zero 503s | 12,556 rows/s |
| PostgreSQL | ingest_writers = 2, split pipeline | >= 13,000/s clean window | no clean 5-min soak captured (see below) | ~15,700 rows/s burst at 16,000/s offered |

SQLite went from 9,000 to 12,600 sustained (+40%). The writer-side profile
after the change: compress ~0, commit ~65%, insert ~27%, still ~95% busy. The
gain exceeded compression's 16% share because the freed loop time produced
larger batches, and fewer commits per event amortizes the dominant checkpoint
cost.

PostgreSQL could not be brought to a clean 5-minute soak on this machine: with
the server now faster, the load generator saturates first (the bench aborts
with "client saturated" at 18,000-20,000/s offered, and starves into
connection timeouts around 15,000/s). The server itself never crashed, never
dropped a flush, and persisted up to ~15,700 rows/s. The single-box bottleneck
moved from Stackpit's writer to the machine.

Writer tests (48) pass on both backends, including a new
`multi_writer_overlapping_fingerprints_aggregate_exactly` test (2 writers,
overlapping fingerprints and users, exact event_count and HLL assertions). A
code-review pass verified the concurrency reasoning (upsert-before-HLL row
locks, lock ordering, byte accounting, shutdown draining).

## 3. Batch size sweep (2026-07-04)

The writer never waits to fill a batch: it drains whatever queued during the
previous flush, capped at `BATCH_SIZE` (2000). At 12,600/s the backlog during
one flush exceeds the cap, forcing multiple commit cycles where one larger
transaction would do. Tested via a temporary `STACKPIT_BATCH_SIZE` env
override, same binary, identical quick protocol (30 s windows, ramp-start
13000, step 1000, 60 s soak), SQLite:

| Batch cap | Knee | Best clean window | Soak |
|---|---|---|---|
| 2000 (baseline) | 14,000/s | 14k, ~13.3k rows/s | 12,600/s SUSTAINED, 12,800 rows/s |
| 10000 | 20,000/s | 20k, ~20,700 rows/s persisted | not measurable (client saturates) |

Every 30 s window from 13k to 20k passed with zero 503s and accept p99 under
8 ms. +43% on the knee; the commit/checkpoint cost amortizes much further
than the 2000 cap allowed. Soaks above ~13k collapse the load generator on
this box (sampler pool timeouts, accept p50 1.4 -> 75 ms, connection-error
storms), so the sustained figure for larger caps needs a second machine.

README refresh (2026-07-04, final branch code, 60 s windows, default batch):
knee 12,000, soak 10,800/s x 300 s SUSTAINED with zero 503s, 10,867 rows/s,
p50 1.7 ms / p99 6.9 ms, WAL 8.9 MiB; `docs/benchmark.svg` regenerated from
this run. A pinned attempt at the round-1 operating point (knee forced to
14,000, soak 12,600) passed its 14k ramp window but failed the 5-minute soak
with 503 bursts in minutes 4-5: the knee genuinely varies 12,000-14,000 run
to run on this desktop, so 10,800 is the published conservative figure.

Productized as `storage.ingest_batch_size` (default 2000, clamped 1..=50000,
both backends; the env knob is gone). Reliability trade-offs of larger caps:
the batch is the transaction, so a double-failed flush drops up to the cap
(2000 -> 10000 events); longer write-lock holds (background pools sit behind
busy_timeout=5s); slightly larger in-flight crash-loss window on top of the
50k channel; burstier commit cadence at saturation. No waiting is introduced
at low load; the cap is only reachable under backlog.

## How we run the tests

Unit/integration tests (GUIX, from repo root):

```bash
# SQLite (default features)
guix shell -m manifest.scm -- sh -c 'CC=gcc OPENSSL_DIR=$(dirname $(dirname $(realpath $(which openssl)))) cargo test --lib writer::'

# PostgreSQL (shared DB, tests must not interleave)
podman run -d --name pg-stackpit-bench -e POSTGRES_PASSWORD=bench -e POSTGRES_DB=stackpit_bench -p 127.0.0.1:5433:5432 docker.io/library/postgres:17
podman exec pg-stackpit-bench psql -U postgres -c 'CREATE DATABASE stackpit_test'
guix shell -m manifest.scm -- sh -c 'CC=gcc OPENSSL_DIR=$(dirname $(dirname $(realpath $(which openssl)))) DATABASE_URL=postgres://postgres:bench@127.0.0.1:5433/stackpit_test CARGO_TARGET_DIR=target-pg cargo test --no-default-features --features postgres --lib writer:: -- --test-threads=1'
```

## Running the benchmark

`stackpit-bench` is an open-loop load generator: it fires ~2.9 KiB Sentry
error envelopes (100 distinct issues by default) at a paced target rate,
regardless of how fast the server responds. It ramps the rate until the write
path falls behind (the knee), then soaks at 90% of the knee for 5 minutes
(`--soak` to change). "Sustained" means: persisted rows keep up with accepted
requests and rejections stay under 1% in every window. It samples persisted
rows directly from the database once per second, so it must run on the same
host as the SQLite file (or have access to the PostgreSQL instance).

Artifacts per run: a per-second `bench.csv`, a `bench.svg` chart
(target vs accepted vs persisted), and a printed summary (knee, soak verdict,
persisted average, accept latency p50/p99, max WAL size). Exit code 0 = soak
sustained; 2 = soak failed its criteria (the artifacts are still written).

### Steps

```bash
# 1. Release build (server + bench)
cargo build --release -p stackpit -p stackpit-bench
# On the GUIX dev box, wrap cargo instead (sqlite into target/):
#   guix shell -m manifest.scm -- sh -c 'CC=gcc OPENSSL_DIR=$(dirname $(dirname $(realpath $(which openssl)))) cargo build --release -p stackpit -p stackpit-bench'
# PostgreSQL server variant (into target-pg/, same bench binary works for both):
#   guix shell -m manifest.scm -- sh -c 'CC=gcc OPENSSL_DIR=$(dirname $(dirname $(realpath $(which openssl)))) CARGO_TARGET_DIR=target-pg cargo build --release --no-default-features --features postgres -p stackpit'

# 2. Fresh working dir; the bench refuses a non-empty events table
mkdir bench-run && cd bench-run
/path/to/target/release/stackpit init
# PostgreSQL instead of SQLite: start a local instance, e.g.
#   podman run -d --name pg-stackpit-bench -e POSTGRES_PASSWORD=bench \
#     -e POSTGRES_DB=stackpit_bench -p 127.0.0.1:5433:5432 docker.io/library/postgres:17
# then add to stackpit.toml under [storage]:
#   database_url = "postgres://postgres:bench@127.0.0.1:5433/stackpit_bench"
#   ingest_writers = 2

# 3. Start the server; default mode = "open" auto-provisions the project
#    on the first envelope, so no key setup is needed. The RUST_LOG filter
#    enables the per-batch writer timings used for profiling (see below).
RUST_LOG='stackpit=info,stackpit::writer=debug' \
  /path/to/target/release/stackpit serve > server.log 2>&1 &

# 4. Pre-warm with ONE envelope before high ramp-start rates, otherwise the
#    auth flood before auto-provisioning trips the per-IP failure limiter
#    and the bench aborts on 429 (see gotchas)
curl -s -X POST http://127.0.0.1:3001/api/1/envelope \
  -H 'X-Sentry-Auth: Sentry sentry_key=0123456789abcdef0123456789abcdef, sentry_version=7' \
  -H 'Content-Type: application/x-sentry-envelope' \
  --data-binary $'{"event_id":"00000000000000000000000000000001"}\n{"type":"event","length":47}\n{"event_id":"00000000000000000000000000000001"}\n'
sleep 2
# The pre-warm event persists and trips the fresh-DB check; clear it:
sqlite3 stackpit.db "DELETE FROM events; DELETE FROM issues;"
# (postgres: psql ... -c 'TRUNCATE events, issues')

# 5. Run
/path/to/target/release/stackpit-bench \
  --url http://127.0.0.1:3001 --project 1 \
  --key 0123456789abcdef0123456789abcdef \
  --db stackpit.db \
  --ramp-start 10000 --ramp-step 2000 --ramp-interval 60 \
  --out bench-results
# For postgres: --db postgres://postgres:bench@127.0.0.1:5433/stackpit_bench
```

### Flag choices, and why

- Defaults (`--ramp-start 250 --ramp-step 250 --ramp-interval 20`) work but
  take a long time to reach the knee and, worse, 20 s windows are too short to
  catch slow periodic effects (WAL checkpoint stalls, flush hiccups). The
  knee then lands too high and the soak fails late. I use 60 s windows.
- To pin the soak at a known-good rate, use a coarse ramp: with
  `--ramp-start 10000 --ramp-step 8000 --ramp-interval 60` the 10k window
  passes, 18k and 26k fail, the knee locks at 10,000 and the soak runs at
  9,000. That is how the published SQLite number was produced.
- `--issues` controls fingerprint cardinality (the server groups on exception
  type/value); leave it at 100 unless you want to stress aggregation.

### Profiling the writer

With `RUST_LOG='stackpit=info,stackpit::writer=debug'` the writer logs one
`batch flush timings` line per batch with `items`, `agg`, `compress_us`,
`insert_us`, `agg_us`, `commit_us`. To see where the writer's second goes
during a soak, strip ANSI codes and sum the stage columns per minute:

```bash
sed 's/\x1b\[[0-9;]*m//g' server.log | grep 'batch flush timings'
```

Busy% = sum of the four stage columns over wall time. With
`ingest_writers > 1` the tracing span (`writer{w=0}`, `writer{w=1}`, ...)
identifies the task.

### Gotchas

- **Cold-start 429s**: at high ramp-start rates against a fresh DB, requests
  flood in before the first envelope auto-provisions the project; the failed
  auth attempts trip the per-IP failure limiter and the bench aborts on 429.
  Pre-warm with one envelope (step 4), then clear the persisted pre-warm rows.
- **Client saturation**: server and generator share the cores. Once the server
  outruns the generator, windows above ~16,000/s abort with scheduler-lag or
  connection-error messages. That is a client limit, not a server one. Getting
  a true server ceiling needs a second machine (or at least a separate box for
  PostgreSQL).
- The persisted line can briefly exceed the accepted line while the writer
  drains backlog after an overloaded ramp window; that is real, not a bug.
- Do not run against a database you care about: the run inserts millions of
  rows and the fresh-DB assertion exists for a reason.

## Guidance for the next person

- **Missing artifact**: a clean PostgreSQL 5-minute soak with the split
  pipeline. On this laptop, calibrate the ramp to land the soak below client
  saturation (ramp-start 12000, step 1000 should find knee ~13,000-14,000 and
  soak ~12,000), with the pre-warm above. Better: drive the load from a second
  machine.
- **ingest_writers > 2** is unlikely to show gains on a single box; the
  machine is already CPU-bound. Test on separate hosts before concluding
  anything about scaling beyond 2.
- **Checkpoint offloading is a dead end (measured 2026-07-04)**:
  wal_autocheckpoint=8000 plus a background PASSIVE checkpoint every 1s,
  re-tested with compression off the writer, was a regression: knee 13,000/s
  (6,384 503s in the 14,000/s window the baseline sustains), 60s soak at
  11,700/s, 12,236 rows/s persisted, WAL ballooning to 69.7 MiB (vs 8.7 MiB).
  The background checkpointer competes for the same disk and falls behind;
  the inline autocheckpoint=1000 amortizes better. Don't re-test on a single
  disk. Batch sizing turned out to be the real lever (section 3): with
  `ingest_batch_size = 10000` the knee moves to 20,000/s. A verified
  sustained figure above 12,600/s needs load driven from a second machine.
- **PostgreSQL's next insert lever is COPY** (binary) instead of multi-row
  INSERT if the insert stage stays dominant on real hardware.
- Known accepted gaps in the current changes: duplicate NewIssue/Regression/
  threshold notifications are possible when two flushes race on the same
  fingerprint (cooldown state is in the DB, so the blast radius is one extra
  notification); the multi-writer test runs on a 1-connection pool, so it
  verifies count exactness, not true lock concurrency.
- Before merging: CHANGELOG entry, operator-guide section for
  `ingest_writers` and `ingest_batch_size`, PostgreSQL README benchmark
  refresh (SQLite refreshed 2026-07-04, see section 3), and a decision on
  the defaults (1 writer is safe, 2 is probably right for PostgreSQL; batch
  2000 is safe, larger caps raise the drop-on-double-failure unit).
