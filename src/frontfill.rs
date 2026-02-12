use alloy::providers::{Provider, ProviderBuilder, WsConnect};
use alloy::rpc::types::{BlockNumberOrTag, Filter};
use anyhow::{Result, anyhow};
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

use crate::contracts::contract_addresses;
use crate::decode::decode_log;
#[cfg(feature = "kafka")]
use crate::storage::KafkaStorage;
use crate::storage::{EventSink, ParquetStorage};

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
    pub rpc_url: String,
    pub sink: SinkConfig,
}

pub async fn run(config: FrontfillConfig) -> Result<()> {
    if config.flush_blocks == 0 {
        return Err(anyhow!("flush-blocks must be greater than 0"));
    }

    info!(
        flush_blocks = config.flush_blocks,
        rpc_url = %config.rpc_url,
        "Runtime configuration loaded"
    );

    let storage: Arc<Mutex<Box<dyn EventSink>>> = match config.sink {
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
    };

    let ws = WsConnect::new(&config.rpc_url);
    let provider = ProviderBuilder::new().on_ws(ws).await?;
    info!("Connected to Polygon WebSocket");

    let filter = Filter::new()
        .address(contract_addresses()?)
        .from_block(BlockNumberOrTag::Latest);

    let sub = provider.subscribe_logs(&filter).await?;
    let mut stream = sub.into_stream();

    info!("Listening for events");
    let mut next_rotation_block: Option<u64> = None;

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
                break;
            }
        };

        let block_number = log.block_number.unwrap_or(0);
        let tx_hash = log.transaction_hash.unwrap_or_default().to_string();
        let log_index = log.log_index.unwrap_or(0);
        let mut storage_guard = storage.lock().await;

        if let Some(target_block) = next_rotation_block {
            if block_number >= target_block {
                storage_guard.rotate().await?;
                info!(block_number, "Rotated storage writers");
                next_rotation_block = Some(block_number.saturating_add(config.flush_blocks));
            }
        } else {
            next_rotation_block = Some(block_number.saturating_add(config.flush_blocks));
        }

        if let Some(decoded) = decode_log(&log)? {
            storage_guard
                .write_event(
                    decoded.event_type,
                    block_number,
                    &tx_hash,
                    log_index,
                    &decoded.columns,
                )
                .await?;
        }
    }

    Ok(())
}
