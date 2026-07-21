use super::utils::sanitize_svg_text;
use crate::queries::types::DailySessions;

/// charts-rs emits a fixed pixel `width`/`height` on the root `<svg>`. Override
/// them with an inline style so the chart fills its container; the existing
/// `viewBox` makes it scale uniformly (height auto-derived, text undistorted).
fn make_responsive(svg: String) -> String {
    svg.replacen("<svg ", "<svg style=\"width:100%;height:auto\" ", 1)
}

/// Bar chart of total sessions per day. `None` when there's no data so the
/// template can hide the card. Mirrors `render_event_chart`'s styling.
pub fn render_session_chart(daily: &[DailySessions]) -> Option<String> {
    if daily.is_empty() {
        return None;
    }
    let buckets: Vec<(String, f32)> = daily
        .iter()
        .map(|d| {
            let label = chrono::DateTime::from_timestamp(d.day, 0)
                .map(|dt| dt.format("%m-%d").to_string())
                .unwrap_or_default();
            (label, d.total as f32)
        })
        .collect();
    render_session_chart_sized(&buckets, 800.0, 250.0).ok()
}

fn render_session_chart_sized(
    buckets: &[(String, f32)],
    width: f32,
    height: f32,
) -> Result<String, Box<dyn std::error::Error>> {
    use charts_rs::{BarChart, THEME_GRAFANA};

    let x_labels: Vec<String> = buckets.iter().map(|(l, _)| sanitize_svg_text(l)).collect();
    let values: Vec<f32> = buckets.iter().map(|(_, c)| *c).collect();

    let mut chart =
        BarChart::new_with_theme(vec![("Sessions", values).into()], x_labels, THEME_GRAFANA);

    chart.width = width;
    chart.height = height;
    chart.margin.left = 20.0;
    chart.margin.right = 20.0;
    chart.margin.top = 20.0;
    chart.margin.bottom = 20.0;
    chart.legend_show = Some(false);
    chart.x_axis_name_rotate = -45.0;
    chart.x_axis_font_size = 10.0;
    chart.series_label_formatter = "{c:.0}".to_string();
    chart.background_color = charts_rs::Color::transparent();

    Ok(make_responsive(chart.svg()?))
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

pub fn render_event_chart(buckets: &[(String, f32)]) -> Result<String, Box<dyn std::error::Error>> {
    render_event_chart_sized(buckets, 800.0, 250.0)
}

pub fn render_event_chart_wide(
    buckets: &[(String, f32)],
) -> Result<String, Box<dyn std::error::Error>> {
    render_event_chart_sized(buckets, 1400.0, 220.0)
}

fn render_event_chart_sized(
    buckets: &[(String, f32)],
    width: f32,
    height: f32,
) -> Result<String, Box<dyn std::error::Error>> {
    use charts_rs::{BarChart, THEME_GRAFANA};

    // Labels come from chrono formatting, but sanitize defensively anyway.
    let x_labels: Vec<String> = buckets.iter().map(|(l, _)| sanitize_svg_text(l)).collect();
    let values: Vec<f32> = buckets.iter().map(|(_, c)| *c).collect();

    let mut chart =
        BarChart::new_with_theme(vec![("Events", values).into()], x_labels, THEME_GRAFANA);

    chart.width = width;
    chart.height = height;
    chart.margin.left = 20.0;
    chart.margin.right = 20.0;
    chart.margin.top = 20.0;
    chart.margin.bottom = 20.0;
    chart.legend_show = Some(false);
    chart.x_axis_name_rotate = -45.0;
    chart.x_axis_font_size = 10.0;
    chart.series_label_formatter = "{c:.0}".to_string();
    // Transparent background lets the card supply the color (works in light and dark modes).
    chart.background_color = charts_rs::Color::transparent();

    Ok(make_responsive(chart.svg()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_responsive_injects_style_and_keeps_viewbox() {
        let out = make_responsive(
            "<svg width=\"1400\" height=\"220\" viewBox=\"0 0 1400 220\">x</svg>".into(),
        );
        assert!(out.starts_with("<svg style=\"width:100%;height:auto\" "));
        assert!(out.contains("viewBox=\"0 0 1400 220\""));
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
