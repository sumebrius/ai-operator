use std::sync::Arc;

use crate::{
    config::{API_ROOT, AppState, WS_URI},
    openai::webhook::RealtimeCallIncoming,
};
use reqwest::{Client, Response, header};
use serde::{self, Serialize};
use tokio::net::TcpStream;
use tokio_stream::StreamExt;
use tokio_tungstenite::{self, MaybeTlsStream, WebSocketStream, tungstenite};

#[tracing::instrument(skip_all, fields(call.id = &call.call_id()))]
pub async fn handle_call(state: AppState, call: RealtimeCallIncoming) {
    info!("Handling call");
    let control_client = OpenAiControlSession::new(&state, &call);

    if let Err(err) = control_client.accept().await {
        error!("Error response accepting call: {}", err);
        return;
    };
    info!("Call Answered");

    let mut ws_client = match control_client.connect_ws().await {
        Ok((client, _)) => client,
        Err(err) => {
            error!("Error starting websocket connection: {}", err);
            return;
        }
    };

    while let Some(msg) = ws_client.next().await {
        debug!("Message Received: {:?}", msg);
    }
}

pub struct OpenAiControlSession {
    client: Client,
    token: Arc<String>,
    call_id: String,
    prompt: Arc<String>,
}

impl OpenAiControlSession {
    pub fn new(state: &AppState, call: &RealtimeCallIncoming) -> Self {
        let token = state.openai_key();

        let mut headers = header::HeaderMap::new();
        let mut auth =
            header::HeaderValue::try_from(format!("Bearer {}", &token)).expect("Fucky API key");
        auth.set_sensitive(true);
        headers.insert(header::AUTHORIZATION, auth);
        let client = reqwest::ClientBuilder::new()
            .default_headers(headers)
            .build()
            .expect("Fucky Client");

        Self {
            client,
            token: state.openai_key(),
            call_id: call.call_id().to_string(),
            prompt: state.prompt(),
        }
    }

    pub async fn accept(&self) -> Result<Response, reqwest::Error> {
        self.execute(AcceptCall::with_prompt(&self.prompt)).await
    }

    async fn execute(&self, action: impl OpenApiCall) -> Result<Response, reqwest::Error> {
        let response = self
            .client
            .post(action.get_url(&self.call_id))
            .json(&action)
            .send()
            .await?;

        if !response.status().is_success() {
            let error = response.error_for_status_ref().unwrap_err();
            error!(
                "{} response from OpenAI API: {:?}",
                response.status(),
                response.text().await
            );
            Err(error)
        } else {
            response.error_for_status()
        }
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
        let uri = http::Uri::try_from(format!("{}?call_id={}", WS_URI, self.call_id))
            .expect("Fucky WS URI or Header");
        let request = tungstenite::ClientRequestBuilder::new(uri)
            .with_header("Authorization", format!("Bearer {}", self.token));
        tokio_tungstenite::connect_async(request).await
    }
}

pub trait OpenApiCall: Serialize {
    fn get_url(&self, call_id: &str) -> String;
}

#[derive(Serialize, Debug)]
pub struct AcceptCall<'a> {
    #[serde(rename = "type")]
    event_type: String,
    model: Option<String>,
    audio: AudioConfig,
    instructions: &'a str,
    max_output_tokens: MaxTokens,
}

impl<'a> AcceptCall<'a> {
    fn with_prompt(prompt: &'a String) -> Self {
        Self {
            instructions: prompt,
            ..Default::default()
        }
    }
}

impl<'a> Default for AcceptCall<'a> {
    fn default() -> Self {
        Self {
            event_type: "realtime".to_string(),
            model: Some("gpt-realtime".to_string()),
            audio: Default::default(),
            instructions: Default::default(),
            max_output_tokens: MaxTokens::Tokens(4096),
        }
    }
}

impl<'a> OpenApiCall for AcceptCall<'a> {
    fn get_url(&self, call_id: &str) -> String {
        format!("{}/{}/accept", API_ROOT, call_id)
    }
}

#[derive(Debug, Default, Serialize)]
struct AudioConfig {
    input: AudioInput,
    output: AudioOutput,
}

#[derive(Debug, Default, Serialize)]
struct AudioInput {
    format: AudioFormat,
}

#[derive(Debug, Default, Serialize)]
struct AudioOutput {
    format: AudioFormat,
}

#[derive(Debug, Default, Serialize)]
struct AudioFormat {
    #[serde(rename = "type")]
    codec: Codec,
}

#[derive(Debug)]
enum Codec {
    _Pcm,
    Alaw,
    _Ulaw,
}

impl Default for Codec {
    fn default() -> Self {
        Self::Alaw
    }
}

impl Serialize for Codec {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Codec::_Pcm => serializer.serialize_str("audio/pcm"),
            Codec::Alaw => serializer.serialize_str("audio/pcma"),
            Codec::_Ulaw => serializer.serialize_str("audio/pcmu"),
        }
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
