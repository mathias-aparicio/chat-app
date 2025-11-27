use std::sync::Arc;

use anyhow::Result;
use tracing_subscriber::EnvFilter;

use crate::db::ScyllaDb;
use crate::handler::create_router;

mod consumer;
mod db;
mod handler;
mod producer;
mod schema;
#[tokio::main]
async fn main() -> Result<()> {
    // TODO : Allow axum_anyhow errors to fit in the tracing
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,chat_app=debug"));

    tracing_subscriber::fmt().with_env_filter(filter).init();
    let db = ScyllaDb::new().await?;
    let db_worker = Arc::new(db);
    let db_router = db_worker.clone();
    let group_id = format!(
        "{}_{}",
        "chat_group",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    );
    let consumer = consumer::MessageConsumer::new("localhost:19092", "chat-messages", &group_id)?;
    let consumer_handle = tokio::task::spawn(async move {
        consumer.consume_messages(db_worker.clone()).await?;
        anyhow::Ok(())
    });
    let app = create_router(db_router.clone()).await?;
    let addr = "127.0.0.1:3000";
    let listener = tokio::net::TcpListener::bind(addr).await?;

    println!("Listening on port {}", addr);
    // TODO : Implement gracefull shutdown
    tokio::select! {
        result = axum::serve(listener, app) => {
            if let Err(e) = result {
                eprintln!("Server error: {}", e);
            }
        }
        _ = consumer_handle => {
            eprintln!("Consumer task stopped unexpectedly!");
        }
    }

    Ok(())
}
