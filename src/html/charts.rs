use super::utils::{sanitize_svg_output, sanitize_svg_text};

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

    // These labels come from chrono formatting so they should be clean,
    // but I'd rather sanitize them than find out the hard way.
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

    let svg = chart.svg()?;

    // charts-rs shouldn't produce script tags, but this SVG goes straight
    // into the template with |safe -- so let's strip them just in case.
    let sanitized = sanitize_svg_output(&svg);

    Ok(sanitized)
}
