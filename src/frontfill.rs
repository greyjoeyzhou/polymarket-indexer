use alloy::providers::{Provider, ProviderBuilder, WsConnect};
use alloy::rpc::types::{BlockNumberOrTag, Filter};
use alloy::transports::Authorization;
use anyhow::{Result, anyhow};
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{Duration, sleep};
use tracing::info;

use crate::contracts::contract_addresses;
use crate::decode::decode_log;
use crate::metrics::start_metrics_runtime;
#[cfg(feature = "kafka")]
use crate::storage::KafkaStorage;
use crate::storage::{EventSink, ParquetStorage};

type SharedSink = Arc<Mutex<Box<dyn EventSink>>>;

pub enum SinkConfig {
    Parquet {
        output_dir: String,
    },
    Kafka {
        brokers: Option<String>,
        topic_prefix: String,
    },
}

pub struct FrontfillConfig {
    pub flush_blocks: u64,
    pub start_block: Option<u64>,
    pub rpc_url: String,
    pub rpc_auth_key: Option<String>,
    pub rpc_auth_header: String,
    pub rpc_auth_scheme: String,
    pub metrics_enabled: bool,
    pub metrics_bind: String,
    pub metrics_port: u16,
    pub sink: SinkConfig,
}

fn build_filter(start_block: Option<u64>) -> Result<Filter> {
    let filter = Filter::new().address(contract_addresses()?);
    Ok(match start_block {
        Some(block) => filter.from_block(BlockNumberOrTag::Number(block)),
        None => filter.from_block(BlockNumberOrTag::Latest),
    })
}

fn reconnect_delay(attempt: u32) -> Duration {
    let shift = attempt.min(5);
    Duration::from_secs(1_u64 << shift)
}

fn build_ws_auth(
    key: Option<&str>,
    header_name: &str,
    scheme: &str,
) -> Result<Option<Authorization>> {
    let Some(key) = key else {
        return Ok(None);
    };

    if key.is_empty() {
        return Ok(None);
    }

    if !header_name.eq_ignore_ascii_case("authorization") {
        return Err(anyhow!(
            "frontfill websocket auth supports only the Authorization header"
        ));
    }

    if scheme.is_empty() {
        return Ok(Some(Authorization::raw(key)));
    }

    if scheme.eq_ignore_ascii_case("bearer") {
        return Ok(Some(Authorization::bearer(key)));
    }

    Ok(Some(Authorization::raw(format!("{scheme} {key}"))))
}

fn build_storage(sink: SinkConfig) -> Result<SharedSink> {
    Ok(match sink {
        SinkConfig::Parquet { output_dir } => {
            Arc::new(Mutex::new(Box::new(ParquetStorage::new(&output_dir))))
        }
        SinkConfig::Kafka {
            brokers,
            topic_prefix,
        } => {
            #[cfg(feature = "kafka")]
            {
                let brokers = brokers
                    .as_ref()
                    .ok_or_else(|| anyhow!("kafka sink requires --kafka-brokers"))?;
                Arc::new(Mutex::new(Box::new(KafkaStorage::new(
                    brokers,
                    &topic_prefix,
                )?)))
            }

            #[cfg(not(feature = "kafka"))]
            {
                let _ = brokers;
                let _ = topic_prefix;
                return Err(anyhow!("kafka sink requires the 'kafka' feature flag"));
            }
        }
    })
}

fn build_ws_connect(
    rpc_url: &str,
    rpc_auth_key: Option<&str>,
    rpc_auth_header: &str,
    rpc_auth_scheme: &str,
) -> Result<WsConnect> {
    let auth = build_ws_auth(rpc_auth_key, rpc_auth_header, rpc_auth_scheme)?;
    let mut ws = WsConnect::new(rpc_url);
    if let Some(auth) = auth {
        ws = ws.with_auth(auth);
    }
    Ok(ws)
}

