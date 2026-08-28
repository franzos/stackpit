use crate::queries::types::{DailySessions, TransactionTrendPoint};
use serde::Serialize;

/// Payload embedded on a `<canvas data-chart>` for the client-side Chart.js
/// reader (`static/charts.js`). Labels are the pre-formatted bucket captions;
/// values are the per-bucket counts; `name` is the dataset/tooltip label.
#[derive(Serialize)]
struct ChartData<'a> {
    labels: Vec<&'a str>,
    values: Vec<f32>,
    name: &'a str,
}

/// Serialize time-bucketed counts to the JSON the client renders as a bar chart.
/// Returns an empty string when there's nothing to plot so the template can hide
/// the card (`{% if !chart_data.is_empty() %}`).
pub fn chart_json(buckets: &[(String, f32)], name: &str) -> String {
    if buckets.is_empty() {
        return String::new();
    }
    let data = ChartData {
        labels: buckets.iter().map(|(l, _)| l.as_str()).collect(),
        values: buckets.iter().map(|(_, c)| *c).collect(),
        name,
    };
    serde_json::to_string(&data).unwrap_or_default()
}

/// JSON for the release-health "sessions per day" bar chart. Empty string when
/// there's no data.
pub fn session_chart_json(daily: &[DailySessions]) -> String {
    let buckets: Vec<(String, f32)> = daily
        .iter()
        .map(|d| {
            let label = chrono::DateTime::from_timestamp(d.day, 0)
                .map(|dt| dt.format("%b %d").to_string())
                .unwrap_or_default();
            (label, d.total as f32)
        })
        .collect();
    chart_json(&buckets, "Sessions")
}

/// One line of a multi-series chart. `markers` are indices to call out — drawn
/// as enlarged points in the error colour by `static/charts.js`.
#[derive(Serialize)]
struct ChartSeries<'a> {
    name: &'a str,
    values: Vec<f32>,
    markers: Vec<usize>,
}

/// Payload for a multi-series line chart. The client branches on the presence of
/// `series`, so this shape is additive: existing single-series bar charts keep
/// emitting [`ChartData`] and render unchanged.
#[derive(Serialize)]
struct MultiSeriesChartData<'a> {
    labels: Vec<&'a str>,
    series: Vec<ChartSeries<'a>>,
    /// Suffix the client appends in tooltips, so the axis numbers are not
    /// unitless. Kept server-side rather than duplicating `format_duration` in JS.
    unit: &'a str,
    /// When `"duration"`, the client formats Y ticks with the same adaptive
    /// ladder as `format_duration`. The ticks cannot be computed here because
    /// Chart.js picks the scale, so this is the one place the ladder is
    /// mirrored in JS.
    #[serde(skip_serializing_if = "str::is_empty")]
    y_format: &'a str,
}

/// JSON for the transaction summary's percentile trend: p50 and p95 as two
/// lines, with the regression-marked points flagged on the p95 series. Empty
/// string when there is nothing to plot.
pub fn trend_chart_json(
    points: &[TransactionTrendPoint],
    p50_name: &str,
    p95_name: &str,
) -> String {
    if points.is_empty() {
        return String::new();
    }
    let markers = points
        .iter()
        .enumerate()
        .filter(|(_, p)| p.regressed)
        .map(|(i, _)| i)
        .collect();

    let data = MultiSeriesChartData {
        labels: points.iter().map(|p| p.label.as_str()).collect(),
        series: vec![
            ChartSeries {
                name: p50_name,
                values: points.iter().map(|p| p.p50_ms as f32).collect(),
                markers: Vec::new(),
            },
            ChartSeries {
                name: p95_name,
                values: points.iter().map(|p| p.p95_ms as f32).collect(),
                markers,
            },
        ],
        unit: "ms",
        y_format: "duration",
    };
    serde_json::to_string(&data).unwrap_or_default()
}

