//! Metric payload parsing (JSON buckets + statsd lines) -> normalized rows.

use crate::ingest::text::clean_text;

/// (mri, metric_type, value, tags, values_json, bucket_timestamp).
pub type MetricRow = (
    String,
    String,
    f64,
    Option<String>,
    Option<String>,
    Option<i64>,
);

const MAX_METRIC_ENTRIES: usize = 10_000;

/// Parse a metric payload into normalized rows. `bucket_timestamp` is `Some`
/// when the bucket includes its own timestamp.
pub fn parse_metric_payload(payload: &[u8]) -> Vec<MetricRow> {
    let mut rows = parse_metric_rows(payload);
    for (mri, metric_type, ..) in &mut rows {
        *mri = clean_text(mri);
        *metric_type = clean_text(metric_type);
    }
    rows
}

fn parse_metric_rows(payload: &[u8]) -> Vec<MetricRow> {
    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(payload) {
        if let Some(arr) = json.as_array() {
            return arr
                .iter()
                .take(MAX_METRIC_ENTRIES)
                .filter_map(parse_metric_bucket)
                .collect();
        }

        let mri = json
            .get("mri")
            .or_else(|| json.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let metric_type = json
            .get("type")
            .or_else(|| json.get("ty"))
            .and_then(|v| v.as_str())
            .or_else(|| match mri.chars().next() {
                Some('c') => Some("counter"),
                Some('d') => Some("distribution"),
                Some('g') => Some("gauge"),
                Some('s') => Some("set"),
                _ => None,
            })
            .unwrap_or("counter")
            .to_string();

        let (value, values_json) = extract_metric_value(json.get("value"));
        let tags = json.get("tags").map(|t| t.to_string());

        return vec![(mri, metric_type, value, tags, values_json, None)];
    }

    let text = match std::str::from_utf8(payload) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    text.lines()
        .filter(|line| !line.trim().is_empty())
        .take(MAX_METRIC_ENTRIES)
        .map(parse_statsd_line)
        .collect()
}

fn extract_metric_value(v: Option<&serde_json::Value>) -> (f64, Option<String>) {
    match v {
        Some(val) => {
            if let Some(arr) = val.as_array() {
                let values_json = serde_json::to_string(arr).ok();
                let sum: f64 = arr.iter().filter_map(|v| v.as_f64()).sum();
                (sum, values_json)
            } else {
                (val.as_f64().unwrap_or(0.0), None)
            }
        }
        None => (0.0, None),
    }
}

fn parse_metric_bucket(bucket: &serde_json::Value) -> Option<MetricRow> {
    let name = bucket.get("name").and_then(|v| v.as_str())?;
    let unit = bucket
        .get("unit")
        .and_then(|v| v.as_str())
        .unwrap_or("none");
    let metric_type = bucket
        .get("type")
        .or_else(|| bucket.get("ty"))
        .and_then(|v| v.as_str())
        .unwrap_or("counter");

    // Sentry MRI format: {type}:{namespace}/{name}@{unit}
    // If the name already contains a slash (namespace/metric_name), use as-is;
    // otherwise prefix with "custom/" to match Sentry relay normalization.
    let qualified_name = if name.contains('/') {
        name.to_string()
    } else {
        format!("custom/{name}")
    };
    let mri = format!("{metric_type}:{qualified_name}@{unit}");

    let bucket_ts = bucket.get("timestamp").and_then(|v| v.as_i64());
    let (value, values_json) = extract_metric_value(bucket.get("value"));
    let tags = bucket.get("tags").map(|t| t.to_string());

    Some((
        mri,
        metric_type.to_string(),
        value,
        tags,
        values_json,
        bucket_ts,
    ))
}

fn parse_statsd_line(line: &str) -> MetricRow {
    let (name_unit, rest) = match line.split_once(':') {
        Some(parts) => parts,
        None => {
            return (
                line.to_string(),
                "counter".to_string(),
                0.0,
                None,
                None,
                None,
            )
        }
    };

    let (name, unit) = match name_unit.split_once('@') {
        Some((n, u)) => (n, u),
        None => (name_unit, "none"),
    };

    let (value_type, tags_part) = match rest.split_once("|#") {
        Some((vt, tags)) => (vt, Some(tags)),
        None => (rest, None),
    };

    let (value_str, type_str) = match value_type.split_once('|') {
        Some((v, t)) => (v, t),
        None => (value_type, "c"),
    };

    let values: Vec<f64> = value_str
        .split(':')
        .filter_map(|v| v.parse().ok())
        .collect();
    let value: f64 = values.iter().sum();
    let values_json = if values.len() > 1 {
        serde_json::to_string(&values).ok()
    } else {
        None
    };

    let metric_type = match type_str {
        "c" => "counter",
        "g" => "gauge",
        "d" => "distribution",
        "s" => "set",
        "ms" => "distribution",
        other => other,
    }
    .to_string();

    let qualified_name = if name.contains('/') {
        name.to_string()
    } else {
        format!("custom/{name}")
    };
    let mri = format!("{metric_type}:{qualified_name}@{unit}");

    let tags = tags_part.map(|t| {
        let tag_map: serde_json::Map<String, serde_json::Value> = t
            .split(',')
            .filter_map(|pair| {
                let (k, v) = pair.split_once(':')?;
                Some((k.to_string(), serde_json::Value::String(v.to_string())))
            })
            .collect();
        serde_json::Value::Object(tag_map).to_string()
    });

    (mri, metric_type, value, tags, values_json, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_metric_payload_ignores_zstd_compressed_input() {
        let json = br#"[{"name":"requests","type":"c","value":1.0}]"#;
        assert_eq!(parse_metric_payload(json).len(), 1);

        let compressed = zstd::encode_all(json.as_slice(), 3).unwrap();
        assert!(parse_metric_payload(&compressed).is_empty());
    }

    #[test]
    fn mri_and_type_are_cleaned_on_json_and_statsd_paths() {
        use crate::ingest::text::MAX_TEXT_CHARS;
        let long = "x".repeat(5000);
        let assert_clean = |path: &str, row: &MetricRow, mri_prefix: &str, ty_prefix: &str| {
            let (mri, ty) = (&row.0, &row.1);
            assert!(
                !mri.contains('\0') && !ty.contains('\0'),
                "{path}: NUL kept"
            );
            assert_eq!(
                mri.chars().count(),
                MAX_TEXT_CHARS,
                "{path}: mri not capped"
            );
            assert!(mri.starts_with(mri_prefix), "{path}: mri {mri:.40}");
            assert!(
                ty.chars().count() <= MAX_TEXT_CHARS,
                "{path}: type not capped"
            );
            assert!(ty.starts_with(ty_prefix), "{path}: type {ty:.40}");
        };

        let bucket = format!(r#"[{{"name":"req\u0000{long}","type":"c\u0000{long}"}}]"#);
        let rows = parse_metric_payload(bucket.as_bytes());
        assert_clean("bucket", &rows[0], "c", "c");

        let single = format!(r#"{{"mri":"c:custom/req\u0000{long}","type":"g\u0000{long}"}}"#);
        let rows = parse_metric_payload(single.as_bytes());
        assert_clean("single", &rows[0], "c:custom/req", "g");

        let statsd = format!("req\0{long}:1|x\0{long}");
        let rows = parse_metric_payload(statsd.as_bytes());
        assert_clean("statsd", &rows[0], "x", "x");

        let rows = parse_metric_payload(b"api.requests@second:1|c");
        assert_eq!(rows[0].0, "counter:custom/api.requests@second");
        assert_eq!(rows[0].1, "counter");
    }
}
