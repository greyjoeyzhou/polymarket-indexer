use alloy::primitives::{Address, B256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::Filter;
use alloy::sol;
use alloy::sol_types::SolEvent;
use anyhow::{Result, anyhow};
use clap::Parser;
use futures::StreamExt;
use reqwest::Url;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

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

// Polygon RPC URL (HTTP)
const RPC_HTTP_URL: &str = "https://polygon-bor-rpc.publicnode.com";

#[derive(Debug, Parser)]
#[command(name = "polymarket-backfill")]
struct Args {
    #[arg(long, env = "START_BLOCK")]
    start_block: u64,

    #[arg(long, env = "END_BLOCK")]
    end_block: u64,

    #[arg(long, default_value_t = 1000, env = "CHUNK_SIZE_BY_BLOCK_NUMBER")]
    chunk_size_by_block_number: u64,

    #[arg(long, default_value_t = 2, env = "PARALLELISM")]
    parallelism: usize,

    #[arg(long, default_value_t = 15, env = "RATE_LIMIT_PER_SECOND")]
    rate_limit_per_second: usize,

    #[arg(long, default_value = RPC_HTTP_URL, env = "RPC_HTTP_URL")]
    rpc_http_url: String,

    #[arg(long, env = "RPC_HTTP_KEY")]
    rpc_http_key: Option<String>,

    #[arg(long, default_value = "output", env = "OUTPUT_DIR")]
    output_dir: String,
}

#[derive(Clone)]
struct RateLimiter {
    semaphore: Arc<Semaphore>,
    max_per_second: usize,
}

impl RateLimiter {
    fn new(max_per_second: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_per_second)),
            max_per_second,
        }
    }

    async fn acquire(&self) {
        let permit = self.semaphore.acquire().await.expect("rate limiter");
        permit.forget();
    }

    fn spawn_refill(self) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
            loop {
                interval.tick().await;
                let available = self.semaphore.available_permits();
                if available < self.max_per_second {
                    self.semaphore.add_permits(self.max_per_second - available);
                }
            }
        });
    }
}

#[derive(Debug, Clone)]
struct EventRow {
    block_number: u64,
    tx_hash: String,
    log_index: u64,
    values: Vec<String>,
}

struct EventSpec {
    event_type: &'static str,
    columns: &'static [&'static str],
}

fn value_to_string<T: serde::Serialize>(value: &T) -> Result<String> {
    let value = serde_json::to_value(value)?;
    Ok(match value {
        serde_json::Value::String(value) => value,
        other => other.to_string(),
    })
}

fn build_http_url(base: &str, key: Option<&str>) -> Result<Url> {
    if let Some(key) = key {
        if base.contains("{API_KEY}") {
            let url = base.replace("{API_KEY}", key);
            return Ok(Url::parse(&url)?);
        }

        let mut url = Url::parse(base)?;
        if !key.is_empty() {
            url.query_pairs_mut().append_pair("apiKey", key);
        }
        return Ok(url);
    }

    Ok(Url::parse(base)?)
}

fn topic0(log: &alloy::rpc::types::Log) -> Option<B256> {
    log.topics().first().copied()
}

