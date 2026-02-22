use anyhow::Result;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::routing::get;
use axum::{Router, response::IntoResponse};
use prometheus::{
    Encoder, Gauge, GaugeVec, Histogram, HistogramOpts, IntCounter, IntCounterVec, Registry,
    TextEncoder,
};
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, oneshot};
use tokio::task::JoinHandle;

#[derive(Clone)]
pub struct FrontfillMetrics {
    pub current_block_number: Gauge,
    pub logs_total: IntCounter,
    pub logs_by_type_total: IntCounterVec,
    pub logs_per_minute: Gauge,
    pub logs_per_minute_by_type: GaugeVec,
    pub decode_errors_total: IntCounter,
    pub write_errors_total: IntCounter,
    pub missing_metadata_total: IntCounter,
    pub rotate_total: IntCounter,
    pub shutdown_total: IntCounter,
    pub sink_write_seconds: Histogram,
    pub ws_connected: Gauge,
    pub last_log_unix_seconds: Gauge,
    rate_state: Arc<Mutex<RateState>>,
}

struct RateState {
    total: VecDeque<Instant>,
    by_type: HashMap<String, VecDeque<Instant>>,
}

pub struct MetricsRuntime {
    pub metrics: FrontfillMetrics,
    server_handle: MetricsServerHandle,
    rate_handle: RateUpdaterHandle,
}

#[derive(Clone)]
struct HealthState {
    ws_connected: Gauge,
    last_log_unix_seconds: Gauge,
    max_staleness_seconds: u64,
}

#[derive(Clone)]
struct AppState {
    registry: Arc<Registry>,
    health: HealthState,
}

impl MetricsRuntime {
    pub async fn shutdown(self) {
        self.rate_handle.shutdown().await;
        self.server_handle.shutdown().await;
    }
}

pub async fn start_metrics_runtime(bind: &str, port: u16) -> Result<MetricsRuntime> {
    let registry = Arc::new(Registry::new());
    let metrics = FrontfillMetrics::register(registry.as_ref())?;
    let health = HealthState {
        ws_connected: metrics.ws_connected.clone(),
        last_log_unix_seconds: metrics.last_log_unix_seconds.clone(),
        max_staleness_seconds: 120,
    };
    let server_handle = start_metrics_server(registry, health, bind, port).await?;
    let rate_handle = metrics.spawn_rate_updater();

    Ok(MetricsRuntime {
        metrics,
        server_handle,
        rate_handle,
    })
}

impl FrontfillMetrics {
    fn register(registry: &Registry) -> Result<Self> {
        let current_block_number = Gauge::new(
            "polymarket_frontfill_current_block_number",
            "Latest block number observed by frontfill",
        )?;
        let logs_total = IntCounter::new(
            "polymarket_frontfill_logs_total",
            "Total logs observed by frontfill",
        )?;
        let logs_by_type_total = IntCounterVec::new(
            prometheus::Opts::new(
                "polymarket_frontfill_logs_by_type_total",
                "Total decoded logs by event type",
            ),
            &["event_type"],
        )?;
        let logs_per_minute = Gauge::new(
            "polymarket_frontfill_logs_per_minute",
            "Rolling 60-second log rate as logs/min",
        )?;
        let logs_per_minute_by_type = GaugeVec::new(
            prometheus::Opts::new(
                "polymarket_frontfill_logs_per_minute_by_type",
                "Rolling 60-second log rate by event type as logs/min",
            ),
            &["event_type"],
        )?;
        let decode_errors_total = IntCounter::new(
            "polymarket_frontfill_decode_errors_total",
            "Total decode errors in frontfill",
        )?;
        let write_errors_total = IntCounter::new(
            "polymarket_frontfill_write_errors_total",
            "Total sink write errors in frontfill",
        )?;
        let missing_metadata_total = IntCounter::new(
            "polymarket_frontfill_missing_metadata_total",
            "Total logs rejected due to missing block_number, transaction_hash, or log_index",
        )?;
        let rotate_total = IntCounter::new(
            "polymarket_frontfill_rotate_total",
            "Total sink rotations triggered",
        )?;
        let shutdown_total = IntCounter::new(
            "polymarket_frontfill_shutdown_total",
            "Total frontfill shutdown signals handled",
        )?;
        let sink_write_seconds = Histogram::with_opts(HistogramOpts::new(
            "polymarket_frontfill_sink_write_seconds",
            "Sink write latency in seconds",
        ))?;
        let ws_connected = Gauge::new(
            "polymarket_frontfill_ws_connected",
            "WebSocket connection status (1 connected, 0 disconnected)",
        )?;
        let last_log_unix_seconds = Gauge::new(
            "polymarket_frontfill_last_log_unix_seconds",
            "Unix timestamp of last observed log",
        )?;

        registry.register(Box::new(current_block_number.clone()))?;
        registry.register(Box::new(logs_total.clone()))?;
        registry.register(Box::new(logs_by_type_total.clone()))?;
        registry.register(Box::new(logs_per_minute.clone()))?;
        registry.register(Box::new(logs_per_minute_by_type.clone()))?;
        registry.register(Box::new(decode_errors_total.clone()))?;
        registry.register(Box::new(write_errors_total.clone()))?;
        registry.register(Box::new(missing_metadata_total.clone()))?;
        registry.register(Box::new(rotate_total.clone()))?;
        registry.register(Box::new(shutdown_total.clone()))?;
        registry.register(Box::new(sink_write_seconds.clone()))?;
        registry.register(Box::new(ws_connected.clone()))?;
        registry.register(Box::new(last_log_unix_seconds.clone()))?;

        Ok(Self {
            current_block_number,
            logs_total,
            logs_by_type_total,
            logs_per_minute,
            logs_per_minute_by_type,
            decode_errors_total,
            write_errors_total,
            missing_metadata_total,
            rotate_total,
            shutdown_total,
            sink_write_seconds,
            ws_connected,
            last_log_unix_seconds,
            rate_state: Arc::new(Mutex::new(RateState {
                total: VecDeque::new(),
                by_type: HashMap::new(),
            })),
        })
    }

