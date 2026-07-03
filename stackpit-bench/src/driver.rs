#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    S429,
    S503,
    Error,
    Timeout,
    Dropped,
}

use crate::payload::PayloadGen;
use crate::stats::Collector;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct Driver {
    pub client: reqwest::Client,
    pub envelope_url: String,
    pub auth: String,
    pub timeout_ms: f64,
}

pub fn make_client(timeout: Duration, pool_size: usize) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(timeout)
        .pool_max_idle_per_host(pool_size)
        .tcp_nodelay(true)
        .build()
        .expect("reqwest client")
}

/// Provision the project before the flood: one envelope, retried briefly, so
/// auto-provisioning finishes before thousands of concurrent requests race it
/// and trip the per-IP auth-failure limiter into 429s.
pub async fn provision(d: &Driver, gen: &mut PayloadGen) -> anyhow::Result<()> {
    for attempt in 0..10u64 {
        let res = d
            .client
            .post(&d.envelope_url)
            .header("Content-Type", "application/x-sentry-envelope")
            .header("X-Sentry-Auth", &d.auth)
            .body(gen.next_envelope())
            .send()
            .await;
        if let Ok(resp) = res {
            if resp.status().is_success() {
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_millis(200 * (attempt + 1))).await;
    }
    anyhow::bail!("warm-up envelope never got a 2xx; is the server up and the key correct?")
}

pub async fn prewarm(client: &reqwest::Client, base_url: &str, n: usize) {
    let url = format!("{}/health", base_url.trim_end_matches('/'));
    let mut handles = Vec::with_capacity(n);
    for _ in 0..n {
        let client = client.clone();
        let url = url.clone();
        handles.push(tokio::spawn(async move {
            let _ = client.get(&url).send().await;
        }));
    }
    for h in handles {
        let _ = h.await;
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn run_interval(
    d: &Driver,
    gen: &mut PayloadGen,
    col: Arc<Collector>,
    rate: u64,
    secs: u64,
    run_start: Instant,
    inflight: Arc<Semaphore>,
    phase: &'static str,
) {
    let total = rate * secs;
    let interval_start = Instant::now();
    for i in 0..total {
        let intended = interval_start + Duration::from_secs_f64(i as f64 / rate as f64);
        tokio::time::sleep_until(intended.into()).await;
        let lag_ms = intended.elapsed().as_secs_f64() * 1000.0;
        let second = run_start.elapsed().as_secs();
        col.note_tick(second, rate, phase);

        let permit = match inflight.clone().try_acquire_owned() {
            Ok(p) => p,
            Err(_) => {
                col.record(second, Outcome::Dropped, 0.0, lag_ms);
                continue;
            }
        };
        let body = gen.next_envelope();
        let client = d.client.clone();
        let url = d.envelope_url.clone();
        let auth = d.auth.clone();
        let timeout_ms = d.timeout_ms;
        let col = col.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let res = client
                .post(&url)
                .header("Content-Type", "application/x-sentry-envelope")
                .header("X-Sentry-Auth", &auth)
                .body(body)
                .send()
                .await;
            let (outcome, latency_ms) = match res {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    let _ = resp.bytes().await;
                    let l = intended.elapsed().as_secs_f64() * 1000.0;
                    let o = match status {
                        200..=299 => Outcome::Ok,
                        429 => Outcome::S429,
                        503 => Outcome::S503,
                        _ => Outcome::Error,
                    };
                    (o, l)
                }
                Err(e) if e.is_timeout() => (Outcome::Timeout, timeout_ms),
                Err(_) => (Outcome::Error, intended.elapsed().as_secs_f64() * 1000.0),
            };
            col.record(second, outcome, latency_ms, lag_ms);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payload::PayloadGen;
    use crate::stats::Collector;
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn spawn_server(status_line: &'static str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 65536];
                    loop {
                        // Read until end of headers, then drain content-length body.
                        let mut req = Vec::new();
                        loop {
                            let n = match sock.read(&mut buf).await {
                                Ok(0) | Err(_) => return,
                                Ok(n) => n,
                            };
                            req.extend_from_slice(&buf[..n]);
                            if let Some(pos) = find_headers_end(&req) {
                                let cl = content_length(&req[..pos]).unwrap_or(0);
                                while req.len() < pos + cl {
                                    let n = match sock.read(&mut buf).await {
                                        Ok(0) | Err(_) => return,
                                        Ok(n) => n,
                                    };
                                    req.extend_from_slice(&buf[..n]);
                                }
                                break;
                            }
                        }
                        let resp = format!(
                            "HTTP/1.1 {status_line}\r\ncontent-length: 2\r\nconnection: keep-alive\r\n\r\n{{}}"
                        );
                        if sock.write_all(resp.as_bytes()).await.is_err() {
                            return;
                        }
                    }
                });
            }
        });
        format!("http://{addr}")
    }

    fn find_headers_end(b: &[u8]) -> Option<usize> {
        b.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4)
    }

    fn content_length(head: &[u8]) -> Option<usize> {
        let text = std::str::from_utf8(head).ok()?;
        text.lines().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.eq_ignore_ascii_case("content-length")
                .then(|| v.trim().parse().ok())?
        })
    }

    async fn drive(base: String, rate: u64, secs: u64, permits: usize) -> Arc<Collector> {
        let col = Collector::new();
        let d = Driver {
            client: make_client(Duration::from_secs(2), 64),
            envelope_url: format!("{base}/api/1/envelope"),
            auth: "Sentry sentry_key=k, sentry_version=7".into(),
            timeout_ms: 2000.0,
        };
        let mut gen = PayloadGen::new(5);
        let inflight = Arc::new(tokio::sync::Semaphore::new(permits));
        run_interval(
            &d,
            &mut gen,
            col.clone(),
            rate,
            secs,
            Instant::now(),
            inflight,
            "ramp",
        )
        .await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        col
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn all_ticks_succeed_against_200_server() {
        let base = spawn_server("200 OK").await;
        let col = drive(base, 200, 2, 1024).await;
        let agg = col.aggregate(0, 10, 200);
        assert_eq!(agg.scheduled, 400);
        assert_eq!(agg.ok, 400);
        assert_eq!(agg.dropped, 0);
        assert!(agg.lag_p99_ms < crate::knee::CLIENT_LAG_LIMIT_MS);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn s503_and_429_are_classified() {
        let base = spawn_server("503 Service Unavailable").await;
        let col = drive(base, 100, 1, 1024).await;
        let agg = col.aggregate(0, 10, 100);
        assert_eq!(agg.s503, 100);

        let base = spawn_server("429 Too Many Requests").await;
        let col = drive(base, 50, 1, 1024).await;
        assert!(col.saw_429());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn semaphore_exhaustion_records_dropped() {
        let base = spawn_server("200 OK").await;
        let col = drive(base, 500, 1, 1).await;
        let agg = col.aggregate(0, 10, 500);
        assert_eq!(agg.scheduled, 500);
        assert!(agg.dropped > 0, "expected drops with 1 permit");
        assert_eq!(agg.ok + agg.dropped + agg.errors + agg.timeouts, 500);
    }
}
