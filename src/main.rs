use axum_prometheus::PrometheusMetricLayer;

use tracing_subscriber::EnvFilter;

use std::{collections::HashMap, sync::Arc};
use tokio::signal;

use crate::{
    consumer::MessageConsumer,
    db::{Db, ScyllaDb},
    handler::create_router,
    producer::{MessageProducer, Producer},
    schema::PandaMessage,
};
use anyhow::Result;
use axum::routing::get;
use opentelemetry_otlp::WithExportConfig;
use tera::Tera;
use tokio::sync::{RwLock, mpsc, watch};
use tracing_subscriber::{Registry, layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid; // Needed for with_endpoint

mod consumer;
mod db;
mod handler;
mod producer;
mod schema;
mod websocket;
// FIXME : Change me to something random
pub const NODE_ID: [u8; 6] = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06];
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
    // ************
    // --- Tracing and telemetry ---
    // ************

    // 1. Setup Logging & Filters
    let fmt_layer = tracing_subscriber::fmt::layer();
    let filter_layer =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,chat_app=trace"));

    // 2. Setup OpenTelemetry (Jaeger/OTLP)
    let env_otel_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4317".to_string());

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(env_otel_endpoint)
        .build()?;

    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .build();

    let tracer = opentelemetry::trace::TracerProvider::tracer(&provider, "chat-app");

    let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    // 3. Combine
    Registry::default()
        .with(filter_layer)
        .with(fmt_layer)
        .with(telemetry_layer) // Add the new layer
        .try_init()?;

    // ************
    // --- AppState ---
    // ************

    let scylla_host = std::env::var("SCYLLA_HOST").unwrap_or_else(|_| "localhost:9042".to_string());
    let kafka_host = std::env::var("KAFKA_HOST").unwrap_or_else(|_| "localhost:19092".to_string());
    let app_port = std::env::var("APP_PORT").unwrap_or_else(|_| "8000".to_string());
    let db = ScyllaDb::new(&scylla_host).await?;
    let db_worker = Arc::new(db);
    let db_router = db_worker.clone();
    let connections_map: ConnectionMap = Arc::new(RwLock::new(HashMap::new()));
    let tera = Arc::new(Tera::new("templates/**/*.html")?);
    let producer = Arc::new(MessageProducer::new(&kafka_host, "chat-messages")?);
    let (prometheus_layer, metric_handle) = PrometheusMetricLayer::pair();
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

    // ************
    // --- Consumer and Web Server ---
    // ************

    let consumer = MessageConsumer::new(&kafka_host, "chat-messages", &group_id)?;

    consumer.spawn_lag_monitor();

    let (shutdown_tx, mut shutdown_rx) = watch::channel(());
    let consumer_handle = tokio::spawn(async move {
        tracing::info!("Starting background consumer...");
        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    tracing::info!("Shutdown signal received, stopping consumer...");
                    break;
                }
                result = consumer.consume_messages(db_worker.clone(), connections_map.clone()) => {
                    match result {
                        Ok(_) => {
                            tracing::warn!("Consumer connection closed, restarting in 1s...");
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        }
                        Err(e) => {
                            tracing::error!("Consumer error: {:?}. Retrying in 5s...", e);
                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        }
                    }
                }
            }
        }
    });
    let app = create_router(app_state, prometheus_layer).await?;
    let app = app.route("/metrics", get(|| async move { metric_handle.render() }));
    let addr = format!("0.0.0.0:{}", app_port);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("Listening on port {}", addr);
    if let Err(e) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(shutdown_tx))
        .await
    {
        eprintln!("Server error: {}", e);
    }

    let _ = consumer_handle.await;
    Ok(())
}
async fn shutdown_signal(shutdown_tx: watch::Sender<()>) {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("Signal received, starting graceful shutdown...");
    // Notify the background consumer to stop
    let _ = shutdown_tx.send(());
}
