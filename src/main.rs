#[macro_use]
extern crate log;

use axum::{
    Router,
    routing::{get, post},
};

mod config;
mod openai;

#[tokio::main]
async fn main() {
    config::configure();

    let webhook_server = Router::new()
        .route("/", get(|| async { "Sup" }))
        .route("/", post(openai::webhook::webhook));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    info!("Serving Webhook server");
    axum::serve(listener, webhook_server).await.unwrap();
}
