use anyhow::{Result, anyhow};
use arrow::array::{ArrayRef, StringArray, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::file::properties::WriterProperties;
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

#[cfg(feature = "kafka")]
use apache_avro::{Schema as AvroSchema, Writer as AvroWriter, types::Record};
use async_trait::async_trait;

#[cfg(feature = "kafka")]
use rdkafka::config::ClientConfig;
#[cfg(feature = "kafka")]
use rdkafka::producer::{FutureProducer, FutureRecord};
#[cfg(feature = "kafka")]
use rdkafka::util::Timeout;

// Simplified storage structure for MVP
#[async_trait]
pub trait EventSink: Send {
    async fn write_event(
        &mut self,
        event_type: &str,
        block_number: u64,
        tx_hash: &str,
        log_index: u64,
        columns: &[(&str, String)],
    ) -> Result<()>;

    async fn rotate(&mut self) -> Result<()>;

    async fn close(&mut self) -> Result<()>;
}

pub struct ParquetStorage {
    base_path: String,
    writers: HashMap<String, ArrowWriter<File>>,
    schemas: HashMap<String, Arc<Schema>>,
    file_set_id: u64,
}

impl ParquetStorage {
    pub fn new(base_path: &str) -> Self {
        Self {
            base_path: base_path.to_string(),
            writers: HashMap::new(),
            schemas: HashMap::new(),
            file_set_id: 0,
        }
    }

    fn base_fields() -> Vec<Field> {
        vec![
            Field::new("event_type", DataType::Utf8, false),
            Field::new("block_number", DataType::UInt64, false),
            Field::new("transaction_hash", DataType::Utf8, false),
            Field::new("log_index", DataType::UInt64, false),
        ]
    }

    fn expected_schema_names(columns: &[&str]) -> Vec<String> {
        let mut names = vec![
            "event_type".to_string(),
            "block_number".to_string(),
            "transaction_hash".to_string(),
            "log_index".to_string(),
        ];
        names.extend(columns.iter().map(|name| (*name).to_string()));
        names
    }

    fn schema_names(schema: &Schema) -> Vec<String> {
        schema
            .fields()
            .iter()
            .map(|field| field.name().to_string())
            .collect()
    }

    fn get_or_create_schema(&mut self, event_type: &str, columns: &[&str]) -> Result<Arc<Schema>> {
        if let Some(schema) = self.schemas.get(event_type) {
            let expected = Self::expected_schema_names(columns);
            let actual = Self::schema_names(schema);
            if expected != actual {
                return Err(anyhow!("schema mismatch for event type '{event_type}'"));
            }

            return Ok(schema.clone());
        }

        let mut fields = Self::base_fields();
        for name in columns {
            fields.push(Field::new(*name, DataType::Utf8, false));
        }

        let schema = Arc::new(Schema::new(fields));
        self.schemas.insert(event_type.to_string(), schema.clone());
        Ok(schema)
    }

    pub fn close_writers(&mut self) -> Result<()> {
        for (_, writer) in self.writers.drain() {
            writer.close()?;
        }

        Ok(())
    }

    pub fn rotate_writers(&mut self) -> Result<()> {
        self.close_writers()?;

        self.file_set_id = self.file_set_id.saturating_add(1);
        Ok(())
    }

    pub fn write_event(
        &mut self,
        event_type: &str,
        block_number: u64,
        tx_hash: &str,
        log_index: u64,
        columns: &[(&str, String)],
    ) -> Result<()> {
        let column_names = columns.iter().map(|(name, _)| *name).collect::<Vec<_>>();
        let schema = self.get_or_create_schema(event_type, &column_names)?;

        // Ensure directory exists
        let dir_path = Path::new(&self.base_path).join(event_type);
        if !dir_path.exists() {
            std::fs::create_dir_all(&dir_path)?;
        }

        // Initialize writer if not exists for this event type
        if !self.writers.contains_key(event_type) {
            let file_path = dir_path.join(format!(
                "{}_set{}.parquet",
                chrono::Utc::now().timestamp_millis(),
                self.file_set_id
            ));
            let file = File::create(file_path)?;
            let props = WriterProperties::builder().build();
            let writer = ArrowWriter::try_new(file, schema.clone(), Some(props))?;
            self.writers.insert(event_type.to_string(), writer);
        }

        // Create arrays directly
        let event_type_array = StringArray::from(vec![event_type]);
        let block_number_array = UInt64Array::from(vec![block_number]);
        let tx_hash_array = StringArray::from(vec![tx_hash]);
        let log_index_array = UInt64Array::from(vec![log_index]);
        let mut arrays: Vec<ArrayRef> = vec![
            Arc::new(event_type_array),
            Arc::new(block_number_array),
            Arc::new(tx_hash_array),
            Arc::new(log_index_array),
        ];

        for (_, value) in columns {
            arrays.push(Arc::new(StringArray::from(vec![value.clone()])));
        }

        let batch = RecordBatch::try_new(schema, arrays)?;

        let writer = self.writers.get_mut(event_type).unwrap();
        writer.write(&batch)?;

        Ok(())
    }
}

#[async_trait]
impl EventSink for ParquetStorage {
    async fn write_event(
        &mut self,
        event_type: &str,
        block_number: u64,
        tx_hash: &str,
        log_index: u64,
        columns: &[(&str, String)],
    ) -> Result<()> {
        ParquetStorage::write_event(self, event_type, block_number, tx_hash, log_index, columns)
    }

    async fn rotate(&mut self) -> Result<()> {
        ParquetStorage::rotate_writers(self)
    }

    async fn close(&mut self) -> Result<()> {
        ParquetStorage::close_writers(self)
    }
}

#[cfg(feature = "kafka")]
pub struct KafkaStorage {
    producer: FutureProducer,
    topic_prefix: String,
    schemas: HashMap<String, AvroSchema>,
}

#[cfg(feature = "kafka")]
impl KafkaStorage {
    pub fn new(brokers: &str, topic_prefix: &str) -> Result<Self> {
        let producer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", "5000")
            .create()?;

        Ok(Self {
            producer,
            topic_prefix: topic_prefix.to_string(),
            schemas: HashMap::new(),
        })
    }

    fn expected_schema_names(columns: &[&str]) -> Vec<String> {
        let mut names = vec![
            "event_type".to_string(),
            "block_number".to_string(),
            "transaction_hash".to_string(),
            "log_index".to_string(),
        ];
        names.extend(columns.iter().map(|name| (*name).to_string()));
        names
    }

    fn schema_field_names(schema: &AvroSchema) -> Vec<String> {
        match schema {
            AvroSchema::Record(record) => record
                .fields
                .iter()
                .map(|field| field.name.clone())
                .collect(),
            _ => Vec::new(),
        }
    }

    fn get_or_create_schema(&mut self, event_type: &str, columns: &[&str]) -> Result<AvroSchema> {
        if let Some(schema) = self.schemas.get(event_type) {
            let expected = Self::expected_schema_names(columns);
            let actual = Self::schema_field_names(schema);
            if expected != actual {
                return Err(anyhow!("schema mismatch for event type '{event_type}'"));
            }

            return Ok(schema.clone());
        }

        let mut fields = vec![
            serde_json::json!({"name": "event_type", "type": "string"}),
            serde_json::json!({"name": "block_number", "type": "long"}),
            serde_json::json!({"name": "transaction_hash", "type": "string"}),
            serde_json::json!({"name": "log_index", "type": "long"}),
        ];

        for name in columns {
            fields.push(serde_json::json!({"name": name, "type": "string"}));
        }

        let schema_json = serde_json::json!({
            "type": "record",
            "name": event_type,
            "fields": fields,
        });

        let schema = AvroSchema::parse_str(&schema_json.to_string())?;
        self.schemas.insert(event_type.to_string(), schema.clone());
        Ok(schema)
    }

    fn topic_name(&self, event_type: &str) -> String {
        format!("{}.{}", self.topic_prefix, event_type)
    }
}

#[cfg(feature = "kafka")]
#[async_trait]
impl EventSink for KafkaStorage {
    async fn write_event(
        &mut self,
        event_type: &str,
        block_number: u64,
        tx_hash: &str,
        log_index: u64,
        columns: &[(&str, String)],
    ) -> Result<()> {
        let column_names = columns.iter().map(|(name, _)| *name).collect::<Vec<_>>();
        let schema = self.get_or_create_schema(event_type, &column_names)?;

        let mut record = Record::new(&schema)
            .ok_or_else(|| anyhow!("failed to build avro record for event type '{event_type}'"))?;

        record.put("event_type", event_type);
        record.put("block_number", block_number as i64);
        record.put("transaction_hash", tx_hash);
        record.put("log_index", log_index as i64);

        for (name, value) in columns {
            record.put(*name, value.as_str());
        }

        let mut writer = AvroWriter::new(&schema, Vec::new());
        writer.append(record)?;
        let payload = writer.into_inner()?;

        let topic = self.topic_name(event_type);
        let record = FutureRecord::to(&topic).payload(&payload).key(tx_hash);

        self.producer
            .send(record, Timeout::After(std::time::Duration::from_secs(0)))
            .await
            .map_err(|(error, _)| anyhow!(error))?;
        Ok(())
    }

    async fn rotate(&mut self) -> Result<()> {
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::ParquetStorage;
    use parquet::file::reader::{FileReader, SerializedFileReader};

    #[test]
    fn write_event_creates_parquet_file_and_single_row_group() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let base_path = temp_dir.path().to_string_lossy().to_string();

        let mut storage = ParquetStorage::new(&base_path);
        let columns = vec![("order_hash", "0xabc".to_string())];
        for i in 0..10 {
            storage
                .write_event("OrderCancelled", 1000 + i, "0xabc", i, &columns)
                .expect("write event");
        }
        storage.close_writers().expect("close writers");

        let event_dir = temp_dir.path().join("OrderCancelled");
        let mut files = std::fs::read_dir(event_dir)
            .expect("read event dir")
            .map(|entry| entry.expect("dir entry").path())
            .collect::<Vec<_>>();
        files.sort();

        assert_eq!(files.len(), 1, "expected one parquet file");

        let file = std::fs::File::open(&files[0]).expect("open parquet file");
        let reader = SerializedFileReader::new(file).expect("create parquet reader");
        let metadata = reader.metadata().file_metadata();

        assert_eq!(metadata.num_rows(), 10);
        assert_eq!(reader.metadata().num_row_groups(), 1);
    }

    #[test]
    fn rotate_writers_starts_new_file_set() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let base_path = temp_dir.path().to_string_lossy().to_string();

        let mut storage = ParquetStorage::new(&base_path);
        let columns = vec![("order_hash", "0x1".to_string())];
        storage
            .write_event("OrderCancelled", 1, "0x1", 1, &columns)
            .expect("write first event");
        storage.rotate_writers().expect("rotate writers");
        let columns = vec![("order_hash", "0x2".to_string())];
        storage
            .write_event("OrderCancelled", 2, "0x2", 2, &columns)
            .expect("write second event");
        storage.close_writers().expect("close writers");

        let event_dir = temp_dir.path().join("OrderCancelled");
        let files = std::fs::read_dir(event_dir)
            .expect("read event dir")
            .count();

        assert_eq!(files, 2, "expected one file per file set");
    }
}