pub async fn run(config: FrontfillConfig) -> Result<()> {
    if config.flush_blocks == 0 {
        return Err(anyhow!("flush-blocks must be greater than 0"));
    }

    info!(
        flush_blocks = config.flush_blocks,
        start_block = ?config.start_block,
        rpc_url = %config.rpc_url,
        "Runtime configuration loaded"
    );

    let storage = build_storage(config.sink)?;

    let mut metrics_runtime = if config.metrics_enabled {
        Some(start_metrics_runtime(&config.metrics_bind, config.metrics_port).await?)
    } else {
        None
    };

    if let Some(runtime) = metrics_runtime.as_ref() {
        runtime.metrics.ws_connected.set(0.0);
    }
    let mut next_rotation_block: Option<u64> = None;

    let mut reconnect_attempt = 0_u32;
    loop {
        let ws = build_ws_connect(
            &config.rpc_url,
            config.rpc_auth_key.as_deref(),
            &config.rpc_auth_header,
            &config.rpc_auth_scheme,
        )?;

        let provider = match ProviderBuilder::new().on_ws(ws).await {
            Ok(provider) => provider,
            Err(error) => {
                let delay = reconnect_delay(reconnect_attempt);
                reconnect_attempt = reconnect_attempt.saturating_add(1);
                info!(error = %error, ?delay, "WebSocket connection failed; retrying");
                sleep(delay).await;
                continue;
            }
        };

        let filter = build_filter(config.start_block)?;
        let sub = match provider.subscribe_logs(&filter).await {
            Ok(sub) => sub,
            Err(error) => {
                let delay = reconnect_delay(reconnect_attempt);
                reconnect_attempt = reconnect_attempt.saturating_add(1);
                info!(error = %error, ?delay, "WebSocket subscribe failed; retrying");
                sleep(delay).await;
                continue;
            }
        };

        reconnect_attempt = 0;
        let mut stream = sub.into_stream();

        if let Some(runtime) = metrics_runtime.as_ref() {
            runtime.metrics.ws_connected.set(1.0);
        }

        info!("Connected to Polygon WebSocket and listening for events");

        loop {
            let log = tokio::select! {
                maybe_log = stream.next() => {
                    match maybe_log {
                        Some(log) => log,
                        None => break,
                    }
                }
                result = tokio::signal::ctrl_c() => {
                    result?;
                    info!("Received shutdown signal, closing parquet writers");
                    let mut storage_guard = storage.lock().await;
                    storage_guard.close().await?;
                    info!("Shutdown complete");

                    if let Some(runtime) = metrics_runtime.take() {
                        runtime.metrics.shutdown_total.inc();
                        runtime.metrics.ws_connected.set(0.0);
                        runtime.shutdown().await;
                    }

                    return Ok(());
                }
            };

            let block_number = match log.block_number {
                Some(block_number) => block_number,
                None => {
                    let error = anyhow!("frontfill received log missing block_number");
                    if let Some(runtime) = metrics_runtime.as_ref() {
                        runtime.metrics.missing_metadata_total.inc();
                    }
                    if let Some(runtime) = metrics_runtime.take() {
                        runtime.metrics.ws_connected.set(0.0);
                        runtime.shutdown().await;
                    }
                    return Err(error);
                }
            };

            let tx_hash = match log.transaction_hash {
                Some(tx_hash) => tx_hash.to_string(),
                None => {
                    let error = anyhow!("frontfill received log missing transaction_hash");
                    if let Some(runtime) = metrics_runtime.as_ref() {
                        runtime.metrics.missing_metadata_total.inc();
                    }
                    if let Some(runtime) = metrics_runtime.take() {
                        runtime.metrics.ws_connected.set(0.0);
                        runtime.shutdown().await;
                    }
                    return Err(error);
                }
            };

            let log_index = match log.log_index {
                Some(log_index) => log_index,
                None => {
                    let error = anyhow!("frontfill received log missing log_index");
                    if let Some(runtime) = metrics_runtime.as_ref() {
                        runtime.metrics.missing_metadata_total.inc();
                    }
                    if let Some(runtime) = metrics_runtime.take() {
                        runtime.metrics.ws_connected.set(0.0);
                        runtime.shutdown().await;
                    }
                    return Err(error);
                }
            };

            if let Some(runtime) = metrics_runtime.as_ref() {
                runtime.metrics.observe_log(block_number).await;
            }

            let mut storage_guard = storage.lock().await;

            if let Some(target_block) = next_rotation_block {
                if block_number >= target_block {
                    storage_guard.rotate().await?;
                    if let Some(runtime) = metrics_runtime.as_ref() {
                        runtime.metrics.rotate_total.inc();
                    }
                    info!(block_number, "Rotated storage writers");
                    next_rotation_block = Some(block_number.saturating_add(config.flush_blocks));
                }
            } else {
                next_rotation_block = Some(block_number.saturating_add(config.flush_blocks));
            }

            let decoded = match decode_log(&log) {
                Ok(decoded) => decoded,
                Err(error) => {
                    if let Some(runtime) = metrics_runtime.as_ref() {
                        runtime.metrics.decode_errors_total.inc();
                    }

                    if let Some(runtime) = metrics_runtime.take() {
                        runtime.metrics.ws_connected.set(0.0);
                        runtime.shutdown().await;
                    }
                    return Err(error);
                }
            };

            if let Some(decoded) = decoded {
                if let Some(runtime) = metrics_runtime.as_ref() {
                    runtime
                        .metrics
                        .observe_decoded_type(decoded.event_type)
                        .await;
                }

                let write_timer = metrics_runtime
                    .as_ref()
                    .map(|runtime| runtime.metrics.sink_write_seconds.start_timer());

                if let Err(error) = storage_guard
                    .write_event(
                        decoded.event_type,
                        block_number,
                        &tx_hash,
                        log_index,
                        &decoded.columns,
                    )
                    .await
                {
                    if let Some(runtime) = metrics_runtime.as_ref() {
                        runtime.metrics.write_errors_total.inc();
                    }

                    if let Some(timer) = write_timer {
                        timer.observe_duration();
                    }

                    if let Some(runtime) = metrics_runtime.take() {
                        runtime.metrics.ws_connected.set(0.0);
                        runtime.shutdown().await;
                    }

                    return Err(error);
                }

                if let Some(timer) = write_timer {
                    timer.observe_duration();
                }
            }
        }

        if let Some(runtime) = metrics_runtime.as_ref() {
            runtime.metrics.ws_connected.set(0.0);
        }

        let delay = reconnect_delay(reconnect_attempt);
        reconnect_attempt = reconnect_attempt.saturating_add(1);
        info!(?delay, "WebSocket stream ended; reconnecting");
        sleep(delay).await;
    }
}

