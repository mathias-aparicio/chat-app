use crate::schema::{Chat, ChatMessage, PandaMessage, User};
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use scylla::{
    client::{session::Session, session_builder::SessionBuilder},
    deserialize::row::DeserializeRow,
    serialize::row::SerializeRow,
    value::CqlTimeuuid,
};
use uuid::Uuid;

#[async_trait]
pub trait Db: Send + Sync {
    async fn insert_message(&self, message: PandaMessage) -> Result<ChatMessage>;
    async fn create_user(&self, username: &str) -> Result<User>;
    async fn create_chat(&self, name: &str, members: &[Uuid]) -> Result<Chat>;
    async fn get_user(&self, user_id: Uuid) -> Result<User>;
    async fn get_chat(&self, chat_id: Uuid) -> Result<Chat>;
}

pub struct ScyllaDb {
    session: Session,
}

impl ScyllaDb {
    pub async fn new() -> Result<Self> {
        let session = SessionBuilder::new()
            .known_node("127.0.0.1:9042")
            .use_keyspace("ks", false)
            .build()
            .await?;
        Ok(Self { session })
    }
    /// Generic helper to execute a SELECT statement     
    async fn fetch_single<T>(&self, query: &str, values: impl SerializeRow) -> Result<T>
    where
        T: for<'f, 'm> DeserializeRow<'f, 'm>,
    {
        self.session
            .query_unpaged(query, values)
            .await
            .context("Failed to execute query")?
            .into_rows_result()
            .context("Failed to parse rows result")?
            .first_row()
            .context("Row not found")
    }

    /// Generic helper to execute an INSERT statement
    async fn insert_data(&self, query: &str, values: impl SerializeRow) -> Result<()> {
        self.session
            .query_unpaged(query, values)
            .await
            .context("Failed to execute insert query")?;
        Ok(())
    }
}

#[async_trait]
impl Db for ScyllaDb {
    async fn insert_message(&self, message: PandaMessage) -> Result<ChatMessage> {
        let message_id = CqlTimeuuid::from(Uuid::new_v4());
        let chat_id = message.chat_id;
        let sender_id = message.sender_id;
        let content = message.content;
        let values = (message_id, chat_id, sender_id, content.clone());
        self.insert_data(
            "INSERT INTO ks.messages (message_id, chat_id, sender_id, content)",
            values,
        )
        .await?;
        let message_id = Uuid::from(message_id);
        Ok(ChatMessage {
            message_id,
            content,
            chat_id,
            sender_id,
        })
    }
    async fn get_user(&self, user_id: Uuid) -> Result<User> {
        self.fetch_single("SELECT * FROM ks.users WHERE user_id = ?", (user_id,))
            .await
            .context("Could not fetch user")
    }

    async fn get_chat(&self, chat_id: Uuid) -> Result<Chat> {
        self.fetch_single("SELECT * FROM ks.chat WHERE chat_id = ?", (chat_id,))
            .await
            .context("Could not fetch chat")
    }

    async fn create_user(&self, username: &str) -> Result<User> {
        let now = Utc::now();
        let user_id = Uuid::new_v4();
        let values = (user_id, username, now, now);
        self.insert_data(
            "INSERT INTO ks.users (user_id, username, created_at, updated_at) VALUES (?, ?, ?, ?)",
            values,
        )
        .await
        .context("Failed to create user")?;

        Ok(User {
            user_id,
            username: username.to_string(),
            created_at: now,
            updated_at: now,
        })
    }

    async fn create_chat(&self, name: &str, members: &[Uuid]) -> Result<Chat> {
        let now = Utc::now();
        let chat_id = Uuid::new_v4();

        let values = (chat_id, members, name, now);

        self.insert_data(
            "INSERT INTO ks.chats (chat_id, members, name, created_at) VALUES (?, ?, ?, ?)",
            values,
        )
        .await
        .context("Failed to create chat")?;

        Ok(Chat {
            chat_id,
            members: members.to_vec(),
            name: name.to_string(),
            created_at: now,
        })
    }
}
