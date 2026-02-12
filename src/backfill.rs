use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::{BlockNumberOrTag, Filter};
use anyhow::{Result, anyhow};
use futures::StreamExt;
use reqwest::Url;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{info, warn};

use crate::contracts::contract_addresses;
use crate::decode::decode_log;

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

pub struct BackfillConfig {
    pub start_block: u64,
    pub end_block: u64,
    pub chunk_size_by_block_number: u64,
    pub parallelism: usize,
    pub rate_limit_per_second: usize,
    pub rpc_http_url: String,
    pub rpc_http_key: Option<String>,
    pub output_dir: String,
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

fn write_parquet_file(
    output_dir: &str,
    event_type: &str,
    columns: &[String],
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
        fields.push(Field::new(column, DataType::Utf8, false));
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

pub async fn run(config: BackfillConfig) -> Result<()> {
    if config.start_block > config.end_block {
        return Err(anyhow!("start-block must be <= end-block"));
    }
    if config.chunk_size_by_block_number == 0 {
        return Err(anyhow!("chunk-size-by-block-number must be > 0"));
    }
    if config.parallelism == 0 {
        return Err(anyhow!("parallelism must be > 0"));
    }
    if config.rate_limit_per_second == 0 {
        return Err(anyhow!("rate-limit-per-second must be > 0"));
    }

    info!(
        start_block = config.start_block,
        end_block = config.end_block,
        chunk_size = config.chunk_size_by_block_number,
        parallelism = config.parallelism,
        rate_limit = config.rate_limit_per_second,
        output_dir = %config.output_dir,
        rpc_http_url = %config.rpc_http_url,
        "Starting backfill"
    );

    let rate_limiter = RateLimiter::new(config.rate_limit_per_second);
    rate_limiter.clone().spawn_refill();

    let http_url = build_http_url(&config.rpc_http_url, config.rpc_http_key.as_deref())?;
    let provider = Arc::new(ProviderBuilder::new().on_http(http_url));

    let addresses = contract_addresses()?;
    let ranges = chunk_ranges(
        config.start_block,
        config.end_block,
        config.chunk_size_by_block_number,
    );
    let output_dir = Arc::new(config.output_dir);

    futures::stream::iter(ranges)
        .for_each_concurrent(config.parallelism, |(chunk_start, chunk_end)| {
            let provider = Arc::clone(&provider);
            let output_dir = Arc::clone(&output_dir);
            let rate_limiter = rate_limiter.clone();
            let addresses = addresses.clone();

            async move {
                rate_limiter.acquire().await;

                let filter = Filter::new()
                    .address(addresses)
                    .from_block(BlockNumberOrTag::Number(chunk_start))
                    .to_block(BlockNumberOrTag::Number(chunk_end));

                info!(chunk_start, chunk_end, "Fetching logs");
                let logs = match provider.get_logs(&filter).await {
                    Ok(logs) => logs,
                    Err(error) => {
                        warn!(chunk_start, chunk_end, error = %error, "Fetch failed");
                        return;
                    }
                };

                let mut by_event: HashMap<String, (Vec<String>, Vec<EventRow>)> = HashMap::new();

                for log in logs {
                    let decoded = match decode_log(&log) {
                        Ok(Some(decoded)) => decoded,
                        Ok(None) => continue,
                        Err(error) => {
                            warn!(error = %error, "Decode failed");
                            continue;
                        }
                    };

                    let block_number = log.block_number.unwrap_or(chunk_start);
                    let tx_hash = log.transaction_hash.unwrap_or_default().to_string();
                    let log_index = log.log_index.unwrap_or(0);

                    let entry = by_event
                        .entry(decoded.event_type.to_string())
                        .or_insert_with(|| {
                            (
                                decoded
                                    .columns
                                    .iter()
                                    .map(|(name, _)| (*name).to_string())
                                    .collect(),
                                Vec::new(),
                            )
                        });

                    entry.1.push(EventRow {
                        block_number,
                        tx_hash,
                        log_index,
                        values: decoded
                            .columns
                            .into_iter()
                            .map(|(_, value)| value)
                            .collect(),
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