#[cfg(test)]
mod tests {
    use alloy::transports::Authorization;
    use tempfile::tempdir;

    use super::{FrontfillConfig, SinkConfig, build_storage, build_ws_auth, build_ws_connect, run};

    fn test_config(sink: SinkConfig) -> FrontfillConfig {
        FrontfillConfig {
            flush_blocks: 10,
            start_block: None,
            rpc_url: "wss://example.invalid".to_string(),
            rpc_auth_key: None,
            rpc_auth_header: "Authorization".to_string(),
            rpc_auth_scheme: "Bearer".to_string(),
            metrics_enabled: false,
            metrics_bind: "127.0.0.1".to_string(),
            metrics_port: 9090,
            sink,
        }
    }

    #[test]
    fn build_ws_auth_defaults_to_bearer_authorization() {
        let auth = build_ws_auth(Some("abc"), "Authorization", "Bearer").expect("auth");
        assert_eq!(auth, Some(Authorization::bearer("abc")));
    }

    #[test]
    fn build_ws_auth_supports_raw_scheme_when_empty() {
        let auth = build_ws_auth(Some("abc"), "Authorization", "").expect("auth");
        assert_eq!(auth, Some(Authorization::raw("abc")));
    }

    #[test]
    fn build_ws_auth_rejects_non_authorization_header() {
        let error = build_ws_auth(Some("abc"), "x-api-key", "Bearer").expect_err("error");
        assert_eq!(
            error.to_string(),
            "frontfill websocket auth supports only the Authorization header"
        );
    }

