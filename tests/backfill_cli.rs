use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn backfill_requires_valid_block_range() {
    let mut command = Command::new(env!("CARGO_BIN_EXE_backfill"));

    command
        .args(["--start-block", "10", "--end-block", "9"])
        .env("RPC_HTTP_URL", "https://example.com/rpc")
        .assert()
        .failure()
        .stderr(contains("start-block must be <= end-block"));
}
