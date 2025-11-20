use std::time::SystemTime;
use std::{fs::File, io::Write, sync::Arc};

use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use reqwest::{Client, Response, header};
use tokio_tungstenite::tungstenite::Message;

use super::api::{self, ApiClient};
use super::websocket::{WebsocketClient, client_event, server_event::ServerEvent};
use crate::{config::AppState, openai::webhook::RealtimeCallIncoming};

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

    let now: DateTime<Utc> = SystemTime::now().into();
    let mut log = File::create(format!(
        "call_logs/{}_{}.jsonl",
        now.naive_local().format("%Y-%m-%dT%H:%M:%S"),
        call_id
    ))
    .expect("Cant open log file");

    match ws_write.send(&client_event::Response::Create).await {
        Ok(_) => info!("User greeting initialised"),
        Err(err) => error!("Error greeting user event: {:?}", err),
    };

    while let Some(msg) = ws_read.next().await {
        let msg = match msg {
            Ok(msg) => msg,
            Err(err) => {
                error!("Can't retrieve WS message: {:?}", err);
                continue;
            }
        };
        let event = match ServerEvent::try_from(&msg) {
            Ok(event) => event,
            Err(err) => {
                error!("Fucky WS message: {:#?}", err);
                if let Message::Text(msg_bytes) = msg {
                    let _ = log.write_all(msg_bytes.as_bytes());
                    let _ = log.write_all(b"\n");
                }
                continue;
            }
        };
        if event.ignore() {
            continue;
        }

        debug!("Server Event: {:?}", event);
        if let Message::Text(msg_bytes) = msg {
            let _ = log.write_all(msg_bytes.as_bytes());
            let _ = log.write_all(b"\n");
        }

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
                if let Err(err) = ws_write.send(&message).await {
                    error!("Error sending tool response: {:?}", err)
                }
            }
            match ws_write.send(&client_event::Response::Create).await {
                Ok(_) => info!("Response started"),
                Err(err) => error!("Error starting response event: {:?}", err),
            };
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
        self.execute(payload, call_id).await
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
