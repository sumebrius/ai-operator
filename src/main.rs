#[macro_use]
extern crate tracing;

use axum::{
    Router,
    routing::{get, post},
};
use tower_http::trace::TraceLayer;

use crate::config::AppState;

mod config;
mod openai;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let webhook_server = Router::new()
        .route("/", get(|| async { "Sup" }))
        .route("/", post(openai::webhook::webhook))
        .layer(TraceLayer::new_for_http())
        .with_state(AppState::start());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    info!("Serving Webhook server");
    axum::serve(listener, webhook_server).await.unwrap();
}
