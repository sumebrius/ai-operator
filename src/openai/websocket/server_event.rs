/// Mostly structs defining events we receive from the WS connection
/// as defined here https://platform.openai.com/docs/api-reference/realtime-server-events
#[cfg(debug_assertions)]
use std::{fs::File, io::Write, time::SystemTime};

#[cfg(debug_assertions)]
use chrono::{DateTime, Utc};
use futures_util::{StreamExt, stream::SplitStream};
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{self, Message},
};

use crate::openai::{
    tools::{SideEffect, Tool},
    websocket::client_event,
};

/// Alias for the actual Source type we get from tokio_tungstenite
type WsSource = SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>;

/// A thin wrapper around a tungstenite WS source, with a filter for
/// events we actually give a shit about.
/// Also handles dumping the raw payloads out to a log file for debugging
/// in dev mode.
pub struct MessageSource {
    #[cfg(debug_assertions)]
    debug_file: File,
    stream: WsSource,
}

impl MessageSource {
    #[cfg(debug_assertions)]
    pub fn new(stream: WsSource, call_id: &str) -> Self {
        let now: DateTime<Utc> = SystemTime::now().into();
        let file = File::create(format!(
            "call_logs/{}_{}.jsonl",
            now.naive_local().format("%Y-%m-%dT%H:%M:%S"),
            call_id
        ))
        .expect("Cant open log file");

        Self {
            debug_file: file,
            stream,
        }
    }
    #[cfg(not(debug_assertions))]
    pub fn new(stream: WsSource, _call_id: &str) -> Self {
        Self { stream }
    }

    /// A wrapper around the underlying stream's `.next()` call,
    /// with a filter for relevant messages only.
    pub async fn get_event(&mut self) -> Option<ServerEvent> {
        loop {
            match self.stream.next().await {
                Some(msg) => match self.decode_event(msg) {
                    Some(event) => return Some(event),
                    None => continue,
                },
                None => return None,
            }
        }
    }

    fn decode_event(
        &mut self,
        msg: Result<Message, tungstenite::error::Error>,
    ) -> Option<ServerEvent> {
        let msg = match msg {
            Ok(msg) => msg,
            Err(err) => {
                error!("Can't retrieve WS message: {:?}", err);
                return None;
            }
        };
        let event = match ServerEvent::try_from(&msg) {
            Ok(event) => event,
            Err(err) => {
                error!("Fucky WS message: {:#?}", err);
                self.log(msg);
                return None;
            }
        };
        if event.ignore_for_dbg_log() {
            return None;
        }
        event.log();

        self.log(msg);
        Some(event)
    }

    /// Log the message to our debug file (if we're doing that)
    #[cfg(debug_assertions)]
    fn log(&mut self, msg: Message) {
        if let Message::Text(msg_bytes) = msg {
            self.debug_file
                .write_all(msg_bytes.as_bytes())
                .unwrap_or_else(|err| error!("Unable to log message to file: {:?}", err));
            self.debug_file
                .write_all(b"\n")
                .unwrap_or_else(|err| error!("Unable to log message to file: {:?}", err));
        }
    }
    #[cfg(not(debug_assertions))]
    fn log(&mut self, _msg: Message) {}
}