fn decode_log(log: &alloy::rpc::types::Log) -> Result<Option<(EventSpec, Vec<String>)>> {
    let topic0 = if let Some(topic0) = topic0(log) {
        topic0
    } else {
        return Ok(None);
    };

    if topic0 == ConditionPreparation::SIGNATURE_HASH {
        let event = log.log_decode::<ConditionPreparation>()?;
        return Ok(Some((
            EventSpec {
                event_type: "ConditionPreparation",
                columns: &[
                    "condition_id",
                    "oracle",
                    "question_id",
                    "outcome_slot_count",
                ],
            },
            vec![
                value_to_string(&event.inner.conditionId)?,
                value_to_string(&event.inner.oracle)?,
                value_to_string(&event.inner.questionId)?,
                value_to_string(&event.inner.outcomeSlotCount)?,
            ],
        )));
    }

    if topic0 == PositionSplit::SIGNATURE_HASH {
        let event = log.log_decode::<PositionSplit>()?;
        return Ok(Some((
            EventSpec {
                event_type: "PositionSplit",
                columns: &[
                    "collateral_token",
                    "parent_collection_id",
                    "condition_id",
                    "partition",
                    "amounts",
                    "amount",
                ],
            },
            vec![
                value_to_string(&event.inner.collateralToken)?,
                value_to_string(&event.inner.parentCollectionId)?,
                value_to_string(&event.inner.conditionId)?,
                value_to_string(&event.inner.partition)?,
                value_to_string(&event.inner.amounts)?,
                value_to_string(&event.inner.amount)?,
            ],
        )));
    }

    if topic0 == PositionsMerge::SIGNATURE_HASH {
        let event = log.log_decode::<PositionsMerge>()?;
        return Ok(Some((
            EventSpec {
                event_type: "PositionsMerge",
                columns: &[
                    "collateral_token",
                    "parent_collection_id",
                    "condition_id",
                    "partition",
                    "amounts",
                    "amount",
                ],
            },
            vec![
                value_to_string(&event.inner.collateralToken)?,
                value_to_string(&event.inner.parentCollectionId)?,
                value_to_string(&event.inner.conditionId)?,
                value_to_string(&event.inner.partition)?,
                value_to_string(&event.inner.amounts)?,
                value_to_string(&event.inner.amount)?,
            ],
        )));
    }

    if topic0 == PayoutRedemption::SIGNATURE_HASH {
        let event = log.log_decode::<PayoutRedemption>()?;
        return Ok(Some((
            EventSpec {
                event_type: "PayoutRedemption",
                columns: &[
                    "redeemer",
                    "collateral_token",
                    "parent_collection_id",
                    "condition_id",
                    "index_sets",
                    "amount",
                ],
            },
            vec![
                value_to_string(&event.inner.redeemer)?,
                value_to_string(&event.inner.collateralToken)?,
                value_to_string(&event.inner.parentCollectionId)?,
                value_to_string(&event.inner.conditionId)?,
                value_to_string(&event.inner.indexSets)?,
                value_to_string(&event.inner.amount)?,
            ],
        )));
    }

    if topic0 == OrderFilled::SIGNATURE_HASH {
        let event = log.log_decode::<OrderFilled>()?;
        return Ok(Some((
            EventSpec {
                event_type: "OrderFilled",
                columns: &[
                    "order_hash",
                    "maker",
                    "taker",
                    "maker_fill_amount",
                    "taker_fill_amount",
                    "maker_fee",
                    "taker_fee",
                ],
            },
            vec![
                value_to_string(&event.inner.orderHash)?,
                value_to_string(&event.inner.maker)?,
                value_to_string(&event.inner.taker)?,
                value_to_string(&event.inner.makerFillAmount)?,
                value_to_string(&event.inner.takerFillAmount)?,
                value_to_string(&event.inner.makerFee)?,
                value_to_string(&event.inner.takerFee)?,
            ],
        )));
    }

    if topic0 == OrdersMatched::SIGNATURE_HASH {
        let event = log.log_decode::<OrdersMatched>()?;
        return Ok(Some((
            EventSpec {
                event_type: "OrdersMatched",
                columns: &["taker_order_hash", "maker_order_hash"],
            },
            vec![
                value_to_string(&event.inner.takerOrderHash)?,
                value_to_string(&event.inner.makerOrderHash)?,
            ],
        )));
    }

    if topic0 == OrderCancelled::SIGNATURE_HASH {
        let event = log.log_decode::<OrderCancelled>()?;
        return Ok(Some((
            EventSpec {
                event_type: "OrderCancelled",
                columns: &["order_hash"],
            },
            vec![value_to_string(&event.inner.orderHash)?],
        )));
    }

    if topic0 == MarketPrepared::SIGNATURE_HASH {
        let event = log.log_decode::<MarketPrepared>()?;
        return Ok(Some((
            EventSpec {
                event_type: "MarketPrepared",
                columns: &["condition_id", "creator", "market_id", "data"],
            },
            vec![
                value_to_string(&event.inner.conditionId)?,
                value_to_string(&event.inner.creator)?,
                value_to_string(&event.inner.marketId)?,
                value_to_string(&event.inner.data)?,
            ],
        )));
    }

    if topic0 == QuestionPrepared::SIGNATURE_HASH {
        let event = log.log_decode::<QuestionPrepared>()?;
        return Ok(Some((
            EventSpec {
                event_type: "QuestionPrepared",
                columns: &["question_id", "condition_id", "creator", "data"],
            },
            vec![
                value_to_string(&event.inner.questionId)?,
                value_to_string(&event.inner.conditionId)?,
                value_to_string(&event.inner.creator)?,
                value_to_string(&event.inner.data)?,
            ],
        )));
    }

    if topic0 == PositionsConverted::SIGNATURE_HASH {
        let event = log.log_decode::<PositionsConverted>()?;
        return Ok(Some((
            EventSpec {
                event_type: "PositionsConverted",
                columns: &["condition_id", "stakeholder", "amount", "fee"],
            },
            vec![
                value_to_string(&event.inner.conditionId)?,
                value_to_string(&event.inner.stakeholder)?,
                value_to_string(&event.inner.amount)?,
                value_to_string(&event.inner.fee)?,
            ],
        )));
    }

    Ok(None)
}

