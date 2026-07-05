use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};

use crate::commercial::license::FeatureStatus;

const LATENCY_BUCKETS: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

pub fn install_metrics_recorder() -> PrometheusHandle {
    let recorder = PrometheusBuilder::new()
        .set_buckets_for_metric(Matcher::Suffix("_seconds".to_string()), LATENCY_BUCKETS)
        .expect("valid histogram buckets")
        .build_recorder();
    let handle = recorder.handle();
    if metrics::set_global_recorder(recorder).is_err() {
        tracing::warn!("metrics recorder already installed; /metrics may render stale data");
    }
    handle
}

pub fn scrape_allowed(status: FeatureStatus) -> bool {
    matches!(status, FeatureStatus::Allowed | FeatureStatus::GraceReadOnly)
}

pub fn record_bridged_metrics(accepted: u64, rejected: u64, dropped: u64) {
    metrics::counter!("stackpit_events_accepted_total").absolute(accepted);
    metrics::counter!("stackpit_events_rejected_total").absolute(rejected);
    metrics::counter!("stackpit_events_dropped_total").absolute(dropped);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commercial::license::FeatureStatus;
    use metrics_exporter_prometheus::PrometheusBuilder;

    #[test]
    fn scrape_allowed_matrix() {
        assert!(scrape_allowed(FeatureStatus::Allowed));
        assert!(scrape_allowed(FeatureStatus::GraceReadOnly));
        assert!(!scrape_allowed(FeatureStatus::Locked));
    }

    #[test]
    fn bridged_metrics_render() {
        let recorder = PrometheusBuilder::new().build_recorder();
        let handle = recorder.handle();
        metrics::with_local_recorder(&recorder, || {
            record_bridged_metrics(10, 2, 1);
        });
        let out = handle.render();
        assert!(out.contains("stackpit_events_accepted_total 10"), "got:\n{out}");
        assert!(out.contains("stackpit_events_rejected_total 2"), "got:\n{out}");
        assert!(out.contains("stackpit_events_dropped_total 1"), "got:\n{out}");
    }
}
