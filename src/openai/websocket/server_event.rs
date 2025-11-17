use serde::{Deserialize, Serialize};
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum ServerEvent {
    #[serde(rename = "error")]
    Error(Error),
    #[serde(rename = "session.created")]
    SessionCreated(SessionCreated),
    #[serde(rename = "session.updated")]
    SessionUpdated(SessionUpdated),
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
    ResponseDone,
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
    pub fn ignore(&self) -> bool {
        matches!(
            self,
            Self::ResponseOutputTextDelta
                | Self::ResponseOutputAudioDelta
                | Self::ResponseOutputAudioTranscriptDelta
                | Self::ConversationItemInputAudioTranscriptionDelta
        )
    }
}

#[derive(Debug)]
#[allow(dead_code)]
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
pub struct SessionCreated {
    event_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SessionUpdated {
    event_id: String,
}

// #[derive(Debug, Deserialize, Serialize)]
// pub struct Spare {
//     event_id: String,
// }
