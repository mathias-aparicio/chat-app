use anyhow::Result;
use scylla::client::{session::Session, session_builder::SessionBuilder};
pub async fn create_table() -> Result<Session> {
    let session = SessionBuilder::new()
        .known_node("127.0.0.1:9042")
        .use_keyspace("ks", false)
        .build()
        .await?;
    Ok(session)
}
