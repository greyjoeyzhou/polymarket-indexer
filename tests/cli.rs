use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn help_flag_exits_successfully() {
    let mut command = Command::new(env!("CARGO_BIN_EXE_polymarket-indexer"));

    command
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("--flush-blocks"));
}

#[test]
fn zero_flush_blocks_returns_error() {
    let mut command = Command::new(env!("CARGO_BIN_EXE_polymarket-indexer"));

    command
        .args(["--flush-blocks", "0"])
        .assert()
        .failure()
        .stderr(contains("flush-blocks must be greater than 0"));
}