/// Main enum defining all the possible events we could get from the server.
/// We ignore the content of most of these, so don't define any structs to
/// deserialise them.
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum ServerEvent {
    #[serde(rename = "error")]
    Error(Error),
    #[serde(rename = "session.created")]
    SessionCreated,
    #[serde(rename = "session.updated")]
    SessionUpdated,
    #[serde(rename = "conversation.item.added")]
    ConversationItemAdded,
    #[serde(rename = "conversation.item.done")]
    ConversationItemDone,
    #[serde(rename = "conversation.item.retrieved")]
    ConversationItemRetrieved,
    #[serde(rename = "conversation.item.input_audio_transcription.completed")]
    ConversationItemInputAudioTranscriptionCompleted,
    #[serde(rename = "conversation.item.input_audio_transcription.delta")]
    ConversationItemInputAudioTranscriptionDelta,
    #[serde(rename = "conversation.item.input_audio_transcription.segment")]
    ConversationItemInputAudioTranscriptionSegment,
    #[serde(rename = "conversation.item.input_audio_transcription.failed")]
    ConversationItemInputAudioTranscriptionFailed,
    #[serde(rename = "conversation.item.truncated")]
    ConversationItemTruncated,
    #[serde(rename = "conversation.item.deleted")]
    ConversationItemDeleted,
    #[serde(rename = "input_audio_buffer.committed")]
    InputAudioBufferCommitted,
    #[serde(rename = "input_audio_buffer.cleared")]
    InputAudioBufferCleared,
    #[serde(rename = "input_audio_buffer.speech_started")]
    InputAudioBufferSpeechStarted,
    #[serde(rename = "input_audio_buffer.speech_stopped")]
    InputAudioBufferSpeechStopped,
    #[serde(rename = "input_audio_buffer.timeout_triggered")]
    InputAudioBufferTimeoutTriggered,
    #[serde(rename = "output_audio_buffer.started")]
    OutputAudioBufferStarted,
    #[serde(rename = "output_audio_buffer.stopped")]
    OutputAudioBufferStopped,
    #[serde(rename = "output_audio_buffer.cleared")]
    OutputAudioBufferCleared,
    #[serde(rename = "response.created")]
    ResponseCreated,
    #[serde(rename = "response.done")]
    ResponseDone(ResponseDone),
    #[serde(rename = "response.output_item.added")]
    ResponseOutputItemAdded,
    #[serde(rename = "response.output_item.done")]
    ResponseOutputItemDone,
    #[serde(rename = "response.content_part.added")]
    ResponseContentPartAdded,
    #[serde(rename = "response.content_part.done")]
    ResponseContentPartDone,
    #[serde(rename = "response.output_text.delta")]
    ResponseOutputTextDelta,
    #[serde(rename = "response.output_text.done")]
    ResponseOutputTextDone,
    #[serde(rename = "response.output_audio_transcript.delta")]
    ResponseOutputAudioTranscriptDelta,
    #[serde(rename = "response.output_audio_transcript.done")]
    ResponseOutputAudioTranscriptDone,
    #[serde(rename = "response.output_audio.delta")]
    ResponseOutputAudioDelta,
    #[serde(rename = "response.output_audio.done")]
    ResponseOutputAudioDone,
    #[serde(rename = "response.function_call_arguments.delta")]
    ResponseFunctionCallArgumentsDelta,
    #[serde(rename = "response.function_call_arguments.done")]
    ResponseFunctionCallArgumentsDone,
    #[serde(rename = "response.mcp_call_arguments.delta")]
    ResponseMcpCallArgumentsDelta,
    #[serde(rename = "response.mcp_call_arguments.done")]
    ResponseMcpCallArgumentsDone,
    #[serde(rename = "response.mcp_call.in_progress")]
    ResponseMcpCallInProgress,
    #[serde(rename = "response.mcp_call.completed")]
    ResponseMcpCallCompleted,
    #[serde(rename = "response.mcp_call.failed")]
    ResponseMcpCallFailed,
    #[serde(rename = "mcp_list_tools.in_progress")]
    McpListToolsInProgress,
    #[serde(rename = "mcp_list_tools.completed")]
    McpListToolsCompleted,
    #[serde(rename = "mcp_list_tools.failed")]
    McpListToolsFailed,
    #[serde(rename = "rate_limits.updated")]
    RateLimitsUpdated,
}

impl ServerEvent {
    pub fn ignore_for_dbg_log(&self) -> bool {
        matches!(
            self,
            Self::ResponseOutputTextDelta
                | Self::ResponseOutputAudioDelta
                | Self::ResponseOutputAudioTranscriptDelta
                | Self::ConversationItemInputAudioTranscriptionDelta
                | Self::ResponseFunctionCallArgumentsDelta
                | Self::ResponseMcpCallArgumentsDelta
        )
    }

