#[macro_use]
extern crate log;

use axum::{Router, routing::get};

mod config;

#[tokio::main]
async fn main() {
    config::configure();

    let webhook_server = Router::new().route("/", get(|| async { "Sup" }));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    info!("Serving Webhook server");
    axum::serve(listener, webhook_server).await.unwrap();
}