    #[test]
    fn build_ws_auth_supports_custom_scheme_value() {
        let auth = build_ws_auth(Some("abc"), "Authorization", "Token").expect("auth");
        assert_eq!(auth, Some(Authorization::raw("Token abc")));
    }

    #[test]
    fn build_ws_auth_returns_none_for_empty_key() {
        let auth = build_ws_auth(Some(""), "Authorization", "Bearer").expect("auth");
        assert_eq!(auth, None);
    }

    #[test]
    fn build_ws_auth_returns_none_when_key_is_missing() {
        let auth = build_ws_auth(None, "Authorization", "Bearer").expect("auth");
        assert_eq!(auth, None);
    }

    #[test]
    fn build_ws_connect_sets_bearer_auth_when_provided() {
        let ws = build_ws_connect(
            "wss://example.invalid",
            Some("abc"),
            "Authorization",
            "Bearer",
        )
        .expect("ws");

        assert_eq!(ws.url, "wss://example.invalid");
        assert_eq!(ws.auth, Some(Authorization::bearer("abc")));
    }

    #[test]
    fn build_ws_connect_keeps_auth_empty_without_key() {
        let ws =
            build_ws_connect("wss://example.invalid", None, "Authorization", "Bearer").expect("ws");
        assert_eq!(ws.auth, None);
    }

    #[tokio::test]
    async fn build_storage_initializes_parquet_sink() {
        let dir = tempdir().expect("tempdir");
        let sink = build_storage(SinkConfig::Parquet {
            output_dir: dir.path().to_string_lossy().to_string(),
        })
        .expect("sink");

        sink.lock().await.close().await.expect("close");
    }

    #[cfg(not(feature = "kafka"))]
    #[test]
    fn build_storage_kafka_errors_without_feature() {
        let result = build_storage(SinkConfig::Kafka {
            brokers: Some("localhost:9092".to_string()),
            topic_prefix: "polymarket".to_string(),
        });

        let error = match result {
            Ok(_) => panic!("expected error"),
            Err(error) => error,
        };

        assert_eq!(
            error.to_string(),
            "kafka sink requires the 'kafka' feature flag"
        );
    }

    #[cfg(feature = "kafka")]
    #[tokio::test]
    async fn build_storage_kafka_requires_brokers() {
        let result = build_storage(SinkConfig::Kafka {
            brokers: None,
            topic_prefix: "polymarket".to_string(),
        });

        let error = match result {
            Ok(_) => panic!("expected error"),
            Err(error) => error,
        };

        assert_eq!(error.to_string(), "kafka sink requires --kafka-brokers");
    }

    #[tokio::test]
    async fn run_returns_error_when_flush_blocks_is_zero() {
        let config = FrontfillConfig {
            flush_blocks: 0,
            ..test_config(SinkConfig::Parquet {
                output_dir: "output".to_string(),
            })
        };

        let error = run(config).await.expect_err("error");
        assert_eq!(error.to_string(), "flush-blocks must be greater than 0");
    }

    #[tokio::test]
    async fn run_returns_error_for_invalid_auth_header_when_key_is_set() {
        let dir = tempdir().expect("tempdir");
        let mut config = test_config(SinkConfig::Parquet {
            output_dir: dir.path().to_string_lossy().to_string(),
        });
        config.rpc_auth_key = Some("abc".to_string());
        config.rpc_auth_header = "x-api-key".to_string();

        let error = run(config).await.expect_err("error");
        assert_eq!(
            error.to_string(),
            "frontfill websocket auth supports only the Authorization header"
        );
    }

    #[cfg(not(feature = "kafka"))]
    #[test]
    fn run_returns_error_for_kafka_sink_without_feature() {
        let config = test_config(SinkConfig::Kafka {
            brokers: Some("localhost:9092".to_string()),
            topic_prefix: "polymarket".to_string(),
        });

        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let error = runtime.block_on(run(config)).expect_err("error");
        assert_eq!(
            error.to_string(),
            "kafka sink requires the 'kafka' feature flag"
        );
    }
}
