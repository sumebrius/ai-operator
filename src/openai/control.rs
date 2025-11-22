use std::sync::Arc;

use reqwest::{Client, Response, header};
use tracing::Instrument;

use super::api::{self, ApiClient};
use super::websocket::{WebsocketClient, client_event, server_event::ServerEvent};
use crate::openai::tools::SideEffect;
use crate::{config::AppState, openai::webhook::RealtimeCallIncoming};

#[tracing::instrument(skip_all, fields(call.id = &call.call_id()))]
pub async fn handle_call(state: AppState, call: RealtimeCallIncoming) {
    info!("Handling call");
    let call_id = call.call_id();
    let control_client = OpenAiControlSession::new(&state, call_id);
    if control_client.accept().await.is_err() {
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
            let span = debug_span!("response_handle", response.id = response.event_id());
            let _ = span.enter();

            let function_calls = response.function_calls();
            if function_calls.is_empty() {
                continue;
            }

            for call in function_calls {
                let fn_span = debug_span!(
                    parent: &span,
                    "function_call",
                    function.call.name = call.name(),
                    function.call.id = call.call_id()
                );

                let fn_span_guard = fn_span.enter();
                let (result, side_effect) = call.run();
                info!("Tool call results: {}", result.output);
                drop(fn_span_guard);

                let message: client_event::Conversation = result.into();
                let _ = ws_write.send(&message).instrument(fn_span).await;

                let terminal = control_client.handle_effect(side_effect).await;
                if terminal {
                    return;
                }
            }
            let _ = ws_write
                .send(&client_event::Response::Create)
                .instrument(span)
                .await;
        };
    }
}

pub struct OpenAiControlSession {
    call_id: String,
    api_client: Client,
    token: Arc<String>,
    prompt: Arc<String>,
    transcribe_caller: bool,
}

impl OpenAiControlSession {
    pub fn new(state: &AppState, call_id: &str) -> Self {
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
            call_id: call_id.to_string(),
            api_client: client,
            token: state.openai_key(),
            prompt: state.prompt(),
            transcribe_caller: state.transcribe_caller(),
        }
    }

    pub async fn accept(&self) -> Result<Response, reqwest::Error> {
        let payload = if self.transcribe_caller {
            api::AcceptCall::new(&self.prompt).transcribe_caller()
        } else {
            api::AcceptCall::new(&self.prompt)
        };
        self.execute(payload, &self.call_id)
            .await
            .inspect_err(|err| {
                error!("Error response accepting call: {}", err);
            })
    }

    pub async fn transfer(&self, target: &str) -> Result<Response, reqwest::Error> {
        info!("Transferring call to {}", target);
        let payload = api::ReferCall::new(target);
        self.execute(payload, &self.call_id)
            .await
            .inspect_err(|err| {
                error!("Error response transferring call: {}", err);
            })
    }

    pub async fn hangup(&self) -> Result<Response, reqwest::Error> {
        info!("Terminating call");
        let payload = api::Hangup;
        self.execute(payload, &self.call_id)
            .await
            .inspect_err(|err| {
                error!("Error response hanging up call: {}", err);
            })
    }

    pub async fn handle_effect(&self, side_effect: SideEffect) -> bool {
        match side_effect {
            SideEffect::Noop => false,
            SideEffect::Transfer(target) => self.transfer(&target).await.is_ok(),
            SideEffect::Terminate => {
                let _ = self.hangup().await;
                true
            }
        }
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
