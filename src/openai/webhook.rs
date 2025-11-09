use axum::Json;

use chrono::{DateTime, Utc, serde::ts_seconds};
use serde::{self, Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct RealtimeCallIncoming {
    id: String,
    #[serde(rename = "type")]
    event_type: String,
    #[serde(with = "ts_seconds")]
    created_at: DateTime<Utc>,
    data: RealtimeCallIncomingData,
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

pub async fn webhook(Json(payload): Json<RealtimeCallIncoming>) {
    info!("{:?}", payload);
}