/// Tiny inline bar chart for an issue row's event trend. Hand-rolled SVG so it
/// stays cheap to render for every row and inherits its color from the
/// surrounding text via `currentColor`. Draws an L-shaped axis (a baseline the
/// bars sit on plus a left Y axis) so the bars read as a chart rather than
/// free-floating ticks. Rendered at its natural pixel size (no CSS stretch) to
/// keep strokes crisp. Returns an empty string when there's nothing to plot.
pub fn render_sparkline(counts: &[f32]) -> String {
    let max = counts.iter().copied().fold(0.0_f32, f32::max);
    if counts.is_empty() || max <= 0.0 {
        return String::new();
    }
    let (bar_w, gap, plot_h) = (3.0_f32, 1.0_f32, 20.0_f32);
    let axis_x = 1.5_f32; // left Y axis sits here; bars start just right of it
    let baseline = plot_h; // X axis baseline the bars stand on
    let h = plot_h + 2.0; // room below the baseline for its stroke
    let plot_w = counts.len() as f32 * (bar_w + gap) - gap;
    let w = axis_x + 0.5 + plot_w + 0.5;

    let mut bars = String::new();
    for (i, &c) in counts.iter().enumerate() {
        if c <= 0.0 {
            continue;
        }
        let bh = (c / max * plot_h).max(1.0);
        let x = axis_x + 0.5 + i as f32 * (bar_w + gap);
        let y = baseline - bh;
        bars.push_str(&format!(
            "<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{bar_w:.1}\" height=\"{bh:.1}\"/>"
        ));
    }
    // Y axis down the left, then X axis baseline across the bottom (one path).
    format!(
        "<svg class=\"spark\" viewBox=\"0 0 {w:.1} {h:.1}\" width=\"{w:.0}\" height=\"{h:.0}\" \
         role=\"img\" aria-hidden=\"true\">\
         <path class=\"spark-axis\" fill=\"none\" d=\"M{axis_x:.1} 0 V{baseline:.1} H{w:.1}\"/>\
         <g class=\"spark-bars\" fill=\"currentColor\">{bars}</g></svg>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chart_json_blank_when_no_buckets() {
        assert_eq!(chart_json(&[], "Events"), "");
    }

    #[test]
    fn chart_json_serializes_labels_values_and_name() {
        let buckets = vec![("Jul 20".to_string(), 3.0), ("Jul 21".to_string(), 5.0)];
        let json = chart_json(&buckets, "Events");
        assert_eq!(
            json,
            r#"{"labels":["Jul 20","Jul 21"],"values":[3.0,5.0],"name":"Events"}"#
        );
    }

    #[test]
    fn session_chart_json_blank_when_empty() {
        assert_eq!(session_chart_json(&[]), "");
    }

    fn point(label: &str, p50: i64, p95: i64, regressed: bool) -> TransactionTrendPoint {
        TransactionTrendPoint {
            bucket: 0,
            label: label.to_string(),
            count: 1,
            p50_ms: p50,
            p95_ms: p95,
            regressed,
        }
    }

    #[test]
    fn trend_chart_json_blank_when_no_points() {
        assert_eq!(trend_chart_json(&[], "p50", "p95"), "");
    }

    #[test]
    fn trend_chart_json_emits_two_series_and_marks_only_p95() {
        let json = trend_chart_json(
            &[
                point("Jul 20", 10, 20, false),
                point("Jul 21", 12, 90, true),
            ],
            "p50",
            "p95",
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["labels"], serde_json::json!(["Jul 20", "Jul 21"]));
        assert_eq!(v["unit"], "ms");
        assert_eq!(v["series"][0]["name"], "p50");
        assert_eq!(v["series"][0]["values"], serde_json::json!([10.0, 12.0]));
        // Markers ride on the p95 series only; p50 is never marked.
        assert_eq!(v["series"][0]["markers"], serde_json::json!([]));
        assert_eq!(v["series"][1]["name"], "p95");
        assert_eq!(v["series"][1]["markers"], serde_json::json!([1]));
        // Without this the Y axis reads `8,000` under points labelled 7.99s.
        assert_eq!(v["y_format"], "duration");
    }

    /// The single-series charts plot counts, so they must not ask the client
    /// to format their axis as a duration.
    #[test]
    fn single_series_payload_carries_no_duration_axis() {
        let json = chart_json(&[("Jul 20".to_string(), 3.0)], "Events");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v.get("y_format").is_none());
    }

    #[test]
    fn session_chart_labels_name_their_month() {
        let json = session_chart_json(&[DailySessions {
            // 2026-08-21T00:00:00Z
            day: 1_787_270_400,
            total: 5,
            crashed: 0,
            errored: 0,
        }]);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["labels"], serde_json::json!(["Aug 21"]));
    }

    // The client branches on `series`, so the single-series shape must not grow it.
    #[test]
    fn single_series_payload_has_no_series_key() {
        let json = chart_json(&[("Jul 20".to_string(), 3.0)], "Events");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v.get("series").is_none());
    }

    #[test]
    fn sparkline_blank_when_no_data() {
        assert_eq!(render_sparkline(&[]), "");
        assert_eq!(render_sparkline(&[0.0, 0.0, 0.0]), "");
    }

    #[test]
    fn sparkline_emits_axes_and_one_rect_per_nonzero_bucket() {
        let svg = render_sparkline(&[0.0, 2.0, 4.0, 0.0]);
        assert!(svg.starts_with("<svg class=\"spark\""));
        // L-shaped axis (baseline + left Y axis) is drawn.
        assert!(svg.contains("class=\"spark-axis\""));
        // Only the two non-zero buckets get a bar.
        assert_eq!(svg.matches("<rect").count(), 2);
        // Tallest bar spans the full plot height; shorter one is proportional.
        assert!(svg.contains("height=\"20.0\""));
        assert!(svg.contains("height=\"10.0\""));
    }
}
