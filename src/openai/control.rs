use bytes::Bytes;
use reqwest::{Client, Response};
use serde::{self, Serialize};

use crate::{
    config::{API_ROOT, AppState},
    openai::webhook::RealtimeCallIncoming,
};

pub async fn handle_call(state: AppState, call: RealtimeCallIncoming) {
    let control_client = OpenAiControlSession::new(&state, call);
    let prompt = state.prompt();

    let _ = control_client
        .execute(AcceptCall::with_prompt(prompt))
        .await;
}

pub struct OpenAiControlSession {
    client: Client,
    token: String,
    call_id: String,
}

impl OpenAiControlSession {
    pub fn new(state: &AppState, call: RealtimeCallIncoming) -> Self {
        Self {
            client: Client::new(),
            token: state.openai_key().to_string(),
            call_id: call.get_id().to_string(),
        }
    }

    pub async fn execute(&self, action: impl OpenApiCall) -> Result<Response, reqwest::Error> {
        self.client
            .post(action.get_url(&self.call_id))
            .bearer_auth(&self.token)
            .json(&action)
            .send()
            .await
    }
}

pub trait OpenApiCall: Serialize {
    fn get_url(&self, call_id: &str) -> String;
}

#[derive(Serialize, Debug)]
pub struct AcceptCall {
    #[serde(rename = "type")]
    event_type: String,
    model: Option<String>,
    instructions: Option<Bytes>,
    max_output_tokens: MaxTokens,
}

impl AcceptCall {
    fn with_prompt(prompt: Bytes) -> Self {
        Self {
            instructions: Some(prompt),
            ..Default::default()
        }
    }
}

impl Default for AcceptCall {
    fn default() -> Self {
        Self {
            event_type: "realtime".to_string(),
            model: Some("gpt-realtime".to_string()),
            instructions: Default::default(),
            max_output_tokens: MaxTokens::Tokens(4096),
        }
    }
}

impl OpenApiCall for AcceptCall {
    fn get_url(&self, call_id: &str) -> String {
        format!("{}/{}/accept", API_ROOT, call_id)
    }
}

#[derive(Debug)]
enum MaxTokens {
    Tokens(u16),
    _Inf,
}

impl Serialize for MaxTokens {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            MaxTokens::Tokens(int) => serializer.serialize_u16(*int),
            MaxTokens::_Inf => serializer.serialize_str("inf"),
        }
    }
}

#[derive(Serialize, Debug)]
pub struct _RejectCall {
    status_code: u16,
}

impl Default for _RejectCall {
    fn default() -> Self {
        Self {
            status_code: 603, // SIP 603 Decline
        }
    }
}

impl OpenApiCall for _RejectCall {
    fn get_url(&self, call_id: &str) -> String {
        format!("{}/{}/reject", API_ROOT, call_id)
    }
}

#[derive(Serialize, Debug)]
pub struct _Hangup {}

impl OpenApiCall for _Hangup {
    fn get_url(&self, call_id: &str) -> String {
        format!("{}/{}/hangup", API_ROOT, call_id)
    }
}
