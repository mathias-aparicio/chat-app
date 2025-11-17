use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use scylla::client::{session::Session, session_builder::SessionBuilder};
use scylla::{DeserializeRow, SerializeRow};
use uuid::Uuid;

#[async_trait]
pub trait Db: Send + Sync {
    // TODO : Write two helper function get/post ressource to remove duplication code
    async fn create_user(&self, username: &str) -> Result<User>;
    async fn create_chat(&self, name: &str, members: &[Uuid]) -> Result<Chat>;
    async fn get_user(&self, user_id: &str) -> Result<User>;
    async fn get_chat(&self, chat_id: &str) -> Result<Chat>;
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
}

#[async_trait]
impl Db for ScyllaDb {
    async fn get_user(&self, user_id: &str) -> Result<User> {
        let user: User = self
            .session
            .query_unpaged("SELECT * FROM ks.users WHERE user_id = ?", (user_id,))
            .await
            .context("Could not select rows from users")?
            .into_rows_result()
            .context("Could not gather into rows from users")?
            .first_row()
            .context("Could not get first row from users")?;

        Ok(user)
    }
    async fn get_chat(&self, chat_id: &str) -> Result<Chat> {
        let chat: Chat = self
            .session
            .query_unpaged("SELECT * FROM ks.chat WHERE chat_id = ?", (chat_id,))
            .await?
            .into_rows_result()?
            .first_row()?;

        Ok(chat)
    }
    async fn create_user(&self, username: &str) -> Result<User> {
        let now = Utc::now();
        let user_id = Uuid::new_v4();
        self.session
            .query_unpaged(
                "INSERT INTO ks.users (user_id, username, created_at, updated_at) VALUES (?, ?, ?, ?)",
                (user_id, username, now, now),
            )
            .await?;

        let user: User = self
            .session
            .query_unpaged("SELECT * FROM ks.users WHERE user_id = ?", (user_id,))
            .await?
            .into_rows_result()?
            .first_row()?;

        Ok(user)
    }

    async fn create_chat(&self, name: &str, members: &[Uuid]) -> Result<Chat> {
        let now = Utc::now();
        let chat_id = Uuid::new_v4();
        self.session
            .query_unpaged(
                "INSERT INTO ks.chats (chat_id, members, name, created_at) VALUES (?, ?, ?, ?)",
                (chat_id, members, name, now),
            )
            .await
            .context("Failed to insert chat")?;

        let chat: Chat = self
            .session
            .query_unpaged("SELECT * FROM ks.chats WHERE chat_id = ?", (chat_id,))
            .await?
            .into_rows_result()?
            .first_row()?;

        Ok(chat)
    }
}

#[derive(serde::Deserialize)]
pub struct CreateUser {
    pub username: String,
}

#[derive(DeserializeRow, serde::Deserialize, serde::Serialize, PartialEq, Debug, Clone)]
pub struct User {
    pub user_id: Uuid,
    pub username: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(DeserializeRow, serde::Deserialize, serde::Serialize, PartialEq, Debug, Clone)]
pub struct Chat {
    pub chat_id: Uuid,
    pub members: Vec<Uuid>,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(serde::Deserialize)]
pub struct CreateChat {
    pub name: String,
    pub members: Vec<Uuid>,
}
