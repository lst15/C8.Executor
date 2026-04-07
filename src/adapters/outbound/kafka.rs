use async_trait::async_trait;
use rdkafka::{
    ClientConfig,
    producer::{FutureProducer, FutureRecord},
};

use crate::{
    domain::{error::AppError, events::EventEnvelope},
    ports::EventPublisher,
};

pub struct KafkaEventPublisher {
    producer: FutureProducer,
    topic: String,
}

impl KafkaEventPublisher {
    pub fn new(brokers: &str, topic: &str) -> Result<Self, AppError> {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", "5000")
            .create()
            .map_err(|e| AppError::Dependency(format!("kafka producer init error: {e}")))?;

        Ok(Self {
            producer,
            topic: topic.to_string(),
        })
    }
}

#[async_trait]
impl EventPublisher for KafkaEventPublisher {
    async fn publish(&self, event: EventEnvelope) -> Result<(), AppError> {
        let payload = serde_json::to_string(&event)
            .map_err(|e| AppError::Dependency(format!("serialize event error: {e}")))?;

        let record = FutureRecord::to(&self.topic)
            .payload(&payload)
            .key(&event.run_id);

        self.producer
            .send(record, std::time::Duration::from_secs(3))
            .await
            .map_err(|(e, _)| AppError::Dependency(format!("kafka publish error: {e}")))?;

        Ok(())
    }
}
