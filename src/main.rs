#[macro_use]
extern crate tracing;

use axum::{
    Router, middleware,
    routing::{get, post},
};
use tower_http::trace::TraceLayer;

use crate::{config::AppState, openai::webhook};

mod config;
mod contacts;
mod openai;

#[tokio::main]
async fn main() {
    #[cfg(debug_assertions)]
    {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();
    }
    #[cfg(not(debug_assertions))]
    {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .init();
    }

    let state = AppState::init();

    let webhook_server = Router::new()
        .route("/", get(|| async { "Sup" }))
        .route("/", post(webhook::webhook))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            webhook::validate_webhook,
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    info!("Serving Webhook server");
    axum::serve(listener, webhook_server).await.unwrap();
}
