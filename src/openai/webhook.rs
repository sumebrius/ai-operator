use axum::{Json, extract::State};

use chrono::{DateTime, Utc, serde::ts_seconds};
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