    /// We log every event we receive, just at different levels.
    pub fn log(&self) {
        match self {
            // Error
            ServerEvent::Error(_) => error!("Server Event: {:?}", self),
            // Warn
            ServerEvent::ConversationItemInputAudioTranscriptionFailed => {
                warn!("Server Event: {:?}", self)
            }
            ServerEvent::McpListToolsFailed => warn!("Server Event: {:?}", self),
            ServerEvent::ResponseMcpCallFailed => warn!("Server Event: {:?}", self),
            // Info
            ServerEvent::SessionCreated => info!("Server Event: {:?}", self),
            ServerEvent::SessionUpdated => info!("Server Event: {:?}", self),
            ServerEvent::ConversationItemInputAudioTranscriptionCompleted => {
                info!("Server Event: {:?}", self)
            }
            ServerEvent::ConversationItemTruncated => info!("Server Event: {:?}", self),
            ServerEvent::ConversationItemDeleted => info!("Server Event: {:?}", self),
            ServerEvent::ResponseCreated => info!("Server Event: {:?}", self),
            ServerEvent::ResponseDone(_) => info!("Server Event: {:?}", self),
            ServerEvent::ResponseOutputItemAdded => info!("Server Event: {:?}", self),
            ServerEvent::ResponseOutputAudioTranscriptDone => info!("Server Event: {:?}", self),
            ServerEvent::ResponseFunctionCallArgumentsDone => info!("Server Event: {:?}", self),
            ServerEvent::ResponseMcpCallArgumentsDone => info!("Server Event: {:?}", self),
            ServerEvent::ResponseMcpCallInProgress => info!("Server Event: {:?}", self),
            ServerEvent::ResponseMcpCallCompleted => info!("Server Event: {:?}", self),
            ServerEvent::RateLimitsUpdated => info!("Server Event: {:?}", self),
            // Debug
            ServerEvent::ConversationItemAdded => debug!("Server Event: {:?}", self),
            ServerEvent::ConversationItemDone => debug!("Server Event: {:?}", self),
            ServerEvent::ConversationItemRetrieved => debug!("Server Event: {:?}", self),
            ServerEvent::InputAudioBufferCommitted => debug!("Server Event: {:?}", self),
            ServerEvent::InputAudioBufferCleared => debug!("Server Event: {:?}", self),
            ServerEvent::InputAudioBufferSpeechStarted => debug!("Server Event: {:?}", self),
            ServerEvent::InputAudioBufferSpeechStopped => debug!("Server Event: {:?}", self),
            ServerEvent::InputAudioBufferTimeoutTriggered => debug!("Server Event: {:?}", self),
            ServerEvent::OutputAudioBufferStarted => debug!("Server Event: {:?}", self),
            ServerEvent::OutputAudioBufferStopped => debug!("Server Event: {:?}", self),
            ServerEvent::OutputAudioBufferCleared => debug!("Server Event: {:?}", self),
            ServerEvent::ResponseOutputItemDone => debug!("Server Event: {:?}", self),
            ServerEvent::ResponseContentPartAdded => debug!("Server Event: {:?}", self),
            ServerEvent::ResponseContentPartDone => debug!("Server Event: {:?}", self),
            ServerEvent::ResponseOutputTextDone => debug!("Server Event: {:?}", self),
            ServerEvent::ResponseOutputAudioDone => debug!("Server Event: {:?}", self),
            ServerEvent::McpListToolsInProgress => debug!("Server Event: {:?}", self),
            ServerEvent::McpListToolsCompleted => debug!("Server Event: {:?}", self),
            // Trace
            ServerEvent::ConversationItemInputAudioTranscriptionDelta => {
                trace!("Server Event: {:?}", self)
            }
            ServerEvent::ResponseOutputAudioTranscriptDelta => trace!("Server Event: {:?}", self),
            ServerEvent::ResponseOutputAudioDelta => trace!("Server Event: {:?}", self),
            ServerEvent::ResponseOutputTextDelta => trace!("Server Event: {:?}", self),
            ServerEvent::ResponseFunctionCallArgumentsDelta => trace!("Server Event: {:?}", self),
            ServerEvent::ResponseMcpCallArgumentsDelta => trace!("Server Event: {:?}", self),
            ServerEvent::ConversationItemInputAudioTranscriptionSegment => {
                trace!("Server Event: {:?}", self)
            }
        }
    }
}

