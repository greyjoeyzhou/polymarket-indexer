# Spec: Kafka integration test with testcontainers

## Goal

Provide an integration test that exercises the Kafka sink end-to-end using a local Kafka broker started via testcontainers.

## Constraints

- Test must be gated behind the `kafka` feature.
- Test should be skipped if Docker is unavailable (surface a clear error).
- Use a lightweight Kafka container (testcontainers module).

## Approach

- Add dev-dependencies: `testcontainers` and `testcontainers-modules`.
- Start Kafka container in test, get bootstrap servers.
- Create `KafkaStorage` with a random topic prefix.
- Write a sample event and verify the message is produced by consuming from the topic.

## Test Steps

1) Start container and obtain broker address.
2) Initialize `KafkaStorage` with broker and prefix.
3) Call `write_event` for an event type (e.g., `OrderCancelled`).
4) Use a Kafka consumer to read the produced message from `{prefix}.OrderCancelled`.
5) Assert at least one message is received and payload is non-empty.

## Notes

- Avro schema is generated per event type; the test only validates the payload exists.
- The test uses a short timeout to avoid hanging in CI.
