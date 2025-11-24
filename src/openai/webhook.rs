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
    object: String,
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

    pub fn call_id(&self) -> &str {
        &self.data.call_id
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct RealtimeCallIncomingData {
    call_id: String,
    #[serde(default)]
    sip_headers: Vec<SipHeader>,
}

#[derive(Serialize, Deserialize, Debug)]
struct SipHeader {
    name: String,
    value: String,
}

pub async fn webhook(State(state): State<AppState>, Json(payload): Json<RealtimeCallIncoming>) {
    info_span!("webhook", webhook.id = payload.get_id()).in_scope(|| info!("Webhook received"));
    tokio::spawn(handle_call(state, payload));
}

pub async fn validate_webhook(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    if state.webhook().is_none() {
        return next.run(request).await;
    }

    let (parts, body) = request.into_parts();
    let payload = match to_bytes(body, usize::MAX).await {
        Ok(payload) => payload,
        Err(err) => {
            warn!("Bad Payload received: {}", err);
            return (StatusCode::BAD_REQUEST, "Invalid payload").into_response();
        }
    };

    // let content = String::from_utf8_lossy(&payload);
    // debug!("Incoming content: {}", content);

    if let Some(webhook) = state.webhook() {
        let headers = &parts.headers;
        if webhook.verify(&payload, headers).is_err() {
            warn!("Unvalidated webhook call");
            return (StatusCode::UNAUTHORIZED, "Invalid authentication").into_response();
        }
    }
    debug!("Webhook passed validation");

    let request = Request::from_parts(parts, Body::from(payload));
    next.run(request).await
}