    pub async fn observe_log(&self, block_number: u64) {
        self.logs_total.inc();
        self.current_block_number.set(block_number as f64);
        self.last_log_unix_seconds
            .set(current_unix_timestamp_seconds() as f64);

        let now = Instant::now();
        let mut state = self.rate_state.lock().await;
        state.total.push_back(now);
        prune_and_recalculate_rates(&mut state, self);
    }

    pub async fn observe_decoded_type(&self, event_type: &str) {
        self.logs_by_type_total
            .with_label_values(&[event_type])
            .inc();

        let now = Instant::now();
        let mut state = self.rate_state.lock().await;
        state
            .by_type
            .entry(event_type.to_string())
            .or_default()
            .push_back(now);
        prune_and_recalculate_rates(&mut state, self);
    }

    fn spawn_rate_updater(&self) -> RateUpdaterHandle {
        let rate_state = Arc::clone(&self.rate_state);
        let logs_per_minute = self.logs_per_minute.clone();
        let logs_per_minute_by_type = self.logs_per_minute_by_type.clone();

        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        let task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let mut state = rate_state.lock().await;
                        prune_old_entries(&mut state);
                        logs_per_minute.set(state.total.len() as f64);
                        for (event_type, events) in &state.by_type {
                            logs_per_minute_by_type
                                .with_label_values(&[event_type])
                                .set(events.len() as f64);
                        }
                    }
                    _ = &mut shutdown_rx => {
                        break;
                    }
                }
            }
        });

        RateUpdaterHandle {
            shutdown_tx: Some(shutdown_tx),
            task,
        }
    }
}

fn prune_and_recalculate_rates(state: &mut RateState, metrics: &FrontfillMetrics) {
    prune_old_entries(state);
    metrics.logs_per_minute.set(state.total.len() as f64);
    for (event_type, events) in &state.by_type {
        metrics
            .logs_per_minute_by_type
            .with_label_values(&[event_type])
            .set(events.len() as f64);
    }
}

fn prune_old_entries(state: &mut RateState) {
    let cutoff = Instant::now() - Duration::from_secs(60);
    while let Some(timestamp) = state.total.front() {
        if *timestamp < cutoff {
            state.total.pop_front();
        } else {
            break;
        }
    }

    for entries in state.by_type.values_mut() {
        while let Some(timestamp) = entries.front() {
            if *timestamp < cutoff {
                entries.pop_front();
            } else {
                break;
            }
        }
    }
}

fn current_unix_timestamp_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

struct MetricsServerHandle {
    shutdown_tx: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
}

impl MetricsServerHandle {
    async fn shutdown(mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
        let _ = self.task.await;
    }
}

struct RateUpdaterHandle {
    shutdown_tx: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
}

impl RateUpdaterHandle {
    async fn shutdown(mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
        let _ = self.task.await;
    }
}

