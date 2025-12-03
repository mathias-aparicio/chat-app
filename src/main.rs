use crate::consumer::MessageConsumer;
use crate::db::Db;
use crate::producer::Producer;
use crate::schema::PandaMessage;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::mpsc;
use uuid::Uuid;

use anyhow::Result;
use tera::Tera;
use tokio::sync::RwLock;
use tracing_subscriber::EnvFilter;

use crate::db::ScyllaDb;
use crate::handler::create_router;
use crate::producer::MessageProducer;

mod consumer;
mod db;
mod handler;
mod producer;
mod schema;
mod websocket;
type UserSender = mpsc::Sender<PandaMessage>;
// We use a Rwlock and not a Mutex because tokio::sync::Rwlock for frequent read, less frequent
// write
pub type ConnectionMap = Arc<RwLock<HashMap<Uuid, UserSender>>>;
#[derive(Clone)]
pub struct AppState {
    // Polymorphism  allow testing with the mockall library
    db: Arc<dyn Db>,
    producer: Arc<dyn Producer>,
    tera: Arc<Tera>,
    pub connections_map: ConnectionMap,
}
#[tokio::main]
async fn main() -> Result<()> {
    // TODO : Allow axum_anyhow errors to fit in the tracing
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,chat_app=debug"));

    tracing_subscriber::fmt().with_env_filter(filter).init();
    let db = ScyllaDb::new().await?;
    let db_worker = Arc::new(db);
    let db_router = db_worker.clone();
    let connections_map: ConnectionMap = Arc::new(RwLock::new(HashMap::new()));
    let tera = Arc::new(Tera::new("templates/**/*.html")?);
    let producer = Arc::new(MessageProducer::new("localhost:19092", "chat-messages")?);

    let app_state = AppState {
        db: db_router,
        producer: producer.clone(),
        tera: tera.clone(),
        connections_map: connections_map.clone(), // Clone 1 for Router
    };

    let group_id = format!(
        "{}_{}",
        "chat_group",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    );
    let consumer = MessageConsumer::new("localhost:19092", "chat-messages", &group_id)?;
    let consumer_handle = tokio::spawn(async move {
        tracing::info!("Starting background consumer...");
        if let Err(e) = consumer.consume_messages(db_worker, connections_map).await {
            tracing::error!("Consumer failed: {:?}", e);
        }
    });

    let app = create_router(app_state).await?;
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
