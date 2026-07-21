use crate::queries::types::DailySessions;
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
                .map(|dt| dt.format("%m-%d").to_string())
                .unwrap_or_default();
            (label, d.total as f32)
        })
        .collect();
    chart_json(&buckets, "Sessions")
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
