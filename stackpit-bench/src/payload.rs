use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct PayloadGen {
    counter: u64,
    run_prefix: String,
    /// Per-template JSON tail (everything after the dynamic fields), serialized
    /// once at startup so the hot path is string splicing, not a deep
    /// Value clone plus full re-serialization per event.
    suffixes: Vec<String>,
}

impl PayloadGen {
    pub fn new(issues: u32) -> Self {
        let issues = issues.max(1);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos() as u64;
        let suffixes = (0..issues)
            .map(|k| {
                let mut template = event_template(k);
                let obj = template.as_object_mut().expect("template is an object");
                obj.remove("event_id");
                obj.remove("timestamp");
                obj.remove("user");
                let serialized = serde_json::to_string(&template).expect("template serializes");
                assert!(serialized.len() > 2, "template must keep static fields");
                serialized[1..].to_string()
            })
            .collect();
        Self {
            counter: 0,
            run_prefix: format!("{nanos:016x}"),
            suffixes,
        }
    }

    pub fn next_envelope(&mut self) -> Vec<u8> {
        let k = (self.counter as usize) % self.suffixes.len();
        let event_id = format!("{}{:016x}", self.run_prefix, self.counter);
        let user_id = self.counter % 100;
        self.counter += 1;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_secs_f64();
        let payload = format!(
            "{{\"event_id\":\"{event_id}\",\"timestamp\":{now},\"user\":{{\"id\":\"bench-user-{user_id}\"}},{}",
            self.suffixes[k]
        );
        let header = format!("{{\"event_id\":\"{event_id}\"}}\n");
        let item = format!("{{\"type\":\"event\",\"length\":{}}}\n", payload.len());
        let mut out = Vec::with_capacity(header.len() + item.len() + payload.len() + 1);
        out.extend_from_slice(header.as_bytes());
        out.extend_from_slice(item.as_bytes());
        out.extend_from_slice(payload.as_bytes());
        out.push(b'\n');
        out
    }
}

fn event_template(k: u32) -> Value {
    let frames: Vec<Value> = (0..8)
        .map(|f| {
            json!({
                "filename": format!("app/services/handler_{f}.py"),
                "function": format!("process_stage_{f}"),
                "module": format!("app.services.handler_{f}"),
                "lineno": 40 + f * 17,
                "in_app": f >= 2,
                "context_line": format!("    result = stage_{f}.execute(payload, retries=3)"),
            })
        })
        .collect();
    let breadcrumbs: Vec<Value> = (0..5)
        .map(|b| {
            json!({
                "type": "default",
                "category": "app.pipeline",
                "level": "info",
                "message": format!("pipeline stage {b} completed with status ok, queue depth nominal"),
            })
        })
        .collect();
    json!({
        "event_id": "0",
        "timestamp": 0.0,
        "level": "error",
        "platform": "python",
        "logger": "app.pipeline",
        "transaction": format!("GET /api/bench/{}", k % 10),
        "release": "stackpit-bench@1.0.0",
        "environment": "production",
        "server_name": "bench-host-1",
        "sdk": { "name": "stackpit-bench", "version": "1.0.0" },
        "exception": {
            "values": [{
                "type": format!("BenchError{k:03}"),
                "value": format!("synthetic failure variant {k}: upstream dependency returned malformed response"),
                "mechanism": { "type": "generic", "handled": false },
                "stacktrace": { "frames": frames },
            }]
        },
        "breadcrumbs": { "values": breadcrumbs },
        "tags": {
            "component": "pipeline",
            "region": "eu-1",
            "tier": "backend",
        },
        "user": { "id": "bench-user-0" },
        "extra": { "queue": "default", "attempt": 1 },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(envelope: &[u8]) -> (serde_json::Value, serde_json::Value, serde_json::Value) {
        let text = std::str::from_utf8(envelope).unwrap();
        let mut lines = text.splitn(3, '\n');
        let header: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
        let item: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
        let rest = lines.next().unwrap();
        let len = item["length"].as_u64().unwrap() as usize;
        let event: serde_json::Value = serde_json::from_slice(&rest.as_bytes()[..len]).unwrap();
        (header, item, event)
    }

    #[test]
    fn envelope_framing_is_valid_with_exact_length() {
        let mut g = PayloadGen::new(10);
        let env = g.next_envelope();
        let (header, item, event) = parse(&env);
        assert_eq!(item["type"], "event");
        assert_eq!(header["event_id"], event["event_id"]);
        assert_eq!(event["event_id"].as_str().unwrap().len(), 32);
        assert!(event["timestamp"].as_f64().unwrap() > 1.7e9);
        assert_eq!(event["level"], "error");
        assert!(
            event["exception"]["values"][0]["stacktrace"]["frames"]
                .as_array()
                .unwrap()
                .len()
                >= 6
        );
    }

    #[test]
    fn event_ids_are_unique_and_monotonic_scheme() {
        let mut g = PayloadGen::new(3);
        let mut seen = std::collections::HashSet::new();
        for _ in 0..1000 {
            let (_, _, event) = parse(&g.next_envelope());
            assert!(seen.insert(event["event_id"].as_str().unwrap().to_string()));
        }
    }

    #[test]
    fn exception_rotates_across_issue_cardinality() {
        let mut g = PayloadGen::new(5);
        let mut types = std::collections::HashSet::new();
        for _ in 0..25 {
            let (_, _, event) = parse(&g.next_envelope());
            types.insert(
                event["exception"]["values"][0]["type"]
                    .as_str()
                    .unwrap()
                    .to_string(),
            );
        }
        assert_eq!(types.len(), 5);
    }

    #[test]
    fn payload_size_is_representative() {
        let mut g = PayloadGen::new(100);
        let env = g.next_envelope();
        assert!(env.len() >= 2048, "envelope too small: {}", env.len());
        assert!(env.len() <= 6144, "envelope too large: {}", env.len());
    }
}
