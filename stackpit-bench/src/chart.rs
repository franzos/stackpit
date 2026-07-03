use crate::stats::SecondBucket;
use anyhow::Result;
use charts_rs::{Box as ChartBox, LineChart, Symbol, THEME_GRAFANA};
use std::collections::BTreeMap;
use std::path::Path;

const BIN_SECONDS: u64 = 5;

pub fn render_chart(
    buckets: &BTreeMap<u64, SecondBucket>,
    backend: &str,
    subtitle: &str,
    out_path: &Path,
) -> Result<()> {
    let mut bins: BTreeMap<u64, (u64, u64, u64, u64)> = BTreeMap::new();
    for (s, b) in buckets {
        if b.scheduled == 0 {
            continue;
        }
        let e = bins.entry(s / BIN_SECONDS).or_insert((0, 0, 0, 0));
        e.0 = e.0.max(b.target);
        e.1 += b.ok;
        e.2 += b.persisted;
        e.3 += 1;
    }

    let mut x_labels = Vec::new();
    let mut target = Vec::new();
    let mut accepted = Vec::new();
    let mut persisted = Vec::new();
    for (bin, (t, ok, p, n)) in &bins {
        let start = bin * BIN_SECONDS;
        x_labels.push(if start.is_multiple_of(30) {
            format!("{start}s")
        } else {
            String::new()
        });
        target.push(*t as f32);
        accepted.push(*ok as f32 / *n as f32);
        persisted.push(*p as f32 / *n as f32);
    }

    let mut chart = LineChart::new_with_theme(
        vec![
            ("target", target).into(),
            ("accepted", accepted).into(),
            ("persisted", persisted).into(),
        ],
        x_labels,
        THEME_GRAFANA,
    );
    chart.width = 1000.0;
    chart.height = 420.0;
    chart.title_text = format!("Stackpit ingestion ({backend}): events/sec");
    chart.sub_title_text = subtitle.to_string();
    chart.legend_show = Some(true);
    chart.legend_margin = Some(ChartBox {
        top: 50.0,
        bottom: 10.0,
        ..Default::default()
    });
    chart.series_symbol = Some(Symbol::None);
    chart.x_axis_font_size = 10.0;

    let svg = chart
        .svg()
        .map_err(|e| anyhow::anyhow!("chart render: {e}"))?;
    std::fs::write(out_path, svg)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::SecondBucket;
    use std::collections::BTreeMap;

    #[test]
    fn renders_svg_with_three_series() {
        let mut buckets: BTreeMap<u64, SecondBucket> = BTreeMap::new();
        for s in 0..90u64 {
            buckets.insert(
                s,
                SecondBucket {
                    phase: "ramp",
                    target: 250 + s * 10,
                    ok: 240 + s * 10,
                    persisted: 230 + s * 10,
                    scheduled: 250 + s * 10,
                    ..Default::default()
                },
            );
        }
        buckets.insert(
            120,
            SecondBucket {
                phase: "soak",
                target: 0,
                scheduled: 0,
                persisted: 4321,
                ..Default::default()
            },
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bench.svg");
        render_chart(&buckets, "PostgreSQL", "soak 4500/s from t=60s", &path).unwrap();
        let svg = std::fs::read_to_string(&path).unwrap();
        assert!(svg.starts_with("<svg") || svg.contains("<svg"));
        assert!(svg.contains("Stackpit ingestion (PostgreSQL): events/sec"));
        assert!(svg.contains("target"));
        assert!(svg.contains("accepted"));
        assert!(svg.contains("persisted"));
        assert!(svg.contains("soak 4500/s"));
        assert!(svg.contains("60s"));
        assert!(
            !svg.contains("120s"),
            "send-free second 120 must not contribute to any bin"
        );
    }
}
