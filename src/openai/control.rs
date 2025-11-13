use bytes::Bytes;
use reqwest::{Client, Response, header};
use serde::{self, Serialize};
use tokio::net::TcpStream;
use tokio_stream::StreamExt;
use tokio_tungstenite::{self, MaybeTlsStream, WebSocketStream, tungstenite};

use crate::{
    config::{API_ROOT, AppState, WS_URI},
    openai::webhook::RealtimeCallIncoming,
};

pub async fn handle_call(state: AppState, call: RealtimeCallIncoming) {
    let control_client = OpenAiControlSession::new(&state, &call);

    if let Err(err) = control_client.accept().await {
        error!(
            { call = &call.get_id() },
            "Error response accepting call: {}", err
        );
        return;
    };

    let mut ws_client = match control_client.connect_ws().await {
        Ok((client, _)) => client,
        Err(err) => {
            error!(
                { call = &call.get_id() },
                "Error response accepting call: {}", err
            );
            return;
        }
    };

    while let Some(msg) = ws_client.next().await {
        debug!("Message Received: {:?}", msg);
    }
}

pub struct OpenAiControlSession {
    client: Client,
    token: String,
    call_id: String,
    prompt: Bytes,
}

impl OpenAiControlSession {
    pub fn new(state: &AppState, call: &RealtimeCallIncoming) -> Self {
        let token = state.openai_key().to_string();

        let mut headers = header::HeaderMap::new();
        let mut auth =
            header::HeaderValue::try_from(format!("Bearer {}", token)).expect("Fucky API key");
        auth.set_sensitive(true);
        headers.insert(header::AUTHORIZATION, auth);
        let client = reqwest::ClientBuilder::new()
            .default_headers(headers)
            .build()
            .expect("Fucky Client");

        Self {
            client,
            token,
            call_id: call.get_id().to_string(),
            prompt: state.prompt(),
        }
    }

    pub async fn accept(&self) -> Result<Response, reqwest::Error> {
        self.execute(AcceptCall::with_prompt(self.prompt.clone()))
            .await
    }

    async fn execute(&self, action: impl OpenApiCall) -> Result<Response, reqwest::Error> {
        self.client
            .post(action.get_url(&self.call_id))
            .json(&action)
            .send()
            .await
    }

    pub async fn connect_ws(
        &self,
    ) -> Result<
        (
            WebSocketStream<MaybeTlsStream<TcpStream>>,
            http::Response<Option<Vec<u8>>>,
        ),
        tungstenite::Error,
    > {
        let uri = format!("{}?call_id={}", WS_URI, self.call_id);
        let request = http::Request::get(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", self.token))
            .body(())
            .expect("Fucky WS URI or Header");
        tokio_tungstenite::connect_async(request).await
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