fn write_parquet_file(
    output_dir: &str,
    event_type: &str,
    columns: &[&str],
    rows: &[EventRow],
    chunk_start: u64,
    chunk_end: u64,
) -> Result<()> {
    use arrow::array::{ArrayRef, StringArray, UInt64Array};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use parquet::arrow::ArrowWriter;
    use parquet::file::properties::WriterProperties;

    let mut fields = vec![
        Field::new("event_type", DataType::Utf8, false),
        Field::new("block_number", DataType::UInt64, false),
        Field::new("transaction_hash", DataType::Utf8, false),
        Field::new("log_index", DataType::UInt64, false),
    ];
    for column in columns {
        fields.push(Field::new(*column, DataType::Utf8, false));
    }

    let schema = Arc::new(Schema::new(fields));

    let event_dir = std::path::Path::new(output_dir).join(event_type);
    std::fs::create_dir_all(&event_dir)?;
    let file_path = event_dir.join(format!("{}-{}.parquet", chunk_start, chunk_end));
    let file = std::fs::File::create(file_path)?;
    let props = WriterProperties::builder().build();
    let mut writer = ArrowWriter::try_new(file, schema.clone(), Some(props))?;

    let event_type_array = StringArray::from(vec![event_type; rows.len()]);
    let block_number_array =
        UInt64Array::from(rows.iter().map(|row| row.block_number).collect::<Vec<_>>());
    let tx_hash_array = StringArray::from(
        rows.iter()
            .map(|row| row.tx_hash.as_str())
            .collect::<Vec<_>>(),
    );
    let log_index_array =
        UInt64Array::from(rows.iter().map(|row| row.log_index).collect::<Vec<_>>());

    let mut arrays: Vec<ArrayRef> = vec![
        Arc::new(event_type_array),
        Arc::new(block_number_array),
        Arc::new(tx_hash_array),
        Arc::new(log_index_array),
    ];

    for col_index in 0..columns.len() {
        let values = rows
            .iter()
            .map(|row| row.values[col_index].as_str())
            .collect::<Vec<_>>();
        arrays.push(Arc::new(StringArray::from(values)));
    }

    let batch = RecordBatch::try_new(schema, arrays)?;
    writer.write(&batch)?;
    writer.close()?;
    Ok(())
}

