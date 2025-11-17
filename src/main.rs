use anyhow::Result;
use tracing_subscriber::EnvFilter;

use crate::handler::create_router;

mod db;
mod handler;
#[tokio::main]
async fn main() -> Result<()> {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,chat_app=debug"));

    tracing_subscriber::fmt().with_env_filter(filter).init();

    let app = create_router().await?;
    let addr = "127.0.0.1:3000";
    let listener = tokio::net::TcpListener::bind(addr).await?;

    println!("Listening on port {}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}
