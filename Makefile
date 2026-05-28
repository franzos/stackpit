# stackpit — local dev + test tasks.
#
# GUIX workspace: cargo needs the toolchain + OpenSSL paths supplied via
# `guix shell`. The CARGO wrapper does that when `guix` is on PATH and is a
# bare `cargo` otherwise (CI). Backends are mutually-exclusive cargo features;
# every target pins one explicitly — never build both at once.

SHELL       := bash
ADMIN_PORT  ?= 3333
INGEST_PORT ?= 3334
SEED_COUNT  ?= 500
SEED_WORKERS ?= 8
DB          := stackpit.db
LOG         := /tmp/stackpit.log

.PHONY: check clippy test check-pg clippy-pg test-pg fmt build css css-watch \
        serve-bg kill seed test-integration e2e e2e-trace help

# Tailwind v4 standalone CLI. The binary is dynamically linked against glibc +
# libgcc_s; wrap each invocation in `guix shell` on GUIX so the dynamic linker
# can find them. Outside GUIX (e.g. CI), invoke the binary directly.
TAILWIND     := ./.bin/tailwindcss
TAILWIND_IN  := templates/tailwind.css
TAILWIND_OUT := templates/style.css

ifeq ($(shell command -v guix 2>/dev/null),)
TAILWIND_RUN := $(TAILWIND)
else
# guix shell loads libc + libgcc_s; we then re-exec the binary with all argv
# preserved. Using `--` as $0 collides with tailwindcss's argument parsing, so
# the binary path itself is $0 instead.
TAILWIND_RUN := guix shell glibc gcc-toolchain -- bash -c \
  'export LD_LIBRARY_PATH=$$LIBRARY_PATH:$$LD_LIBRARY_PATH; exec "$$0" "$$@"' $(TAILWIND)
endif

# Cargo wrapper. On GUIX, supply rust + gcc + OpenSSL; bare cargo otherwise.
# The trailing `cargo` token is $0 for `bash -c`; real args arrive as "$@".
ifeq ($(shell command -v guix 2>/dev/null),)
CARGO := cargo
else
CARGO := guix shell rust rust:cargo gcc-toolchain openssl pkg-config -- \
  bash -c 'export CC=gcc; export OPENSSL_DIR=$$(dirname $$(dirname $$(realpath $$(which openssl)))); exec cargo "$$@"' cargo
endif

# Runtime wrapper for the built binary (sqlite + libgcc on LD_LIBRARY_PATH).
# Bare exec off GUIX.
ifeq ($(shell command -v guix 2>/dev/null),)
RUN_WRAP :=
else
RUN_WRAP := guix shell sqlite gcc-toolchain -- \
  bash -c 'export LD_LIBRARY_PATH=$$LIBRARY_PATH:$$LD_LIBRARY_PATH; exec "$$@"' wrap
endif

help: ## List targets
	@grep -E '^[a-z-]+:.*##' $(MAKEFILE_LIST) | sed 's/:.*##/\t/' | sort

check: ## cargo check (sqlite)
	$(CARGO) check --no-default-features --features sqlite

clippy: ## clippy -D warnings (sqlite)
	$(CARGO) clippy --no-default-features --features sqlite -- -D warnings

test: ## Unit tests only (sqlite). The integration target is feature-gated, so it's excluded here.
	$(CARGO) test --no-default-features --features sqlite

check-pg: ## cargo check (postgres)
	$(CARGO) check --no-default-features --features postgres

clippy-pg: ## clippy -D warnings (postgres)
	$(CARGO) clippy --no-default-features --features postgres -- -D warnings

test-pg: ## Unit tests only (postgres). Needs DATABASE_URL.
	$(CARGO) test --no-default-features --features postgres -- --test-threads=1

fmt: ## rustfmt via podman (never the host toolchain)
	podman run --rm -v $(CURDIR):/work -w /work rust:latest \
	  sh -c "rustup component add rustfmt && cargo fmt"

