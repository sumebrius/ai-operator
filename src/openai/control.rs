use std::{fs::File, io::Write, sync::Arc};

use crate::{config::AppState, openai::webhook::RealtimeCallIncoming};
use futures_util::StreamExt;
use reqwest::{Client, Response, header};
use tokio_tungstenite::tungstenite::Message;

use super::api::{self, ApiClient};
use super::websocket::{WebsocketClient, client_event};

#[tracing::instrument(skip_all, fields(call.id = &call.call_id()))]
pub async fn handle_call(state: AppState, call: RealtimeCallIncoming) {
    info!("Handling call");
    let control_client = OpenAiControlSession::new(&state);
    let call_id = call.call_id();

    if let Err(err) = control_client.accept(call_id).await {
        error!("Error response accepting call: {}", err);
        return;
    };
    info!("Call Answered");

    let (mut ws_write, mut ws_read) = match control_client.connect_ws(call_id).await {
        Ok(clients) => {
            info!("Websocket control session initialised");
            clients
        }
        Err(err) => {
            error!("Error starting websocket connection: {}", err);
            return;
        }
    };

    let mut log = File::create(format!("call_logs/{}.jsonl", call_id)).expect("Cant open log file");

    match ws_write.send(&client_event::Response::Create).await {
        Ok(_) => info!("User greeting initialised"),
        Err(err) => error!("Error greeting user event: {:?}", err),
    };

    while let Some(msg) = ws_read.next().await {
        match msg {
            Ok(msg) => match msg {
                Message::Text(v) => {
                    debug!("Message: {}", v.as_str());
                    let _ = log.write_all(v.as_bytes());
                    let _ = log.write_all(b"\n");
                }
                other => warn!("Non text WS message: {}", other),
            },
            Err(err) => error!("Fucky WS message: {:?}", err),
        }
    }
}

pub struct OpenAiControlSession {
    api_client: Client,
    token: Arc<String>,
    prompt: Arc<String>,
}

impl OpenAiControlSession {
    pub fn new(state: &AppState) -> Self {
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
            api_client: client,
            token: state.openai_key(),
            prompt: state.prompt(),
        }
    }

    pub async fn accept(&self, call_id: &str) -> Result<Response, reqwest::Error> {
        self.execute(api::AcceptCall::with_prompt(&self.prompt), call_id)
            .await
    }
}

impl ApiClient for OpenAiControlSession {
    fn client(&self) -> &Client {
        &self.api_client
    }
}

impl WebsocketClient for OpenAiControlSession {
    fn get_token(&self) -> &str {
        &self.token
    }
}
