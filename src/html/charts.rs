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
}
