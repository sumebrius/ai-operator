#[macro_use]
extern crate tracing;

use axum::{
    Router,
    extract::Request,
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
};

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

    let webhook_server =
        Router::new()
            .route("/", post(webhook::webhook))
            .layer(middleware::from_fn_with_state(
                state.clone(),
                webhook::validate_webhook,
            ));

    let liveness = Router::new().route("/", get(|| async { "ok" }));

    let app = Router::new()
        .nest("/livez", liveness)
        .nest("/webhook", webhook_server)
        .layer(middleware::from_fn(trace_request))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    info!("Serving Webhook server");
    axum::serve(listener, app).await.unwrap();
}

async fn trace_request(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let uri = request.uri().clone();
    let response = next.run(request).await;
    info!(%method, %uri, status = %response.status(), "HTTP request");
    response
}
