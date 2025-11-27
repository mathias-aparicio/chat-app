use std::sync::Arc;

// FIXME : Make sure the id of message is converted to a TimeuiD
use tokio_stream::StreamExt;

use anyhow::{Context, Result};
use rdkafka::{
    ClientConfig, Message,
    consumer::{Consumer, StreamConsumer},
};

use crate::db::{Db, ScyllaDb};

pub struct MessageConsumer {
    consumer: StreamConsumer,
}

impl MessageConsumer {
    pub fn new(broker: &str, topic: &str, group_id: &str) -> Result<MessageConsumer> {
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", broker)
            .set("group.id", group_id)
            .create()
            .context("Failed to create background consumer")?;

        consumer
            .subscribe(&[topic])
            .context("Consumer failed to subscribe to topic")?;
        Ok(MessageConsumer { consumer })
    }
    /// ScyllaDB and Websocket
    pub async fn consume_messages(&self, db: Arc<ScyllaDb>) -> Result<()> {
        // TODO : Handle errors with ? and tracing
        let mut stream = self.consumer.stream();

        while let Some(message_result) = stream.next().await {
            let message = message_result.context("Kafka consumer error")?;

            let payload = message
                .payload_view::<str>()
                .context("Failed to get message payload view")?
                .context("Error while deserializing message payload")?;

            let chat_message = serde_json::from_str::<crate::schema::PandaMessage>(payload)
                .context("Error while deserializing message payload")?;

            db.insert_message(chat_message)
                .await
                .context("Failed to insert message into DB")?;
        }
        Ok(())
    }
}
