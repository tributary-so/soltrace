use super::{EventQueue, QueueEvent};
use async_trait::async_trait;
use rdkafka::config::ClientConfig;
use rdkafka::producer::{BaseProducer, BaseRecord, Producer};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

#[derive(Clone)]
pub struct KafkaConfig {
    pub brokers: String,
}

impl KafkaConfig {
    pub fn new(brokers: String) -> Self {
        Self { brokers }
    }

    pub fn from_env() -> Option<Self> {
        let brokers = std::env::var("KAFKA_BROKERS").ok()?;
        Some(Self { brokers })
    }
}

pub struct KafkaProducer {
    producer: Arc<BaseProducer>,
}

impl KafkaProducer {
    pub fn new(config: KafkaConfig) -> anyhow::Result<Self> {
        let producer: BaseProducer = ClientConfig::new()
            .set("bootstrap.servers", &config.brokers)
            .set("message.timeout.ms", "5000")
            .set("queue.buffering.max.messages", "100000")
            .set("queue.buffering.max.kbytes", "1048576")
            .set("batch.num.messages", "1000")
            .set("client.id", "soltrace-live")
            .create()?;

        info!("Kafka producer connected to: {}", config.brokers);

        Ok(Self {
            producer: Arc::new(producer),
        })
    }
}

#[async_trait]
impl EventQueue for KafkaProducer {
    async fn send(&self, event: &QueueEvent) -> anyhow::Result<()> {
        let topic = &event.event_name;
        let key = event.signature.clone();
        let payload = serde_json::to_vec(event)?;

        let record = BaseRecord::to(topic)
            .key(&key)
            .payload(&payload);

        match self.producer.send(record) {
            Ok(_) => {
                debug!("Sent event to Kafka topic '{}': {}", topic, event.signature);
                Ok(())
            }
            Err((err, _)) => {
                error!("Failed to send to Kafka: {}", err);
                Err(anyhow::anyhow!("Kafka send error: {err}"))
            }
        }
    }

    async fn flush(&self) -> anyhow::Result<()> {
        match self.producer.flush(std::time::Duration::from_secs(5)) {
            Ok(_) => {
                debug!("Kafka producer flushed");
                Ok(())
            }
            Err(err) => {
                warn!("Kafka flush timeout: {}", err);
                Err(anyhow::anyhow!("Kafka flush timeout: {err}"))
            }
        }
    }
}

impl Drop for KafkaProducer {
    fn drop(&mut self) {
        let _ = self.producer.flush(std::time::Duration::from_secs(5));
    }
}