async fn start_metrics_server(
    registry: Arc<Registry>,
    health: HealthState,
    bind: &str,
    port: u16,
) -> Result<MetricsServerHandle> {
    let address: SocketAddr = format!("{bind}:{port}").parse()?;
    let listener = tokio::net::TcpListener::bind(address).await?;

    let app = Router::new()
        .route("/metrics", get(metrics_handler))
        .route("/health", get(health_handler))
        .route("/ready", get(ready_handler))
        .with_state(AppState { registry, health });

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let server = axum::serve(listener, app).with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;
        });

        if let Err(error) = server.await {
            tracing::warn!(error = %error, "metrics endpoint stopped with error");
        }
    });

    Ok(MetricsServerHandle {
        shutdown_tx: Some(shutdown_tx),
        task,
    })
}

async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    match render_metrics(&state.registry) {
        Ok((content_type, body)) => {
            let mut headers = HeaderMap::new();
            if let Ok(value) = HeaderValue::from_str(&content_type) {
                headers.insert(header::CONTENT_TYPE, value);
            }
            (StatusCode::OK, headers, body)
        }
        Err(error) => {
            tracing::warn!(error = %error, "failed to render metrics");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                HeaderMap::new(),
                "metrics encode error".as_bytes().to_vec(),
            )
        }
    }
}

fn is_healthy(health: &HealthState) -> bool {
    if health.ws_connected.get() < 0.5 {
        return false;
    }

    let last_log = health.last_log_unix_seconds.get() as u64;
    if last_log == 0 {
        return false;
    }

    let now = current_unix_timestamp_seconds();
    now.saturating_sub(last_log) <= health.max_staleness_seconds
}

async fn health_handler(State(state): State<AppState>) -> impl IntoResponse {
    if is_healthy(&state.health) {
        (StatusCode::OK, "ok")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "unhealthy")
    }
}

async fn ready_handler(State(state): State<AppState>) -> impl IntoResponse {
    if is_healthy(&state.health) {
        (StatusCode::OK, "ready")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "not ready")
    }
}

fn render_metrics(registry: &Registry) -> Result<(String, Vec<u8>)> {
    let metric_families = registry.gather();
    let encoder = TextEncoder::new();
    let mut output = Vec::new();
    encoder.encode(&metric_families, &mut output)?;
    Ok((encoder.format_type().to_string(), output))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn metrics_registry_contains_expected_metric_names() {
        let registry = Registry::new();
        let metrics = FrontfillMetrics::register(&registry).expect("register");
        metrics.observe_log(123).await;
        metrics.observe_decoded_type("OrderFilled").await;

        let (_, body) = render_metrics(&registry).expect("render");
        let body = String::from_utf8(body).expect("utf8");

        assert!(body.contains("polymarket_frontfill_current_block_number"));
        assert!(body.contains("polymarket_frontfill_logs_per_minute"));
        assert!(body.contains("polymarket_frontfill_logs_per_minute_by_type"));
    }

    #[tokio::test]
    async fn logs_per_minute_updates_for_total_and_event_types() {
        let registry = Registry::new();
        let metrics = FrontfillMetrics::register(&registry).expect("register");

        metrics.observe_log(1).await;
        metrics.observe_log(2).await;
        metrics.observe_decoded_type("OrderFilled").await;
        metrics.observe_decoded_type("OrderFilled").await;

        assert_eq!(metrics.logs_per_minute.get(), 2.0);
        assert_eq!(
            metrics
                .logs_per_minute_by_type
                .with_label_values(&["OrderFilled"])
                .get(),
            2.0
        );
    }

    #[tokio::test]
    async fn start_metrics_runtime_rejects_invalid_bind() {
        let result = start_metrics_runtime("not-a-host", 9090).await;
        let error = match result {
            Ok(_) => panic!("expected invalid bind error"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("invalid socket address"));
    }

    #[test]
    fn health_is_false_when_disconnected() {
        let health = HealthState {
            ws_connected: Gauge::new("test_ws_connected", "test gauge").expect("gauge"),
            last_log_unix_seconds: Gauge::new("test_last_log", "test gauge").expect("gauge"),
            max_staleness_seconds: 120,
        };

        health.ws_connected.set(0.0);
        health
            .last_log_unix_seconds
            .set(current_unix_timestamp_seconds() as f64);
        assert!(!is_healthy(&health));
    }

    #[test]
    fn health_is_true_when_connected_and_recent() {
        let health = HealthState {
            ws_connected: Gauge::new("test_ws_connected_ok", "test gauge").expect("gauge"),
            last_log_unix_seconds: Gauge::new("test_last_log_ok", "test gauge").expect("gauge"),
            max_staleness_seconds: 120,
        };

        health.ws_connected.set(1.0);
        health
            .last_log_unix_seconds
            .set(current_unix_timestamp_seconds() as f64);
        assert!(is_healthy(&health));
    }
}