/// Error indicating the server sent us a bad message.
/// Gets serialised to send it back up at'em
#[derive(Debug)]
#[allow(dead_code)] // Supress errors about not using the wrapped values, but we use them for "{:?}" in serialisation
pub enum MessageDecodeError {
    MessageType(Message),
    Deserialization(serde_json::Error),
}

impl TryFrom<&Message> for ServerEvent {
    type Error = MessageDecodeError;

    fn try_from(value: &Message) -> Result<Self, MessageDecodeError> {
        match value {
            Message::Text(utf8_bytes) => serde_json::from_slice(utf8_bytes.as_bytes())
                .map_err(MessageDecodeError::Deserialization),
            other => Err(MessageDecodeError::MessageType(other.clone())),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Error {
    event_id: String,
    error: ErrorDetail,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ErrorDetail {
    message: String,
    #[serde(rename = "type")]
    err_type: String,
    code: Option<String>,
    event_id: Option<String>,
    param: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ResponseDone {
    event_id: String,
    response: Response,
}

impl ResponseDone {
    /// Get a list of any function calls made in this request
    pub fn function_calls(&self) -> Vec<&FunctionCall> {
        self.response
            .output
            .iter()
            .filter_map(|item| match item {
                ResponseOutput::FunctionCall(item) => Some(item),
                _ => None,
            })
            .collect()
    }

    pub fn event_id(&self) -> &str {
        &self.event_id
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Response {
    output: Vec<ResponseOutput>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum ResponseOutput {
    #[serde(rename = "message")]
    Message(ResponseMessage),
    #[serde(rename = "function_call")]
    FunctionCall(FunctionCall),
    #[serde(rename = "function_call_output")]
    FunctionCallOutput,
    #[serde(rename = "mcp_approval_response")]
    McpApproval,
    #[serde(rename = "mcp_list_tools")]
    McpList,
    #[serde(rename = "mcp_call")]
    McpCall,
    #[serde(rename = "mcp_approval_request")]
    McpRequest,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ResponseMessage {
    role: Role,
    status: String,
    content: Vec<ResponseContent>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ResponseContent {
    #[serde(rename = "type")]
    content_type: ContentType,
    text: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    Message,
    InputText,
    InputAudio,
    InputImage,
    OutputText,
    OutputAudio,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct FunctionCall {
    id: String,
    call_id: String,
    #[serde(flatten)]
    // There's a whole bunch of serde fuckery to get this serialising *just right*
    tool: Tool,
    arguments: String,
}

impl FunctionCall {
    /// Actually run the call through the tool
    /// Returns the client event to send back to the API,
    /// and a possible side-effect for us to handle
    pub fn run(&self) -> (client_event::FunctionCallOutput, SideEffect) {
        let result = self.tool.run(&self.arguments, &self.call_id);
        let call_id = self.call_id.clone();
        let output = result.output;
        (
            client_event::FunctionCallOutput { output, call_id },
            result.side_effect,
        )
    }

    /// Get the name of the tool - just used for building a tracing span
    pub fn name(&self) -> String {
        format!("{:?}", self.tool)
    }

    pub fn call_id(&self) -> &str {
        &self.call_id
    }
}

// #[derive(Debug, Deserialize, Serialize)]
// pub struct Spare {
//     event_id: String,
// }