fn chunk_ranges(start: u64, end: u64, size: u64) -> Vec<(u64, u64)> {
    let mut ranges = Vec::new();
    let mut current = start;
    while current <= end {
        let chunk_end = (current + size - 1).min(end);
        ranges.push((current, chunk_end));
        if chunk_end == end {
            break;
        }
        current = chunk_end + 1;
    }
    ranges
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
    if args.start_block > args.end_block {
        return Err(anyhow!("start-block must be <= end-block"));
    }
    if args.chunk_size_by_block_number == 0 {
        return Err(anyhow!("chunk-size-by-block-number must be > 0"));
    }
    if args.parallelism == 0 {
        return Err(anyhow!("parallelism must be > 0"));
    }
    if args.rate_limit_per_second == 0 {
        return Err(anyhow!("rate-limit-per-second must be > 0"));
    }

    info!(
        start_block = args.start_block,
        end_block = args.end_block,
        chunk_size = args.chunk_size_by_block_number,
        parallelism = args.parallelism,
        rate_limit = args.rate_limit_per_second,
        output_dir = %args.output_dir,
        rpc_http_url = %args.rpc_http_url,
        "Starting backfill"
    );

    let rate_limiter = RateLimiter::new(args.rate_limit_per_second);
    rate_limiter.clone().spawn_refill();

    let http_url = build_http_url(&args.rpc_http_url, args.rpc_http_key.as_deref())?;
    let provider = ProviderBuilder::new().on_http(http_url);

    let ctf_address = Address::from_str(CTF_ADDRESS)?;
    let exchange_address = Address::from_str(CTF_EXCHANGE_ADDRESS)?;
    let neg_risk_address = Address::from_str(NEG_RISK_ADAPTER_ADDRESS)?;
    let addresses = vec![ctf_address, exchange_address, neg_risk_address];

    let ranges = chunk_ranges(
        args.start_block,
        args.end_block,
        args.chunk_size_by_block_number,
    );

    let provider = Arc::new(provider);
    let output_dir = Arc::new(args.output_dir);

    futures::stream::iter(ranges)
        .for_each_concurrent(args.parallelism, |(chunk_start, chunk_end)| {
            let provider = Arc::clone(&provider);
            let output_dir = Arc::clone(&output_dir);
            let rate_limiter = rate_limiter.clone();
            let addresses = addresses.clone();
            async move {
                rate_limiter.acquire().await;
                let filter = Filter::new()
                    .address(addresses)
                    .from_block(alloy::rpc::types::BlockNumberOrTag::Number(chunk_start))
                    .to_block(alloy::rpc::types::BlockNumberOrTag::Number(chunk_end));

                info!(chunk_start, chunk_end, "Fetching logs");
                let logs = match provider.get_logs(&filter).await {
                    Ok(logs) => logs,
                    Err(error) => {
                        warn!(chunk_start, chunk_end, error = %error, "Fetch failed");
                        return;
                    }
                };

                let mut by_event: HashMap<String, (Vec<&'static str>, Vec<EventRow>)> =
                    HashMap::new();

                for log in logs {
                    let decoded = match decode_log(&log) {
                        Ok(decoded) => decoded,
                        Err(error) => {
                            warn!(error = %error, "Decode failed");
                            continue;
                        }
                    };

                    let (spec, values) = match decoded {
                        Some(decoded) => decoded,
                        None => continue,
                    };

                    let block_number = log.block_number.unwrap_or(chunk_start);
                    let tx_hash = log.transaction_hash.unwrap_or_default().to_string();
                    let log_index = log.log_index.unwrap_or(0);
                    let entry = by_event
                        .entry(spec.event_type.to_string())
                        .or_insert_with(|| (spec.columns.to_vec(), Vec::new()));

                    entry.1.push(EventRow {
                        block_number,
                        tx_hash,
                        log_index,
                        values,
                    });
                }

                for (event_type, (columns, mut rows)) in by_event {
                    rows.sort_by(|a, b| {
                        a.block_number
                            .cmp(&b.block_number)
                            .then(a.log_index.cmp(&b.log_index))
                    });

                    if let Err(error) = write_parquet_file(
                        &output_dir,
                        &event_type,
                        &columns,
                        &rows,
                        chunk_start,
                        chunk_end,
                    ) {
                        warn!(
                            chunk_start,
                            chunk_end,
                            event_type = %event_type,
                            error = %error,
                            "Failed to write parquet"
                        );
                    }
                }

                info!(chunk_start, chunk_end, "Chunk complete");
            }
        })
        .await;

    info!("Backfill complete");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::build_http_url;

    #[test]
    fn build_http_url_replaces_placeholder() {
        let url = build_http_url("https://example.com/{API_KEY}", Some("abc")).expect("url");
        assert_eq!(url.as_str(), "https://example.com/abc");
    }

    #[test]
    fn build_http_url_appends_query() {
        let url = build_http_url("https://example.com/rpc", Some("abc")).expect("url");
        assert_eq!(url.as_str(), "https://example.com/rpc?apiKey=abc");
    }

    #[test]
    fn build_http_url_appends_query_when_existing() {
        let url = build_http_url("https://example.com/rpc?foo=bar", Some("abc")).expect("url");
        assert_eq!(url.as_str(), "https://example.com/rpc?foo=bar&apiKey=abc");
    }
}
