use chrono::{DateTime, Utc};
use scylla::DeserializeRow;
use uuid::Uuid;
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

#[derive(serde::Deserialize, serde::Serialize)]
pub struct ChatMessage {
    pub chat_id: Uuid,
    pub message_id: Uuid,
    pub sender_id: Uuid,
    pub content: String,
}

#[derive(DeserializeRow, serde::Deserialize, serde::Serialize, PartialEq, Debug, Clone)]
pub struct CreatMessage {
    pub content: String,
}
#[derive(serde::Serialize, serde::Deserialize)]
pub struct PandaMessage {
    pub chat_id: Uuid,
    pub sender_id: Uuid,
    pub content: String,
}
