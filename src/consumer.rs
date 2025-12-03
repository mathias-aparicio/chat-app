use std::sync::Arc;

use tokio_stream::StreamExt;

use anyhow::{Context, Result};
use rdkafka::{
    ClientConfig, Message,
    consumer::{Consumer, StreamConsumer},
};

use crate::{
    ConnectionMap,
    db::{Db, ScyllaDb},
};

pub struct MessageConsumer {
    consumer: StreamConsumer,
}
// FIXME : Make sure the id of message is converted to a TimeuiD

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
    pub async fn consume_messages(
        &self,
        db: Arc<ScyllaDb>,
        connections: ConnectionMap,
    ) -> Result<()> {
        let mut stream = self.consumer.stream();

        while let Some(message_result) = stream.next().await {
            let message = message_result.context("Kafka consumer error")?;

            let payload = message
                .payload_view::<str>()
                .context("Failed to get message payload view")?
                .context("Error while deserializing message payload")?;

            let chat_message = serde_json::from_str::<crate::schema::PandaMessage>(payload)
                .context("Error while deserializing message payload")?;

            let chat_id = chat_message.chat_id;
            // 1. Insert messages to database
            db.insert_message(chat_message.clone())
                .await
                .context("Failed to insert message into DB")?;

            // 2. Send the message to all the user_id in the chat
            let members = db.get_members_of_chat(chat_id).await?;
            // FIXME : This looks like a very naive implementation
            for (user_id, sender) in connections.read().await.iter() {
                if members.contains(user_id) {
                    sender.send(chat_message.clone()).await?;
                }
            }
        }
        Ok(())
    }
}
