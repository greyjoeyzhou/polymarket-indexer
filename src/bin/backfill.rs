use anyhow::Result;
use clap::Parser;
use polymarket_indexer::backfill::{self, BackfillConfig};
use tracing_subscriber::EnvFilter;

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

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .init();

    let args = Args::parse();

    backfill::run(BackfillConfig {
        start_block: args.start_block,
        end_block: args.end_block,
        chunk_size_by_block_number: args.chunk_size_by_block_number,
        parallelism: args.parallelism,
        rate_limit_per_second: args.rate_limit_per_second,
        rpc_http_url: args.rpc_http_url,
        rpc_http_key: args.rpc_http_key,
        output_dir: args.output_dir,
    })
    .await
}
