use alloy::primitives::{Address, B256};
use alloy::providers::{Provider, ProviderBuilder, WsConnect};
use alloy::rpc::types::Filter;
use alloy::sol;
use alloy::sol_types::SolEvent;
use anyhow::{Result, anyhow};
use clap::{Parser, ValueEnum};
use futures::StreamExt;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

mod storage;
#[cfg(feature = "kafka")]
use storage::KafkaStorage;
use storage::{EventSink, ParquetStorage};

// Define the events using alloy's sol! macro
sol! {
    #[derive(Debug, serde::Serialize)]
    event ConditionPreparation(bytes32 indexed conditionId, address indexed oracle, bytes32 indexed questionId, uint256 outcomeSlotCount);

    #[derive(Debug, serde::Serialize)]
    event PositionSplit(address indexed collateralToken, address indexed parentCollectionId, bytes32 indexed conditionId, bytes32 partition, uint256[] amounts, uint256 amount);

    #[derive(Debug, serde::Serialize)]
    event PositionsMerge(address indexed collateralToken, address indexed parentCollectionId, bytes32 indexed conditionId, bytes32 partition, uint256[] amounts, uint256 amount);

    #[derive(Debug, serde::Serialize)]
    event PayoutRedemption(address indexed redeemer, address indexed collateralToken, bytes32 indexed parentCollectionId, bytes32 conditionId, uint256[] indexSets, uint256 amount);

    #[derive(Debug, serde::Serialize)]
    event OrderFilled(bytes32 indexed orderHash, address indexed maker, address indexed taker, uint256 makerFillAmount, uint256 takerFillAmount, uint256 makerFee, uint256 takerFee);

    #[derive(Debug, serde::Serialize)]
    event OrdersMatched(bytes32 indexed takerOrderHash, bytes32 indexed makerOrderHash);

    #[derive(Debug, serde::Serialize)]
    event OrderCancelled(bytes32 indexed orderHash);

    #[derive(Debug, serde::Serialize)]
    event MarketPrepared(bytes32 indexed conditionId, address indexed creator, uint256 indexed marketId, bytes data);

    #[derive(Debug, serde::Serialize)]
    event QuestionPrepared(bytes32 indexed questionId, bytes32 indexed conditionId, address indexed creator, bytes data);

    #[derive(Debug, serde::Serialize)]
    event PositionsConverted(bytes32 indexed conditionId, address indexed stakeholder, uint256 amount, uint256 fee);
}

// Contract Addresses
pub const CTF_ADDRESS: &str = "0x4D97DCd97eC945f40cF65F87097ACe5EA0476045";
pub const CTF_EXCHANGE_ADDRESS: &str = "0x4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E";
pub const NEG_RISK_ADAPTER_ADDRESS: &str = "0xd91E80cF2E7be2e162c6513ceD06f1dD0dA35296";

// Polygon RPC URL (Public WebSocket)
const RPC_URL: &str = "wss://polygon-bor-rpc.publicnode.com";

#[derive(Debug, Parser)]
#[command(name = "polymarket-indexer")]
struct Args {
    #[arg(short = 'f', long, default_value_t = 1000, env = "FLUSH_BLOCKS")]
    flush_blocks: u64,

    #[arg(long, default_value = RPC_URL, env = "RPC_URL")]
    rpc_url: String,

    #[arg(long, default_value = "output", env = "OUTPUT_DIR")]
    output_dir: String,

    #[arg(long, default_value = "parquet", env = "SINK")]
    sink: SinkType,

    #[arg(long, env = "KAFKA_BROKERS")]
    kafka_brokers: Option<String>,

