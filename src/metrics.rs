use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use prometheus::{Encoder, IntCounterVec, IntGaugeVec, Opts, Registry, TextEncoder};
use tokio::net::TcpListener;

use crate::storage::{Storage, SyncRun};

#[derive(Clone)]
pub struct Metrics {
    runs: IntCounterVec,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            runs: IntCounterVec::new(
                Opts::new(
                    "ddnser_sync_runs_total",
                    "Completed sync runs in this process",
                ),
                &["source", "result"],
            )
            .expect("valid metric definition"),
        }
    }

    pub fn observe_sync(&self, run: &SyncRun) {
        let result = if run.failed == 0 && run.error.is_none() {
            "success"
        } else {
            "failure"
        };
        let result = result.to_string();
        self.runs.with_label_values(&[&run.source, &result]).inc();
    }

    pub async fn serve(
        self: Arc<Self>,
        storage: Arc<Storage>,
        listener: TcpListener,
    ) -> std::io::Result<()> {
        let app = Router::new()
            .route("/metrics", get(metrics))
            .with_state(MetricsState {
                metrics: self,
                storage,
            });
        axum::serve(listener, app).await
    }
}

#[derive(Clone)]
struct MetricsState {
    metrics: Arc<Metrics>,
    storage: Arc<Storage>,
}

async fn metrics(State(state): State<MetricsState>) -> Response {
    let latest = match state.storage.latest_sync_run().await {
        Ok(run) => run,
        Err(error) => {
            tracing::error!(%error, "failed to read sync metrics");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    match encode(&state.metrics, latest.as_ref()) {
        Ok(body) => ([(header::CONTENT_TYPE, "text/plain; version=0.0.4")], body).into_response(),
        Err(error) => {
            tracing::error!(%error, "failed to encode metrics");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn encode(metrics: &Metrics, latest: Option<&SyncRun>) -> Result<String, prometheus::Error> {
    let registry = Registry::new();
    registry.register(Box::new(metrics.runs.clone()))?;
    if let Some(run) = latest {
        let finished = prometheus::Gauge::with_opts(Opts::new(
            "ddnser_sync_last_finished_timestamp_seconds",
            "Unix timestamp of the latest completed sync",
        ))?;
        let success = prometheus::IntGauge::with_opts(Opts::new(
            "ddnser_sync_last_success",
            "Whether the latest sync succeeded (1) or failed (0)",
        ))?;
        let duration = prometheus::Gauge::with_opts(Opts::new(
            "ddnser_sync_last_duration_seconds",
            "Duration of the latest sync",
        ))?;
        let records = IntGaugeVec::new(
            Opts::new(
                "ddnser_sync_records",
                "Records by result in the latest sync",
            ),
            &["result"],
        )?;
        finished.set(run.finished_at.timestamp_millis() as f64 / 1000.0);
        success.set((run.failed == 0 && run.error.is_none()) as i64);
        duration.set((run.finished_at - run.started_at).num_milliseconds() as f64 / 1000.0);
        records
            .with_label_values(&["created"])
            .set(run.created as i64);
        records
            .with_label_values(&["updated"])
            .set(run.updated as i64);
        records
            .with_label_values(&["unchanged"])
            .set(run.unchanged as i64);
        records
            .with_label_values(&["failed"])
            .set(run.failed as i64);
        registry.register(Box::new(finished))?;
        registry.register(Box::new(success))?;
        registry.register(Box::new(duration))?;
        registry.register(Box::new(records))?;
    }
    let mut output = Vec::new();
    TextEncoder::new().encode(&registry.gather(), &mut output)?;
    Ok(String::from_utf8(output).expect("Prometheus output is UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn empty_registry_has_no_sync_samples() {
        assert!(
            !encode(&Metrics::new(), None)
                .unwrap()
                .contains("ddnser_sync_")
        );
    }

    #[test]
    fn latest_run_exposes_only_aggregate_values() {
        let run = SyncRun {
            source: "webhook".into(),
            started_at: Utc.timestamp_opt(100, 0).unwrap(),
            finished_at: Utc.timestamp_opt(102, 500_000_000).unwrap(),
            ipv4: Some("198.51.100.1".into()),
            created: 1,
            updated: 2,
            unchanged: 3,
            failed: 0,
            error: None,
        };
        let metrics = Metrics::new();
        metrics.observe_sync(&run);
        let output = encode(&metrics, Some(&run)).unwrap();
        assert!(output.contains("ddnser_sync_last_success 1"));
        assert!(output.contains("ddnser_sync_records{result=\"updated\"} 2"));
        assert!(output.contains("ddnser_sync_runs"));
        assert!(output.contains("result=\"success\""));
        assert!(output.contains("source=\"webhook\""));
        assert!(!output.contains("198.51.100.1"));
    }
}
