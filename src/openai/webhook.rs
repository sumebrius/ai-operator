use axum::{
    Json,
    body::{Body, to_bytes},
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
};

use chrono::{DateTime, Utc, serde::ts_seconds};
use http::StatusCode;
use serde::{self, Deserialize, Serialize};

use crate::{config::AppState, openai::control::handle_call};

#[derive(Serialize, Deserialize, Debug)]
pub struct RealtimeCallIncoming {
    id: String,
    #[serde(rename = "type")]
    event_type: String,
    #[serde(with = "ts_seconds")]
    created_at: DateTime<Utc>,
    data: RealtimeCallIncomingData,
}

impl RealtimeCallIncoming {
    pub fn get_id(&self) -> &str {
        &self.id
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct RealtimeCallIncomingData {
    call_id: String,
    sip_headers: Vec<SipHeader>,
}

#[derive(Serialize, Deserialize, Debug)]
struct SipHeader {
    name: String,
    value: String,
}

pub async fn webhook(State(state): State<AppState>, Json(payload): Json<RealtimeCallIncoming>) {
    info_span!("webhook", call.id = payload.get_id()).in_scope(|| info!("Webhook received"));
    tokio::spawn(handle_call(state, payload));
}

pub async fn validate_webhook(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let (parts, body) = request.into_parts();
    let payload = match to_bytes(body, usize::MAX).await {
        Ok(payload) => payload,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, "Invalid payload").into_response();
        }
    };

    if let Some(webhook) = state.webhook() {
        let headers = &parts.headers;
        if webhook.verify(&payload, headers).is_err() {
            return (StatusCode::UNAUTHORIZED, "Invalid authentication").into_response();
        }
    }

    let request = Request::from_parts(parts, Body::from(payload));
    next.run(request).await
}
