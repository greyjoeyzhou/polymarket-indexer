use polymarket_indexer::metrics::start_metrics_runtime;
use std::time::Duration;

fn ephemeral_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    listener.local_addr().expect("local addr").port()
}

#[tokio::test]
async fn metrics_runtime_exposes_prometheus_endpoint() {
    let port = ephemeral_port();
    let runtime = start_metrics_runtime("127.0.0.1", port)
        .await
        .expect("start runtime");

    runtime.metrics.observe_log(123).await;
    runtime.metrics.observe_decoded_type("OrderFilled").await;
    runtime.metrics.decode_errors_total.inc();

    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{port}/metrics");

    let mut body = None;
    for _ in 0..20 {
        match client.get(&url).send().await {
            Ok(response) if response.status().is_success() => {
                body = Some(response.text().await.expect("metrics body"));
                break;
            }
            _ => tokio::time::sleep(Duration::from_millis(50)).await,
        }
    }

    let body = body.expect("metrics endpoint should become available");
    assert!(body.contains("polymarket_frontfill_current_block_number"));
    assert!(body.contains("polymarket_frontfill_logs_total"));
    assert!(body.contains("polymarket_frontfill_logs_by_type_total"));
    assert!(body.contains("polymarket_frontfill_decode_errors_total"));

    runtime.shutdown().await;
}
