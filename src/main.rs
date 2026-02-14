use anyhow::{Result, anyhow};
use clap::{Parser, ValueEnum};
use polymarket_indexer::frontfill::{self, FrontfillConfig, SinkConfig};
use tracing::info;
use tracing_subscriber::EnvFilter;

const RPC_URL: &str = "wss://polygon-bor-rpc.publicnode.com";

#[derive(Debug, Parser)]
#[command(name = "polymarket-indexer")]
struct Args {
    #[arg(short = 'f', long, default_value_t = 1000, env = "FLUSH_BLOCKS")]
    flush_blocks: u64,

    #[arg(long, default_value = RPC_URL, env = "RPC_URL")]
    rpc_url: String,

    #[arg(long, env = "RPC_AUTH_KEY")]
    rpc_auth_key: Option<String>,

    #[arg(long, default_value = "Authorization", env = "RPC_AUTH_HEADER")]
    rpc_auth_header: String,

    #[arg(long, default_value = "Bearer", env = "RPC_AUTH_SCHEME")]
    rpc_auth_scheme: String,

    #[arg(long, default_value = "output", env = "OUTPUT_DIR")]
    output_dir: String,

    #[arg(long, default_value = "parquet", env = "SINK")]
    sink: SinkType,

    #[arg(long, env = "KAFKA_BROKERS")]
    kafka_brokers: Option<String>,

    #[arg(long, default_value = "polymarket", env = "KAFKA_TOPIC_PREFIX")]
    kafka_topic_prefix: String,

    #[arg(long, default_value_t = false, env = "METRICS_ENABLED")]
    metrics_enabled: bool,

    #[arg(long, default_value = "127.0.0.1", env = "METRICS_BIND")]
    metrics_bind: String,

    #[arg(long, default_value_t = 9090, env = "METRICS_PORT")]
    metrics_port: u16,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum SinkType {
    Parquet,
    Kafka,
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
    info!("Starting Polymarket Indexer");

    let sink = match args.sink {
        SinkType::Parquet => SinkConfig::Parquet {
            output_dir: args.output_dir,
        },
        SinkType::Kafka => {
            if args.kafka_brokers.is_none() {
                return Err(anyhow!("kafka sink requires --kafka-brokers"));
            }
            SinkConfig::Kafka {
                brokers: args.kafka_brokers,
                topic_prefix: args.kafka_topic_prefix,
            }
        }
    };

    frontfill::run(FrontfillConfig {
        flush_blocks: args.flush_blocks,
        rpc_url: args.rpc_url,
        rpc_auth_key: args.rpc_auth_key,
        rpc_auth_header: args.rpc_auth_header,
        rpc_auth_scheme: args.rpc_auth_scheme,
        metrics_enabled: args.metrics_enabled,
        metrics_bind: args.metrics_bind,
        metrics_port: args.metrics_port,
        sink,
    })
    .await
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

    #[test]
    fn args_use_default_auth_header_settings() {
        let args = Args::try_parse_from(["polymarket-indexer"]).expect("parse args");
        assert_eq!(args.rpc_auth_header, "Authorization");
        assert_eq!(args.rpc_auth_scheme, "Bearer");
    }

    #[test]
    fn args_use_default_metrics_settings() {
        let args = Args::try_parse_from(["polymarket-indexer"]).expect("parse args");
        assert!(!args.metrics_enabled);
        assert_eq!(args.metrics_bind, "127.0.0.1");
        assert_eq!(args.metrics_port, 9090);
    }
}
