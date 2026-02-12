#[cfg(feature = "kafka")]
mod kafka_tests {
    use std::path::Path;
    use std::time::Duration;

    use futures::StreamExt;
    use rdkafka::config::ClientConfig;
    use rdkafka::consumer::{Consumer, StreamConsumer};
    use rdkafka::message::Message;
    use testcontainers::clients::Cli;
    use testcontainers_modules::kafka::{KAFKA_PORT, Kafka};

    use polymarket_indexer::storage::{EventSink, KafkaStorage};

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn kafka_sink_produces_message() {
        if !Path::new("/var/run/docker.sock").exists() {
            eprintln!("Skipping kafka integration test: docker not available");
            return;
        }

        let docker = Cli::default();
        let kafka = docker.run(Kafka::default());
        let brokers = format!("127.0.0.1:{}", kafka.get_host_port_ipv4(KAFKA_PORT));

        let topic_prefix = format!("polymarket_test_{}", uuid::Uuid::new_v4());
        let topic = format!("{}.OrderCancelled", topic_prefix);

        let mut sink = KafkaStorage::new(&brokers, &topic_prefix).expect("create kafka sink");

        let columns = vec![("order_hash", "0xabc".to_string())];
        sink.write_event("OrderCancelled", 1, "0x1", 1, &columns)
            .await
            .expect("write event");

        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", &brokers)
            .set("group.id", "polymarket-test")
            .set("auto.offset.reset", "earliest")
            .create()
            .expect("create consumer");

        consumer.subscribe(&[&topic]).expect("subscribe to topic");

        let mut stream = consumer.stream();
        let message = tokio::time::timeout(Duration::from_secs(10), stream.next())
            .await
            .expect("receive message")
            .expect("message value")
            .expect("kafka message");

        let payload = message.payload().expect("payload exists");
        assert!(!payload.is_empty());
    }
}
