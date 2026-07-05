# Observability

> Commercial feature - requires a license including the `observability` capability. See [Commercial features](./index.md) for the licensing model.

A Prometheus `/metrics` endpoint on the admin listener, so you can scrape request rates, latency, and ingestion counters into whatever monitoring stack you already run. Stackpit doesn't ship dashboards or alerting itself; the endpoint just exposes what a stock Prometheus, Grafana Agent, or OTel Collector expects.

## Prerequisites

- A commercial license that includes the `observability` feature, activated at `/web/admin/license`.
- A scrape token: the environment variable `STACKPIT_METRICS_TOKEN` set to a value your scraper will send as a bearer token.
- Network access from your scraper to the **admin listener** (default `127.0.0.1:3000`), not the ingestion listener.

## Enabling it

1. Activate a license carrying `observability` at `/web/admin/license`.
2. Set `STACKPIT_METRICS_TOKEN` in the environment stackpit runs under, for example:

```bash
export STACKPIT_METRICS_TOKEN=$(openssl rand -hex 32)
```

3. Restart stackpit. The token is read once at startup; there's no live-reload for it.

Both steps are required. A valid license with no token set (or vice versa) leaves the endpoint returning 404, not a partial or unauthenticated response.

## What it exposes

All metrics live under a single global Prometheus recorder shared by both listeners (admin and ingestion).

| Metric | Type | Meaning |
|---|---|---|
| `http_requests_total` | counter | Requests seen on either listener, labeled `method`, `path`, `status`. |
| `http_request_duration_seconds` | histogram | Request latency for the same requests, same labels. |
| `stackpit_events_accepted_total` | counter | Ingested events accepted by the write path. |
| `stackpit_events_rejected_total` | counter | Events rejected (filters, auth, quota). |
| `stackpit_events_dropped_total` | counter | Events dropped after repeated write-flush failures. |

Label cardinality is bounded on purpose:

- `method` is an allowlist (`GET`, `POST`, `PUT`, `DELETE`, `PATCH`, `HEAD`, `OPTIONS`, `TRACE`, `CONNECT`); anything else collapses to `OTHER`.
- `path` is the matched route template, not the raw request URL, so ingest URLs and project ids never turn into label values.
- There are no per-tenant, per-org, or per-project labels. This is instance-wide telemetry, not a per-customer breakdown.

## Scraping it

Point a standard Prometheus scrape config at the admin listener with the bearer token:

```yaml
scrape_configs:
  - job_name: stackpit
    scheme: http
    static_configs:
      - targets: ["127.0.0.1:3000"]
    metrics_path: /metrics
    authorization:
      type: Bearer
      credentials: <your STACKPIT_METRICS_TOKEN value>
```

The response is standard Prometheus text exposition format (`text/plain; version=0.0.4`), so anything that speaks that format (Prometheus itself, a Grafana Agent remote-write config, an OTel Collector's `prometheus` receiver) works unmodified.

## Access control and exposure

The endpoint fails **closed** at every gate:

| Condition | Response |
|---|---|
| License doesn't have `observability`, or it's Locked | 404 |
| License is fine but `STACKPIT_METRICS_TOKEN` isn't set (or is empty) | 404 |
| Token is set but the request's bearer token doesn't match | 401 |
| License Active or Grace, and bearer token matches | 200, metrics body |

The token comparison is constant-time (the token is held as a `secrecy::SecretString` internally), so a timing side-channel isn't a route to guessing it.

`/metrics` is allowlisted past the normal session-auth middleware, since a Prometheus scraper has no session cookie to send. That's intentional: the bearer token is what protects it, not the session layer. Be honest about what that means operationally: the admin listener's bind address can be non-loopback (it's allowed once `force_secure_cookies = true` is set for a reverse-proxy deployment), so in that configuration the scrape token is the actual access control, not the bind address. Keep the admin port network-restricted anyway (firewall, private network, proxy allowlist) as defence-in-depth, don't rely on the token alone.

## Grace period

During the 30-day grace window after a license expires, `/metrics` keeps serving. This is read-only telemetry with nothing to write, so there's no reduced-functionality mode to worry about, unlike write-gated commercial features elsewhere. Past grace, the license state flips to Locked and the endpoint returns 404 until you renew.

## Related

- [Operator Guide → Metrics / Observability](../operator-guide.md#metrics--observability): where this fits alongside the rest of the admin listener configuration.
- [Commercial features](./index.md): the licensing model this feature sits under.