    #[arg(long, default_value = "polymarket", env = "KAFKA_TOPIC_PREFIX")]
    kafka_topic_prefix: String,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum SinkType {
    Parquet,
    Kafka,
}

fn value_to_string<T: serde::Serialize>(value: &T) -> Result<String> {
    let value = serde_json::to_value(value)?;
    Ok(match value {
        serde_json::Value::String(value) => value,
        other => other.to_string(),
    })
}

fn topic0(log: &alloy::rpc::types::Log) -> Option<B256> {
    log.topics().first().copied()
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .init();

    let args = Args::parse();
    if args.flush_blocks == 0 {
        return Err(anyhow!("flush-blocks must be greater than 0"));
    }

    info!("Starting Polymarket Indexer");
    info!(
        flush_blocks = args.flush_blocks,
        rpc_url = %args.rpc_url,
        output_dir = %args.output_dir,
        "Runtime configuration loaded"
    );

    // Initialize storage
    let storage: Arc<Mutex<Box<dyn EventSink>>> = match args.sink {
        SinkType::Parquet => Arc::new(Mutex::new(Box::new(ParquetStorage::new(&args.output_dir)))),
        SinkType::Kafka => {
            #[cfg(feature = "kafka")]
            {
                let brokers = args
                    .kafka_brokers
                    .as_ref()
                    .ok_or_else(|| anyhow!("kafka sink requires --kafka-brokers"))?;
                Arc::new(Mutex::new(Box::new(KafkaStorage::new(
                    brokers,
                    &args.kafka_topic_prefix,
                )?)))
            }

            #[cfg(not(feature = "kafka"))]
            {
                return Err(anyhow!("kafka sink requires the 'kafka' feature flag"));
            }
        }
    };

    // Connect to Polygon WebSocket
    let ws = WsConnect::new(&args.rpc_url);
    let provider = ProviderBuilder::new().on_ws(ws).await?;
    info!("Connected to Polygon WebSocket");

    // Create filters for each contract
    let ctf_address = Address::from_str(CTF_ADDRESS)?;
    let exchange_address = Address::from_str(CTF_EXCHANGE_ADDRESS)?;
    let neg_risk_address = Address::from_str(NEG_RISK_ADAPTER_ADDRESS)?;

    let filter = Filter::new()
        .address(vec![ctf_address, exchange_address, neg_risk_address])
        .from_block(alloy::rpc::types::BlockNumberOrTag::Latest);

    // Subscribe to logs
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
                next_rotation_block = Some(block_number.saturating_add(args.flush_blocks));
            }
        } else {
            next_rotation_block = Some(block_number.saturating_add(args.flush_blocks));
        }

        // Try to decode log as one of the known events
        // Note: In a full implementation, we would check topics more efficiently

        let Some(topic0) = topic0(&log) else {
            continue;
        };

        match topic0 {
            ConditionPreparation::SIGNATURE_HASH => {
                if let Ok(event) = log.log_decode::<ConditionPreparation>() {
                    info!(block_number, "Detected ConditionPreparation");
                    let columns = vec![
                        ("condition_id", value_to_string(&event.inner.conditionId)?),
                        ("oracle", value_to_string(&event.inner.oracle)?),
                        ("question_id", value_to_string(&event.inner.questionId)?),
                        (
                            "outcome_slot_count",
                            value_to_string(&event.inner.outcomeSlotCount)?,
                        ),
                    ];
                    storage_guard
                        .write_event(
                            "ConditionPreparation",
                            block_number,
                            &tx_hash,
                            log_index,
                            &columns,
                        )
                        .await?;
                } else {
                    warn!(block_number, "Failed to decode ConditionPreparation");
                }
            }
            PositionSplit::SIGNATURE_HASH => {
                if let Ok(event) = log.log_decode::<PositionSplit>() {
                    info!(block_number, "Detected PositionSplit");
                    let columns = vec![
                        (
                            "collateral_token",
                            value_to_string(&event.inner.collateralToken)?,
                        ),
                        (
                            "parent_collection_id",
                            value_to_string(&event.inner.parentCollectionId)?,
                        ),
                        ("condition_id", value_to_string(&event.inner.conditionId)?),
                        ("partition", value_to_string(&event.inner.partition)?),
                        ("amounts", value_to_string(&event.inner.amounts)?),
                        ("amount", value_to_string(&event.inner.amount)?),
                    ];
                    storage_guard
                        .write_event("PositionSplit", block_number, &tx_hash, log_index, &columns)
                        .await?;
                } else {
                    warn!(block_number, "Failed to decode PositionSplit");
                }
            }
            PositionsMerge::SIGNATURE_HASH => {
                if let Ok(event) = log.log_decode::<PositionsMerge>() {
                    info!(block_number, "Detected PositionsMerge");
                    let columns = vec![
                        (
                            "collateral_token",
                            value_to_string(&event.inner.collateralToken)?,
                        ),
                        (
                            "parent_collection_id",
                            value_to_string(&event.inner.parentCollectionId)?,
                        ),
                        ("condition_id", value_to_string(&event.inner.conditionId)?),
                        ("partition", value_to_string(&event.inner.partition)?),
                        ("amounts", value_to_string(&event.inner.amounts)?),
                        ("amount", value_to_string(&event.inner.amount)?),
                    ];
                    storage_guard
                        .write_event(
                            "PositionsMerge",
                            block_number,
                            &tx_hash,
                            log_index,
                            &columns,
                        )
                        .await?;
                } else {
                    warn!(block_number, "Failed to decode PositionsMerge");
                }
            }
            PayoutRedemption::SIGNATURE_HASH => {
                if let Ok(event) = log.log_decode::<PayoutRedemption>() {
                    info!(block_number, "Detected PayoutRedemption");
                    let columns = vec![
                        ("redeemer", value_to_string(&event.inner.redeemer)?),
                        (
                            "collateral_token",
                            value_to_string(&event.inner.collateralToken)?,
                        ),
                        (
                            "parent_collection_id",
                            value_to_string(&event.inner.parentCollectionId)?,
                        ),
                        ("condition_id", value_to_string(&event.inner.conditionId)?),
                        ("index_sets", value_to_string(&event.inner.indexSets)?),
                        ("amount", value_to_string(&event.inner.amount)?),
                    ];
                    storage_guard
                        .write_event(
                            "PayoutRedemption",
                            block_number,
                            &tx_hash,
                            log_index,
                            &columns,
                        )
                        .await?;
                } else {
                    warn!(block_number, "Failed to decode PayoutRedemption");
                }
            }
            OrderFilled::SIGNATURE_HASH => {
                if let Ok(event) = log.log_decode::<OrderFilled>() {
                    info!(block_number, "Detected OrderFilled");
                    let columns = vec![
                        ("order_hash", value_to_string(&event.inner.orderHash)?),
                        ("maker", value_to_string(&event.inner.maker)?),
                        ("taker", value_to_string(&event.inner.taker)?),
                        (
                            "maker_fill_amount",
                            value_to_string(&event.inner.makerFillAmount)?,
                        ),
                        (
                            "taker_fill_amount",
                            value_to_string(&event.inner.takerFillAmount)?,
                        ),
                        ("maker_fee", value_to_string(&event.inner.makerFee)?),
                        ("taker_fee", value_to_string(&event.inner.takerFee)?),
                    ];
                    storage_guard
                        .write_event("OrderFilled", block_number, &tx_hash, log_index, &columns)
                        .await?;
                } else {
                    warn!(block_number, "Failed to decode OrderFilled");
                }
            }
            OrdersMatched::SIGNATURE_HASH => {
                if let Ok(event) = log.log_decode::<OrdersMatched>() {
                    info!(block_number, "Detected OrdersMatched");
                    let columns = vec![
                        (
                            "taker_order_hash",
                            value_to_string(&event.inner.takerOrderHash)?,
                        ),
                        (
                            "maker_order_hash",
                            value_to_string(&event.inner.makerOrderHash)?,
                        ),
                    ];
                    storage_guard
                        .write_event("OrdersMatched", block_number, &tx_hash, log_index, &columns)
                        .await?;
                } else {
                    warn!(block_number, "Failed to decode OrdersMatched");
                }
            }
            OrderCancelled::SIGNATURE_HASH => {
                if let Ok(event) = log.log_decode::<OrderCancelled>() {
                    info!(block_number, "Detected OrderCancelled");
                    let columns = vec![("order_hash", value_to_string(&event.inner.orderHash)?)];
                    storage_guard
                        .write_event(
                            "OrderCancelled",
                            block_number,
                            &tx_hash,
                            log_index,
                            &columns,
                        )
                        .await?;
                } else {
                    warn!(block_number, "Failed to decode OrderCancelled");
                }
            }
            MarketPrepared::SIGNATURE_HASH => {
                if let Ok(event) = log.log_decode::<MarketPrepared>() {
                    info!(block_number, "Detected MarketPrepared");
                    let columns = vec![
                        ("condition_id", value_to_string(&event.inner.conditionId)?),
                        ("creator", value_to_string(&event.inner.creator)?),
                        ("market_id", value_to_string(&event.inner.marketId)?),
                        ("data", value_to_string(&event.inner.data)?),
                    ];
                    storage_guard
                        .write_event(
                            "MarketPrepared",
                            block_number,
                            &tx_hash,
                            log_index,
                            &columns,
                        )
                        .await?;
                } else {
                    warn!(block_number, "Failed to decode MarketPrepared");
                }
            }
            QuestionPrepared::SIGNATURE_HASH => {
                if let Ok(event) = log.log_decode::<QuestionPrepared>() {
                    info!(block_number, "Detected QuestionPrepared");
                    let columns = vec![
                        ("question_id", value_to_string(&event.inner.questionId)?),
                        ("condition_id", value_to_string(&event.inner.conditionId)?),
                        ("creator", value_to_string(&event.inner.creator)?),
                        ("data", value_to_string(&event.inner.data)?),
                    ];
                    storage_guard
                        .write_event(
                            "QuestionPrepared",
                            block_number,
                            &tx_hash,
                            log_index,
                            &columns,
                        )
                        .await?;
                } else {
                    warn!(block_number, "Failed to decode QuestionPrepared");
                }
            }
            PositionsConverted::SIGNATURE_HASH => {
                if let Ok(event) = log.log_decode::<PositionsConverted>() {
                    info!(block_number, "Detected PositionsConverted");
                    let columns = vec![
                        ("condition_id", value_to_string(&event.inner.conditionId)?),
                        ("stakeholder", value_to_string(&event.inner.stakeholder)?),
                        ("amount", value_to_string(&event.inner.amount)?),
                        ("fee", value_to_string(&event.inner.fee)?),
                    ];
                    storage_guard
                        .write_event(
                            "PositionsConverted",
                            block_number,
                            &tx_hash,
                            log_index,
                            &columns,
                        )
                        .await?;
                } else {
                    warn!(block_number, "Failed to decode PositionsConverted");
                }
            }
            _ => {}
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Args;
    use clap::Parser;

    #[test]
    fn args_use_default_flush_blocks() {
        let args = Args::try_parse_from(["polymarket-indexer"]).expect("parse args");
        assert_eq!(args.flush_blocks, 1000);
    }

    #[test]
    fn args_parse_custom_flush_blocks() {
        let args = Args::try_parse_from(["polymarket-indexer", "--flush-blocks", "250"])
            .expect("parse args");
        assert_eq!(args.flush_blocks, 250);
    }
}
