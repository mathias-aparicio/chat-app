// FIXME : Make sure the id of message is converted to a TimeuiD
use crate::schema::PandaMessage;
use anyhow::{Context, Result};
use rdkafka::{
    ClientConfig,
    producer::{FutureProducer, FutureRecord},
    util::Timeout,
};

use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, automock)]
#[async_trait]
pub trait Producer: Send + Sync {
    async fn send_message(&self, message: PandaMessage) -> Result<()>;
}

#[derive(Clone)]
pub struct MessageProducer {
    producer: FutureProducer,
    topic: String,
}

impl MessageProducer {
    pub fn new(brokers: &str, topic: &str) -> Result<Self> {
        let producer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .create()?;

        Ok(MessageProducer {
            producer,
            topic: topic.to_string(),
        })
    }
}

#[async_trait]
impl Producer for MessageProducer {
    async fn send_message(&self, message: PandaMessage) -> Result<()> {
        let payload = serde_json::to_string(&message).context("Failed to deserialize payload")?;
        let sender_id = message.sender_id.to_string();
        self.producer
            .send(
                FutureRecord::to(&self.topic)
                    .payload(&payload)
                    .key(&sender_id),
                Timeout::Never,
            )
            .await
            // Ignore _ representing the unsend message and returns only the error that anyhow
            // accepts
            .map_err(|(e, _)| e)?;
        // FIXME : Handle the copy of message that failed to send to retry
        Ok(())
    }
}
