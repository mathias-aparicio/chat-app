use crate::db::Db;
use anyhow::{Context, Result};
use dashmap::DashMap;
use futures_util::StreamExt;
use metrics::{counter, gauge, histogram};
use rdkafka::Message;
use rdkafka::{
    ClientConfig, Offset,
    consumer::{Consumer, StreamConsumer},
};
use std::sync::Arc;
use tokio::time::Instant;
use tokio::time::{Duration, interval};
use tracing::Instrument;
use uuid::Uuid;

use crate::{ConnectionMap, db::ScyllaDb};

pub struct MessageConsumer {
    consumer: Arc<StreamConsumer>,
    consumer_topic: String,
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

        Ok(MessageConsumer {
            consumer: Arc::new(consumer),
            consumer_topic: topic.to_string(),
        })
    }

    /// Compute how meany messages are waiting to be processed by the redpanda consumer
    pub fn spawn_lag_monitor(&self) {
        let consumer = self.consumer.clone();
        let topic = self.consumer_topic.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(5));
            loop {
                interval.tick().await;

                // 1. Fetch Metadata to discover all partitions
                let metadata = match consumer.fetch_metadata(Some(&topic), Duration::from_secs(5)) {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::warn!("Failed to fetch metadata: {:?}", e);
                        continue;
                    }
                };

                let mut total_lag: i64 = 0;

                // 2. Iterate over every partition found
                if let Some(topic_metadata) = metadata.topics().first() {
                    for partition in topic_metadata.partitions() {
                        let pid = partition.id();

                        if let Ok((_low, high)) =
                            consumer.fetch_watermarks(&topic, pid, Duration::from_secs(5))
                            && let Ok(topic_partitions) = consumer.position()
                            && let Some(elem) = topic_partitions
                                .elements()
                                .iter()
                                .find(|e| e.partition() == pid)
                            && let Offset::Offset(current_offset) = elem.offset()
                        {
                            let partition_lag = high - current_offset;
                            total_lag += partition_lag;
                        }
                    }
                }

                gauge!("consumer_lag").set(total_lag as f64);
            }
        });
    }

    pub async fn consume_messages(
        &self,
        db: Arc<ScyllaDb>,
        connections: ConnectionMap,
    ) -> Result<()> {
        let stream = self.consumer.stream();
        let members_cache: Arc<DashMap<Uuid, (Vec<Uuid>, Instant)>> = Arc::new(DashMap::new());
        const CACHE_TTL: Duration = Duration::from_secs(1);

        stream
            .for_each_concurrent(100, |message_result| {
                let db = db.clone();
                let connections = connections.clone();
                let members_cache = members_cache.clone();
                async move {
                    let result = async {
                        let message = message_result.context("Kafka consumer error")?;
                        let payload = message
                            .payload_view::<str>()
                            .context("no payload")?
                            .context("invalid utf8")?;
                        let start_total = Instant::now();
                        let parent_span = tracing::info_span!(
                            "process_message",
                            raw_message_size = payload.len()
                        );
                        let _enter = parent_span.enter();

                        let chat_message = {
                            let _span = tracing::info_span!("deserialize").entered();
                            serde_json::from_str::<crate::schema::PandaMessage>(payload)?
                        };

                        counter!("consumer_messages_processed_total").increment(1);

                        let chat_id = chat_message.chat_id;

                        // --- MEASUREMENT 1: Database Insert ---
                        let start_db = Instant::now();

                        db.insert_message(chat_message.clone())
                            .await
                            .context("Failed to insert message into DB")?;

                        histogram!("consumer_db_duration_seconds")
                            .record(start_db.elapsed().as_secs_f64());

                        // --- MEASUREMENT 2: Broadcasting ---

                        let start_broadcast = Instant::now();
                        let mut members_to_use = None;

                        async {
                            if let Some(entry) = members_cache.get(&chat_id) {
                                let (members, timestamp) = entry.value();
                                // If cache is used bellow TTL we use its value
                                if timestamp.elapsed() < CACHE_TTL {
                                    members_to_use = Some(members.clone());
                                }
                            }

                            // If we go in this then members_to_use is still None
                            // If the cache expired or missed
                            if members_to_use.is_none() {
                                let members = db.get_members_of_chat(chat_id).await?;
                                members_cache.insert(chat_id, (members.clone(), Instant::now()));
                                members_to_use = Some(members);
                            }

                            if let Some(members) = members_to_use {
                                let lock = connections.read().await;
                                for member_id in members {
                                    if let Some(sender) = lock.get(&member_id) {
                                        // Ignore send errors
                                        let _ = sender.try_send(chat_message.clone());
                                    }
                                }
                            }
                            Ok::<(), anyhow::Error>(())
                        }
                        .instrument(tracing::info_span!("broadcast_logic"))
                        .await?;

                        histogram!("consumer_broadcast_duration_seconds")
                            .record(start_broadcast.elapsed().as_secs_f64());

                        // --- MEASUREMENT 3: Total Loop ---
                        histogram!("consumer_processing_duration_seconds")
                            .record(start_total.elapsed().as_secs_f64());
                        Ok::<(), anyhow::Error>(())
                    }
                    .await;

                    // Handle the error (log it) so the stream returns () and keeps going
                    if let Err(e) = result {
                        tracing::error!("Error processing message: {:?}", e);
                    }
                }
            })
            .await;
        Ok(())
    }
}
