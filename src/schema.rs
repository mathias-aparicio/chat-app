use chrono::{DateTime, Utc};
use scylla::{DeserializeRow, value::CqlTimeuuid};
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

#[derive(DeserializeRow, serde::Deserialize, serde::Serialize, PartialEq, Debug, Clone)]
pub struct CreatMessage {
    pub content: String,
}
#[derive(serde::Deserialize, serde::Serialize)]
pub struct LoginPayload {
    pub username: String,
}
#[derive(serde::Serialize, serde::Deserialize, Clone, DeserializeRow)]
pub struct PandaMessage {
    pub chat_id: Uuid,
    pub sender_id: Uuid,
    pub content: String,
    pub message_id: Uuid,
}

#[derive(scylla::DeserializeRow)]
pub struct RawPandaMessage {
    chat_id: Uuid,
    sender_id: Uuid,
    content: String,
    message_id: CqlTimeuuid, // Matches CQL 'Timeuuid'
}

impl RawPandaMessage {
    pub fn to_panda_message(&self) -> PandaMessage {
        PandaMessage {
            chat_id: self.chat_id,
            sender_id: self.sender_id,
            content: self.content.clone(),
            message_id: Uuid::from(self.message_id),
        }
    }
}
