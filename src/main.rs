use anyhow::Result;

use crate::handler::create_router;

mod db;
mod handler;
#[tokio::main]
async fn main() -> Result<()> {
    let app = create_router().await?;
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, app).await?;
    Ok(())
}
