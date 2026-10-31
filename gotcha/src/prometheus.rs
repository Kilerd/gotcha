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
use axum_prometheus::{EndpointLabel, PrometheusMetricLayerBuilder};

use crate::{GotchaError, GotchaResult};

pub use axum_prometheus::metrics::*;

/// Endpoint label reported for requests without a matched route template.
const UNMATCHED_ENDPOINT: &str = "<unmatched>";

/// HTTP path for the explicitly enabled Prometheus metrics endpoint.
///
/// Applications expose no metrics endpoint by default. Enable it with
/// [`Gotcha::metrics`](crate::Gotcha::metrics) or by returning it from
/// [`GotchaApp::metrics_endpoint`](crate::GotchaApp::metrics_endpoint), and set [`path`](Self::path)
/// to serve it somewhere other than `/metrics`.
///
/// The path must be a literal, absolute URI path without a query, fragment, or `.` / `..`
/// segment; invalid configuration returns an error during router assembly. A path that is also
/// an enabled OpenAPI documentation endpoint is reported as an assembly error. Collisions with
/// business routes follow Axum's usual route-conflict behavior (panic).
///
/// The metric layer covers the business routes registered through the application's route
/// methods, including nested routers. Documentation endpoints, the metrics endpoint itself, and
/// routes or services mounted by `finish_router` are not measured; middleware added with
/// `app_layer` runs outside the layer, so requests it short-circuits are not counted either.
///
/// Matched routes are labelled with their route template (`/users/{id}`), never with the concrete
/// URL. Requests that match no route — unmatched paths, a custom fallback, and routes mounted
/// with `nest_service` — share one `<unmatched>` label instead of producing a label per URL, so
/// scanning for random paths cannot grow the metric registry and raw URLs stay out of metrics.
///
/// [`layer`](Self::layer) is the building block used by the framework, and the way for a custom
/// [`GotchaApp::build_router`](crate::GotchaApp::build_router) override to install metrics.
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
    /// Install the metric layer over every route already registered in `router`, then mount the
    /// render route at [`path`](Self::path).
    ///
    /// The framework calls this while assembling; a custom
    /// [`GotchaApp::build_router`](crate::GotchaApp::build_router) override can call it too, on
    /// the router holding the application's business routes. Mount documentation routes after
    /// this call so their requests are not measured; the metrics route itself is mounted after
    /// the layer and is never measured. Avoid registering another route at
    /// [`path`](Self::path): Axum panics on the conflict.
    ///
    /// The `metrics` recorder is process-wide (see [`prometheus`](crate::prometheus)): the first
    /// call in the process installs it and later calls reuse it. An invalid path, or a
    /// pre-existing foreign `metrics` recorder, returns an error.
    ///
    /// ```
    /// use gotcha::axum::routing::get;
    /// use gotcha::axum::Router;
    /// use gotcha::MetricsEndpoint;
    ///
    /// # async fn example() -> gotcha::GotchaResult<()> {
    /// let router = Router::new().route("/health", get(|| async { "ok" }));
    /// let router = MetricsEndpoint::default().layer(router)?;
    /// # Ok(()) }
    /// ```
    pub fn layer(self, router: Router) -> GotchaResult<Router> {
        let installed = self.install()?;
        let router = router.layer(installed.layer.clone());
        Ok(installed.mount(router))
    }

    fn install(self) -> GotchaResult<InstalledMetrics> {
        crate::endpoint::validate_path("metrics endpoint", &self.path)?;
        let handle = recorder()?;
        let (layer, _) = PrometheusMetricLayerBuilder::new()
            .with_endpoint_label_type(EndpointLabel::MatchedPathWithFallbackFn(|_| UNMATCHED_ENDPOINT.to_owned()))
            .with_metrics_from_fn(|| handle.clone())
            .build_pair();
        Ok(InstalledMetrics { endpoint: self, layer, handle })
    }

    #[cfg(feature = "openapi")]
    pub(crate) fn ensure_no_openapi_conflict(&self, endpoints: &crate::OpenApiEndpoints) -> GotchaResult<()> {
        let documentation = [
            Some(endpoints.json_path.as_str()),
            endpoints.redoc_path.as_deref(),
            endpoints.scalar_path.as_deref(),
        ];
        if documentation.into_iter().flatten().any(|path| path == self.path) {
            return Err(GotchaError::message(format!(
                "metrics endpoint path conflicts with an enabled OpenAPI documentation endpoint: {:?}",
                self.path
            )));
        }
        Ok(())
    }
}

/// The metric layer, recorder handle, and path chosen for one assembled application.
struct InstalledMetrics {
    endpoint: MetricsEndpoint,
    layer: axum_prometheus::PrometheusMetricLayer<'static>,
    handle: PrometheusHandle,
}

impl InstalledMetrics {
    // The layer is installed first; mounting afterwards keeps scrapes out of the metrics.
    fn mount(self, router: Router) -> Router {
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
/// Upkeep runs on one dedicated thread, started with the recorder and independent of any Tokio
/// runtime, so it outlives test runtimes and is started even when the first install happens
/// outside a runtime. It keeps histogram samples bounded when nothing scrapes the endpoint;
/// rendering also drains histograms on each scrape.
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
            let upkeep = handle.clone();
            if let Err(error) = std::thread::Builder::new().name("gotcha-metrics-upkeep".into()).spawn(move || loop {
                std::thread::sleep(Duration::from_secs(5));
                upkeep.run_upkeep();
            }) {
                tracing::warn!("failed to start the Prometheus metrics upkeep thread: {error}");
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
