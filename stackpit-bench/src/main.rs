use anyhow::{bail, Context, Result};
use clap::Parser;
use knee::{IntervalAgg, Verdict};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

mod chart;
mod driver;
mod knee;
mod payload;
mod sampler;
mod stats;

#[derive(Parser, Debug)]
#[command(
    name = "stackpit-bench",
    about = "Open-loop SQLite ingestion benchmark"
)]
pub struct Args {
    /// Ingest base URL (the ingest listener, not the admin UI)
    #[arg(long, default_value = "http://127.0.0.1:3001")]
    pub url: String,
    /// Project id (the {project_id} in /api/{project_id}/envelope)
    #[arg(long)]
    pub project: u64,
    /// Project DSN public key (sentry_key)
    #[arg(long)]
    pub key: String,
    /// SQLite file path or postgres:// URL
    #[arg(long)]
    pub db: String,
    /// Ramp start rate, events/s
    #[arg(long, default_value_t = 250)]
    pub ramp_start: u64,
    /// Ramp step, events/s added per interval
    #[arg(long, default_value_t = 250)]
    pub ramp_step: u64,
    /// Ramp interval seconds
    #[arg(long, default_value_t = 20)]
    pub ramp_interval: u64,
    /// Soak duration seconds
    #[arg(long, default_value_t = 300)]
    pub soak: u64,
    /// Distinct exception type/value pairs (issue cardinality)
    #[arg(long, default_value_t = 100)]
    pub issues: u32,
    /// Per-request timeout seconds
    #[arg(long, default_value_t = 5)]
    pub timeout: u64,
    /// Output directory
    #[arg(long, default_value = "bench-results")]
    pub out: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    std::fs::create_dir_all(&args.out).context("create output directory")?;

    let mut sampler = sampler::Sampler::connect(&args.db).await?;
    sampler.assert_fresh().await?;

    let timeout = Duration::from_secs(args.timeout);
    let d = driver::Driver {
        client: driver::make_client(timeout, 256),
        envelope_url: format!(
            "{}/api/{}/envelope",
            args.url.trim_end_matches('/'),
            args.project
        ),
        auth: format!("Sentry sentry_key={}, sentry_version=7", args.key),
        timeout_ms: timeout.as_secs_f64() * 1000.0,
    };
    driver::prewarm(&d.client, &args.url, 64).await;

    let mut gen = payload::PayloadGen::new(args.issues);
    driver::provision(&d, &mut gen).await?;

    let col = stats::Collector::new();
    let run_start = Instant::now();

