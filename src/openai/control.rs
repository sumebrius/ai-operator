/// Main control loop for handling a call
use std::sync::Arc;

use futures_util::lock::Mutex;
use tracing::Instrument;

use super::api::{self, ApiClient};
use super::websocket::{WebsocketClient, client_event, server_event::ServerEvent};
use crate::openai::api::Voice;
use crate::openai::tools::{SideEffect, SideEffectResult, TransferTarget};
use crate::{config::AppState, openai::webhook::RealtimeCallIncoming};

/// Main thread for handling the event loop.
/// Spawned directly from the webhook
#[tracing::instrument(skip_all, fields(call.id = &call.call_id()))]
pub async fn handle_call(state: AppState, call: RealtimeCallIncoming) {
    info!("Handling call");
    let call_id = call.call_id();
    let control_client = OpenAiControlSession::new(&state, call_id);

    //TODO - We should prolly authenticate the call and not just blindly accept
    if control_client.accept(state.voice()).await.is_err() {
        return;
    };
    info!("Call Answered");

    let (mut ws_write, mut ws_read) = match control_client.connect_websocket(call_id).await {
        Ok(clients) => clients,
        Err(_) => return,
    };

    // Tell the model to start a response, otherwise it will wait for the user.
    let _ = ws_write.send(&client_event::Response::Create).await;

    // Start listening to server events
    while let Some(event) = ws_read.get_event().await {
        // We actually only really care about `response.done`
        // This is where the model actually sends function calls
        if let ServerEvent::ResponseDone(response) = event {
            let span = debug_span!("response_handle", response.id = response.event_id());
            let _ = span.enter();

            let function_calls = response.function_calls();
            if function_calls.is_empty() {
                // And we really only give a shit ones with function calls.
                // We can let the model just handle the rest itself.
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

                // Call the tool
                let (mut result, side_effect) = call.run();
                info!("Tool call results: {}", result.output);
                // Drop the guard here so we can re-use the span
                drop(fn_span_guard);

                // Execute the side effect and handle its impact on the main loop
                match control_client.handle_effect(side_effect).await {
                    SideEffectResult::Ok => {}
                    SideEffectResult::Final => return,
                    SideEffectResult::Error(err) => {
                        error!("Error handling side effect: {:?}", err);
                        result.error(&err)
                    }
                };

                // Send the result back to the model
                let message: client_event::Conversation = result.into();
                let _ = ws_write.send(&message).instrument(fn_span).await;
            }
            // Indicate to the model to start a new response based on the outputs
            let _ = ws_write
                .send(&client_event::Response::Create)
                .instrument(span)
                .await;
        };
    }
}

pub struct OpenAiControlSession {
    call_id: String,
    token: Arc<String>,
    prompt: Arc<String>,
    sip_realm: Arc<String>,
    valid_transfers: Mutex<Vec<TransferTarget>>,
}

impl OpenAiControlSession {
    pub fn new(state: &AppState, call_id: &str) -> Self {
        Self {
            call_id: call_id.to_string(),
            token: state.openai_key(),
            prompt: state.prompt(),
            sip_realm: state.sip_realm(),
            valid_transfers: Mutex::new(Vec::new()),
        }
    }

    pub async fn accept(&self, voice: Voice) -> Result<(), api::ApiError> {
        let payload = api::AcceptCall::new(&self.prompt, voice);
        self.execute(payload, &self.call_id)
            .await
            .inspect_err(|err| {
                error!("Error response accepting call: {}", err);
            })
    }

    pub async fn _reject(&self) -> Result<(), api::ApiError> {
        warn!("Rejecting call");
        let payload = api::_RejectCall::default();
        self.execute(payload, &self.call_id)
            .await
            .inspect_err(|err| {
                error!("Error response hanging up call: {}", err);
            })
    }

    pub async fn transfer(&self, target: &str) -> Result<(), api::ApiError> {
        info!("Transferring call to {}", target);
        let target_uri = format!("sip:{}@{}", target, self.sip_realm);
        let payload = api::ReferCall::new(target_uri);
        self.execute(payload, &self.call_id)
            .await
            .inspect_err(|err| {
                error!("Error response transferring call: {}", err);
            })
    }

    pub async fn hangup(&self) -> Result<(), api::ApiError> {
        warn!("Terminating call");
        let payload = api::HangupCall;
        self.execute(payload, &self.call_id)
            .await
            .inspect_err(|err| {
                error!("Error response hanging up call: {}", err);
            })
    }

    pub async fn handle_effect(&self, side_effect: SideEffect) -> SideEffectResult {
        match side_effect {
            SideEffect::Noop => SideEffectResult::Ok,
            SideEffect::Store(target) => {
                self.valid_transfers.lock().await.push(target);
                SideEffectResult::Ok
            }
            SideEffect::Transfer(call_id) => {
                match self
                    .valid_transfers
                    .lock()
                    .await
                    .iter()
                    .find(|target| target.call_id == call_id)
                {
                    Some(target) => match self.transfer(&target.target).await {
                        Ok(_) => SideEffectResult::Final,
                        Err(err) => SideEffectResult::Error(format!("{:?}", err)),
                    },
                    None => SideEffectResult::Error("Invalid transfer call_id".to_string()),
                }
            }
            SideEffect::Terminate => {
                let _ = self.hangup().await;
                SideEffectResult::Final
            }
        }
    }
}

impl ApiClient for OpenAiControlSession {
    fn bearer_token(&self) -> &str {
        &self.token
    }
}

impl WebsocketClient for OpenAiControlSession {
    fn get_token(&self) -> &str {
        &self.token
    }
}
