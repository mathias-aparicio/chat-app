use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use scylla::{
    client::{session::Session, session_builder::SessionBuilder},
    deserialize::row::DeserializeRow as DeserializeRowTrait,
    serialize::row::SerializeRow,
    statement::batch::Batch,
    value::CqlTimeuuid,
};
use uuid::Uuid;

use crate::{
    NODE_ID,
    schema::{Chat, PandaMessage, RawPandaMessage, User},
};

#[async_trait]
pub trait Db: Send + Sync {
    async fn insert_batch_message(&self, messages: &[PandaMessage]) -> Result<()>;
    async fn insert_message(&self, message: PandaMessage) -> Result<PandaMessage>;
    async fn create_user(&self, username: &str) -> Result<User>;
    async fn create_chat(&self, name: &str, members: &[Uuid]) -> Result<Chat>;
    async fn get_user(&self, user_id: Uuid) -> Result<User>;
    async fn get_chat(&self, chat_id: Uuid) -> Result<Chat>;
    async fn get_messages(&self, chat_id: Uuid) -> Result<Vec<PandaMessage>>;
    async fn get_chats_for_user(&self, user_id: Uuid) -> Result<Vec<Chat>>;
    async fn get_user_by_username(&self, username: &str) -> Result<User>;
    async fn get_all_users(&self) -> Result<Vec<User>>;
    async fn get_members_of_chat(&self, chat_id: Uuid) -> Result<Vec<Uuid>>;
}

pub struct ScyllaDb {
    session: Session,
}

impl ScyllaDb {
    pub async fn new(host: &str) -> Result<Self> {
        let session = SessionBuilder::new()
            .known_node(host)
            .use_keyspace("ks", false)
            .build()
            .await?;
        Ok(Self { session })
    }
    /// Generic helper to execute a SELECT statement     
    async fn fetch_single<T>(&self, query: &str, values: impl SerializeRow) -> Result<T>
    where
        T: for<'f, 'm> DeserializeRowTrait<'f, 'm>,
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
    async fn insert_batch_message(&self, messages: &[PandaMessage]) -> Result<()> {
        let mut batch: Batch = Default::default();
        let mut batch_values = Vec::new();
        for msg in messages {
            batch.append_statement(
            "INSERT INTO ks.messages (message_id, chat_id, sender_id, content) VALUES (?, ?, ?, ?)",
        );
            batch_values.push((
                scylla::value::CqlTimeuuid::from(msg.message_id),
                msg.chat_id,
                msg.sender_id,
                &msg.content,
            ));
        }

        self.session.batch(&batch, batch_values).await?;
        Ok(())
    }
    // TODO : Factorise function that fetch multiple rows
    async fn get_all_users(&self) -> Result<Vec<User>> {
        let users = self
            .session
            .query_unpaged("SELECT * FROM ks.users", &[])
            .await
            .context("Failed to execute query")?
            .into_rows_result()
            .context("Failed to parse rows result")?
            .rows()
            .context("No users found")?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(users)
    }

    async fn get_members_of_chat(&self, chat_id: Uuid) -> Result<Vec<Uuid>> {
        let (members,): (Vec<Uuid>,) = self
            .fetch_single(
                "SELECT members from ks.chats WHERE chat_id = ?",
                ((chat_id),),
            )
            .await
            .context("Could not fetch members of chat")?;

        Ok(members)
    }
    async fn get_messages(&self, chat_id: Uuid) -> Result<Vec<PandaMessage>> {
        // Because Uuid can not be compared with a Timeuuid and serialize it back
        let rows_result = self
            .session
            .query_unpaged(
                "SELECT chat_id, sender_id, content, message_id FROM ks.messages WHERE chat_id = ?",
                (chat_id,),
            )
            .await
            .context("Failed to execute query")?
            .into_rows_result()
            .context("Failed to parse rows result")?;

        let messages = rows_result
            .rows::<RawPandaMessage>()
            .context("Failed to access rows iterator")?
            .map(|row_result| {
                row_result
                    .map(|raw| raw.to_panda_message())
                    .context("Failed to deserialize row")
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(messages)
    }

    async fn get_chats_for_user(&self, user_id: Uuid) -> Result<Vec<Chat>> {
        let chats = self
            .session
            .query_unpaged(
                "SELECT * FROM ks.chats WHERE members CONTAINS ? ALLOW FILTERING",
                ((user_id),),
            )
            .await
            .context("Failed to execute query")?
            .into_rows_result()
            .context("Failed to parse rows result")?
            .rows()
            .context("No Rows not found")?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(chats)
    }

    async fn get_user_by_username(&self, username: &str) -> Result<User> {
        let (user_id,): (Uuid,) = self
            .fetch_single(
                "SELECT user_id FROM ks.users_by_username WHERE username = ?",
                (username,),
            )
            .await
            .context("User not found")?;

        self.get_user(user_id).await
    }
    async fn insert_message(&self, message: PandaMessage) -> Result<PandaMessage> {
        let message_id = CqlTimeuuid::from(message.message_id);
        let chat_id = message.chat_id;
        let sender_id = message.sender_id;
        let content = message.content;
        let values = (message_id, chat_id, sender_id, content.clone());
        self.insert_data(
            "INSERT INTO ks.messages (message_id, chat_id, sender_id, content) VALUES (?, ?, ?, ?)",
            values,
        )
        .await?;
        let message_id = Uuid::from(message_id);
        Ok(PandaMessage {
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
        self.fetch_single("SELECT * FROM ks.chats WHERE chat_id = ?", (chat_id,))
            .await
            .context("Could not fetch chat")
    }

    async fn create_user(&self, username: &str) -> Result<User> {
        let now = Utc::now();
        let user_id = Uuid::now_v1(&NODE_ID);

        // Try to insert into users_by_username first to ensure uniqueness
        let applied = self
            .session
            .query_unpaged(
                "INSERT INTO ks.users_by_username (username, user_id) VALUES (?, ?) IF NOT EXISTS",
                (username, user_id),
            )
            .await?
            .into_rows_result()?
            .first_row::<(bool, Option<String>, Option<Uuid>)>()?
            .0;

        if !applied {
            return Err(anyhow::anyhow!("Username '{}' already exists", username));
        }

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
        let chat_id = Uuid::now_v1(&NODE_ID);

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