    // 1 Hz DB sampler; owns the Sampler for the whole run.
    let sample_col = col.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tick.tick().await;
            let second = run_start.elapsed().as_secs();
            match sampler.sample().await {
                Ok(s) => {
                    sample_col.record_db_sample(second, s.persisted_delta, s.db_bytes, s.wal_bytes)
                }
                Err(e) => eprintln!("sampler error: {e}"),
            }
        }
    });

    let envelope_bytes = gen.next_envelope().len();
    let mut history: Vec<IntervalAgg> = Vec::new();
    let mut target = args.ramp_start;
    let mut last_good_target = 0u64;

    // Ramp
    println!(
        "ramp: start {}/s, +{}/s every {}s",
        args.ramp_start, args.ramp_step, args.ramp_interval
    );
    let knee_target = loop {
        let inflight = Arc::new(tokio::sync::Semaphore::new(
            (target as usize)
                .saturating_mul(args.timeout as usize)
                .max(64),
        ));
        let from_s = run_start.elapsed().as_secs();
        driver::run_interval(
            &d,
            &mut gen,
            col.clone(),
            target,
            args.ramp_interval,
            run_start,
            inflight,
            "ramp",
        )
        .await;
        tokio::time::sleep(Duration::from_secs(1)).await; // response grace
        let to_s = run_start.elapsed().as_secs();
        let agg = col.aggregate(from_s, to_s, target);
        println!(
            "  target {:>6}/s  ok {:>7}  persisted {:>7}  503 {:>5}  err {:>4}  lag p99 {:>6.1}ms",
            agg.target,
            agg.ok,
            agg.persisted,
            agg.s503,
            agg.errors + agg.timeouts + agg.dropped,
            agg.lag_p99_ms
        );
        if col.saw_429() {
            bail!("received HTTP 429: a project/key rate limit is still configured; remove it and re-run");
        }
        if history.is_empty() && agg.ok > 0 && agg.persisted == 0 {
            bail!(
                "server accepted events but no rows appeared in {}; wrong --db path?",
                args.db
            );
        }
        if agg.scheduled > 0 && (agg.errors + agg.timeouts) as f64 / agg.scheduled as f64 > 0.05 {
            bail!(
                "connection failure/timeout rate above 5%; server unreachable or dying, aborting"
            );
        }
        if !knee::interval_failing(&agg) {
            last_good_target = target;
        }
        history.push(agg);
        match knee::evaluate(&history) {
            Verdict::Continue => {
                target += args.ramp_step;
            }
            Verdict::Knee => break last_good_target,
            Verdict::ClientSaturated => {
                bail!("client saturated (scheduler lag p99 above limit); the load generator, not the server, is the bottleneck. Use a bigger client machine or lower rates.")
            }
        }
        if run_start.elapsed().as_secs() > 3600 {
            bail!("ramp exceeded 1h without finding a knee; check configuration");
        }
    };
    if knee_target == 0 {
        bail!(
            "knee below the starting rate ({}/s); lower --ramp-start",
            args.ramp_start
        );
    }

    // Soak at 90% of the last target that passed cleanly.
    let soak_rate = (knee_target as f64 * 0.9) as u64;
    let soak_from = run_start.elapsed().as_secs();
    println!(
        "knee near {knee_target}/s; soaking at {soak_rate}/s for {}s",
        args.soak
    );
    let inflight = Arc::new(tokio::sync::Semaphore::new(
        (soak_rate as usize)
            .saturating_mul(args.timeout as usize)
            .max(64),
    ));
    let soak_task = {
        let d = d.clone();
        let col = col.clone();
        let soak = args.soak;
        tokio::spawn(async move {
            driver::run_interval(
                &d, &mut gen, col, soak_rate, soak, run_start, inflight, "soak",
            )
            .await;
        })
    };
    let mut soak_history: Vec<IntervalAgg> = Vec::new();
    let mut sustained = true;
    let mut evaluated = 0u64;
    while evaluated < args.soak {
        let secs = args.ramp_interval.min(args.soak - evaluated);
        tokio::time::sleep(Duration::from_secs(secs + 1)).await;
        let from_s = soak_from + evaluated;
        let to_s = from_s + secs;
        let agg = col.aggregate(from_s, to_s, soak_rate);
        println!(
            "  soak {:>6}/s  ok {:>7}  persisted {:>7}  503 {:>5}  lag p99 {:>6.1}ms",
            agg.target, agg.ok, agg.persisted, agg.s503, agg.lag_p99_ms
        );
        if col.saw_429() {
            soak_task.abort();
            bail!("received HTTP 429 during soak: rate limit configured");
        }
        soak_history.push(agg);
        evaluated += secs;
        if !matches!(knee::evaluate(&soak_history), Verdict::Continue) {
            sustained = false;
            soak_task.abort();
            break;
        }
    }

    let soak_to = if sustained {
        soak_from + args.soak
    } else {
        run_start.elapsed().as_secs()
    };
    let _ = soak_task.await;

    // Drain stragglers, then report.
    tokio::time::sleep(timeout).await;
    let buckets = col.snapshot();
    let csv_path = args.out.join("bench.csv");
    stats::write_csv(&csv_path, &buckets)?;
    let subtitle = format!("soak {soak_rate}/s from t={soak_from}s to t={soak_to}s");
    let svg_path = args.out.join("bench.svg");
    let backend = if sampler::is_postgres_url(&args.db) {
        "PostgreSQL"
    } else {
        "SQLite"
    };
    chart::render_chart(&buckets, backend, &subtitle, &svg_path)?;

    print_summary(
        &buckets,
        knee_target,
        soak_rate,
        soak_from,
        soak_to,
        evaluated,
        sustained,
        args.issues,
        envelope_bytes,
    );
    println!("wrote {} and {}", csv_path.display(), svg_path.display());
    if !sustained {
        std::process::exit(2);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn print_summary(
    buckets: &std::collections::BTreeMap<u64, stats::SecondBucket>,
    knee_target: u64,
    soak_rate: u64,
    soak_from: u64,
    soak_to: u64,
    soaked_secs: u64,
    sustained: bool,
    issues: u32,
    envelope_bytes: usize,
) {
    let soak: Vec<_> = buckets.range(soak_from..soak_to).map(|(_, b)| b).collect();
    let mut lat: Vec<f64> = soak
        .iter()
        .flat_map(|b| b.latencies_ms.iter().copied())
        .collect();
    lat.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let persisted_total: u64 = soak.iter().map(|b| b.persisted).sum();
    let soak_span = soak_to.saturating_sub(soak_from);
    let persisted_rate = if soak_span > 0 {
        persisted_total / soak_span
    } else {
        0
    };
    let max_wal = buckets.values().map(|b| b.wal_bytes).max().unwrap_or(0);
    println!("\n== summary ==");
    println!("knee: ~{knee_target} events/s (last cleanly sustained ramp target)");
    println!(
        "soak: {soak_rate} events/s for {soaked_secs}s: {}",
        if sustained {
            "SUSTAINED"
        } else {
            "NOT sustained"
        }
    );
    println!("persisted during soak: {persisted_rate} rows/s average");
    println!(
        "HTTP accept latency during soak: p50 {:.1} ms, p99 {:.1} ms",
        stats::percentile(&lat, 0.5),
        stats::percentile(&lat, 0.99)
    );
    println!(
        "max WAL size: {:.1} MiB",
        max_wal as f64 / (1024.0 * 1024.0)
    );
    println!(
        "payload: error events, ~{:.1} KiB each, {issues} distinct issues",
        envelope_bytes as f64 / 1024.0
    );
}
