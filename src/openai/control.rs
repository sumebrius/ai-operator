use std::sync::Arc;

use reqwest::{Client, Response, header};

use super::api::{self, ApiClient};
use super::websocket::{WebsocketClient, client_event, server_event::ServerEvent};
use crate::{config::AppState, openai::webhook::RealtimeCallIncoming};

#[tracing::instrument(skip_all, fields(call.id = &call.call_id()))]
pub async fn handle_call(state: AppState, call: RealtimeCallIncoming) {
    info!("Handling call");
    let control_client = OpenAiControlSession::new(&state);
    let call_id = call.call_id();

    if control_client.accept(call_id).await.is_err() {
        return;
    };
    info!("Call Answered");

    let (mut ws_write, mut ws_read) = match control_client.connect_ws(call_id).await {
        Ok(clients) => clients,
        Err(_) => return,
    };

    let _ = ws_write.send(&client_event::Response::Create).await;

    while let Some(event) = ws_read.get_event().await {
        if let ServerEvent::ResponseDone(response) = event {
            let function_calls = response.function_calls();
            if function_calls.is_empty() {
                continue;
            }

            for call in function_calls {
                let call_name = call.name();
                let result = call.run();
                info!(
                    "Tool call {} {} results: {}",
                    call_name, result.call_id, result.output
                );

                let message: client_event::Conversation = result.into();
                let _ = ws_write.send(&message).await;
            }
            let _ = ws_write.send(&client_event::Response::Create).await;
        };
    }
}

pub struct OpenAiControlSession {
    api_client: Client,
    token: Arc<String>,
    prompt: Arc<String>,
    transcribe_caller: bool,
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
            transcribe_caller: state.transcribe_caller(),
        }
    }

    pub async fn accept(&self, call_id: &str) -> Result<Response, reqwest::Error> {
        let payload = if self.transcribe_caller {
            api::AcceptCall::new(&self.prompt).transcribe_caller()
        } else {
            api::AcceptCall::new(&self.prompt)
        };
        self.execute(payload, call_id).await.inspect_err(|err| {
            error!("Error response accepting call: {}", err);
        })
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