build: css ## Debug build (sqlite). Builds CSS first so the embedded asset is fresh.
	$(CARGO) build --no-default-features --features sqlite

css: ## Build templates/style.css from tailwind.css (minified)
	$(TAILWIND_RUN) -i $(TAILWIND_IN) -o $(TAILWIND_OUT) --minify

css-watch: ## Rebuild templates/style.css on template changes
	$(TAILWIND_RUN) -i $(TAILWIND_IN) -o $(TAILWIND_OUT) --watch

serve-bg: build ## Wipe the DB, launch `stackpit serve` in the background, wait for health
	@rm -f $(DB) $(DB)-wal $(DB)-shm
	@# oauth-configured stackpit.toml needs a master key at boot; ephemeral is fine since the DB is wiped each launch.
	@STACKPIT_MASTER_KEY=$${STACKPIT_MASTER_KEY:-$$(openssl rand -hex 32)} \
	  nohup $(RUN_WRAP) ./target/debug/stackpit serve > $(LOG) 2>&1 & disown
	@printf 'waiting for admin health '
	@for i in $$(seq 1 30); do \
		if curl -fsS http://127.0.0.1:$(ADMIN_PORT)/health >/dev/null 2>&1; then echo "ok"; exit 0; fi; \
		printf '.'; sleep 1; \
	done; \
	echo "FAILED"; tail -20 $(LOG); exit 1

kill: ## Stop the background server by port (NOT pkill -f — that self-matches the shell)
	@PID=$$(ss -ltnp 2>/dev/null | awk '/:$(ADMIN_PORT) /{print}' | grep -oE 'pid=[0-9]+' | head -1 | sed 's/pid=//'); \
	if [ -n "$$PID" ]; then kill "$$PID" && echo "killed $$PID"; else echo "not running on :$(ADMIN_PORT)"; fi

seed: ## Seed fake data (script default --count is 100000; we pass a small count)
	python3 scripts/generate-fake-data.py --count $(SEED_COUNT) --workers $(SEED_WORKERS) --quiet

test-integration: ## Run the integration suite against the running server (serve-bg + seed first)
	@curl -fsS http://127.0.0.1:$(ADMIN_PORT)/health >/dev/null 2>&1 || \
	  { echo "server not up on :$(ADMIN_PORT) — run 'make serve-bg && make seed' first"; exit 1; }
	$(CARGO) test --no-default-features --features sqlite,integration-tests --test integration -- --test-threads=1

PLAYWRIGHT_IMAGE ?= mcr.microsoft.com/playwright:v1.60.0-noble
PLAYWRIGHT_VOL   ?= stackpit-e2e-node-modules

e2e: ## Playwright smoke suite against the running server (serve-bg + seed first)
	@curl -fsS http://127.0.0.1:$(ADMIN_PORT)/health >/dev/null 2>&1 || \
	  { echo "server not up on :$(ADMIN_PORT) — run 'make serve-bg && make seed' first"; exit 1; }
	@TOK=$$(grep '^admin_token' stackpit.toml | sed 's/.*"\(.*\)".*/\1/'); \
	podman run --rm -t --network host \
	  -v "$(CURDIR)/tests/e2e:/work" \
	  -v "$(PLAYWRIGHT_VOL):/work/node_modules" \
	  -w /work \
	  -e BASE_URL=http://localhost:$(ADMIN_PORT) \
	  -e ADMIN_TOKEN="$$TOK" \
	  $(PLAYWRIGHT_IMAGE) \
	  bash -c "npm ci --no-audit --no-fund && npx playwright test"

e2e-trace: ## Open the Playwright HTML report from the last run
	podman run --rm -it --network host \
	  -v "$(CURDIR)/tests/e2e:/work" \
	  -v "$(PLAYWRIGHT_VOL):/work/node_modules" \
	  -w /work \
	  $(PLAYWRIGHT_IMAGE) \
	  bash -c "npx playwright show-report --host 0.0.0.0"
