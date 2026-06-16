mod config;
mod handlers;
mod hub;
mod messages;
mod models;
mod pb;
mod ice;

use axum::{
    extract::{ws::WebSocketUpgrade, Query},
    http::StatusCode,
    routing::get,
    Router,
};
use dashmap::DashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{error, info};

use crate::config::Config;
use crate::hub::MatchmakingHub;

pub type SharedHub = Arc<MatchmakingHub>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing_subscriber::filter::LevelFilter::INFO.into()),
        )
        .init();

    // Load configuration
    dotenv::dotenv().ok();
    let config = Config::from_env();

    info!("Starting Tango Signaling Server");
    info!(
        "Listening on {}:{}",
        config.server_host, config.server_port
    );

    // Initialize the matchmaking hub
    let hub = Arc::new(MatchmakingHub::new());

    // Build router
    let app = Router::new()
        .route("/ok", get(health_check_ok))
        .route("/health", get(health_check_json))
        .route("/", get(index))
        .route("/ws", get(websocket_handler))
        .fallback(not_found)
        .with_state(hub);

    // Create socket address
    let addr: SocketAddr = format!("{}:{}", config.server_host, config.server_port)
        .parse()
        .expect("Failed to parse socket address");

    // Build listener
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind to address");

    info!("Server listening on {}", addr);

    // Run server
    axum::serve(listener, app)
        .await
        .expect("Server error");

    info!("Tango Signaling Server shut down");
    Ok(())
}

async fn health_check_ok() -> String {
    "ok".to_string()
}

async fn health_check_json() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({ "status": "ok" }))
}

async fn index() -> String {
    "ok".to_string()
}

async fn not_found() -> (StatusCode, String) {
    (StatusCode::NOT_FOUND, "not found".to_string())
}

#[derive(serde::Deserialize)]
pub struct WebSocketQuery {
    session_id: String,
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<WebSocketQuery>,
    axum::extract::State(hub): axum::extract::State<SharedHub>,
) -> impl axum::response::IntoResponse {
    let session_id = params.session_id;
    let hub_clone = Arc::clone(&hub);
    ws.on_upgrade(move |socket| async {
        handlers::websocket::handle(socket, session_id, hub_clone).await
    })
}
