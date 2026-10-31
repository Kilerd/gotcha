//! Prometheus metrics: the opt-in HTTP endpoint and the `metrics` recorder re-exports.
//!
//! Enabling the `prometheus` feature on its own only makes [`axum_prometheus`]'s `metrics` crate
//! available (re-exported below); nothing is recorded and no endpoint is mounted. Opt in through
//! [`MetricsEndpoint`] — the builder's [`Gotcha::metrics`](crate::Gotcha::metrics) or
//! [`GotchaApp::metrics_endpoint`](crate::GotchaApp::metrics_endpoint) — to install the metric
//! layer over the application's business routes and serve the Prometheus text format at its path.
//!
//! # Process-wide recorder
//!
//! The `metrics` crate has a single global recorder per process. The first application assembled
//! with metrics enabled installs it; later applications in the same process reuse the recorder and
//! its handle. Metrics are therefore process-wide: counters, gauges, and histograms from every
//! application are rendered by every enabled endpoint, and custom metrics recorded through the
//! re-exported macros are included too. The recorder cannot be replaced afterwards; if another
//! `metrics` recorder was installed before the first assembly, enabling metrics fails with an
//! error instead of panicking. Registering the endpoint a second time in one process is supported
//! (for example across tests) and shares the first recorder.

use std::sync::OnceLock;
use std::time::Duration;

use axum::http::header;
use axum::routing::get;
use axum::Router;
use axum_prometheus::metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use axum_prometheus::PrometheusMetricLayerBuilder;

use crate::{GotchaError, GotchaResult};

pub use axum_prometheus::metrics::*;

/// HTTP path for the explicitly enabled Prometheus metrics endpoint.
///
/// Applications expose no metrics endpoint by default. Enable it with
/// [`Gotcha::metrics`](crate::Gotcha::metrics) or by returning it from
/// [`GotchaApp::metrics_endpoint`](crate::GotchaApp::metrics_endpoint), and set [`path`](Self::path)
/// to serve it somewhere other than `/metrics`.
///
/// The path must be a literal, absolute URI path without a query, fragment, or `.` / `..`
/// segment; invalid configuration returns an error during router assembly. Collisions with
/// existing routes follow Axum's usual route-conflict behavior (panic).
///
/// The metric layer covers the business routes registered through the application's route
/// methods, including nested routers. Documentation endpoints, the metrics endpoint itself, and
/// routes or services mounted by `finish_router` are not measured; middleware added with
/// `app_layer` runs outside the layer, so requests it short-circuits are not counted either.
///
/// ```
/// use gotcha::{Gotcha, MetricsEndpoint};
///
/// let app = Gotcha::new().metrics(MetricsEndpoint { path: "/internal/metrics".into() });
/// ```
#[derive(Clone, Debug)]
pub struct MetricsEndpoint {
    /// Path serving the Prometheus text exposition format. Defaults to `/metrics`.
    pub path: String,
}

impl Default for MetricsEndpoint {
    fn default() -> Self {
        Self { path: "/metrics".into() }
    }
}

impl MetricsEndpoint {
    pub(crate) fn install(self) -> GotchaResult<InstalledMetrics> {
        crate::endpoint::validate_path("metrics endpoint", &self.path)?;
        let handle = recorder()?;
        let (layer, _) = PrometheusMetricLayerBuilder::new().with_metrics_from_fn(|| handle.clone()).build_pair();
        Ok(InstalledMetrics { endpoint: self, layer, handle })
    }
}

/// The metric layer, recorder handle, and path chosen for one assembled application.
pub(crate) struct InstalledMetrics {
    endpoint: MetricsEndpoint,
    pub(crate) layer: axum_prometheus::PrometheusMetricLayer<'static>,
    handle: PrometheusHandle,
}

impl InstalledMetrics {
    /// Mount the render route. Called after the layer is installed on the business routes so a
    /// scrape is not itself recorded.
    pub(crate) fn mount(self, router: Router) -> Router {
        let handle = self.handle;
        router.route(
            &self.endpoint.path,
            get(move || {
                let handle = handle.clone();
                async move { ([(header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")], handle.render()) }
            }),
        )
    }
}

/// The process-wide recorder, installed on first use and shared by every application afterwards.
///
/// The exporter's upkeep runs in the runtime that first installs the recorder, keeping histogram
/// samples bounded when nothing scrapes the endpoint. In a process without a Tokio runtime it is
/// skipped; rendering still works and drains histograms on each scrape.
fn recorder() -> GotchaResult<PrometheusHandle> {
    static RECORDER: OnceLock<Result<PrometheusHandle, String>> = OnceLock::new();
    RECORDER
        .get_or_init(|| {
            let recorder = PrometheusBuilder::new()
                .set_buckets_for_metric(
                    Matcher::Full(axum_prometheus::utils::requests_duration_name().to_owned()),
                    axum_prometheus::utils::SECONDS_DURATION_BUCKETS,
                )
                .map_err(|error| error.to_string())?
                .build_recorder();
            let handle = recorder.handle();
            axum_prometheus::metrics::set_global_recorder(recorder).map_err(|error| format!("failed to install the Prometheus metrics recorder: {error}"))?;
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                let upkeep = handle.clone();
                runtime.spawn(async move {
                    loop {
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        upkeep.run_upkeep();
                    }
                });
            }
            Ok(handle)
        })
        .clone()
        .map_err(GotchaError::message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_path_is_a_literal_absolute_uri_path() {
        // Validate through the shared helper so this test does not install the process-wide
        // recorder; `install` itself is exercised by the integration tests.
        crate::endpoint::validate_path("metrics endpoint", &MetricsEndpoint::default().path).unwrap();
        for path in ["/", "/internal/metrics", "/metrics.json"] {
            crate::endpoint::validate_path("metrics endpoint", path).unwrap();
        }
        for path in ["", "metrics", "/metrics?x=1", "/metrics#part", "/docs/../metrics", "/{id}"] {
            let error = crate::endpoint::validate_path("metrics endpoint", path).unwrap_err();
            assert!(error.to_string().contains("metrics endpoint"), "{path:?}: {error}");
        }
    }
}
