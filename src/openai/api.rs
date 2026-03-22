/// Client and serialisable structs for interacting with the OpenAI Rest API.
///
/// This is used for controlling the SIP call itself.
/// Definitions: https://platform.openai.com/docs/api-reference/realtime-calls
use crate::{
    config::{API_ROOT, DEFAULT_MODEL},
    openai::tools::Tool,
};
use reqwest::{Client, Response};
use serde::{Serialize, Serializer};

/// Again, could this just be a method on the single implementor?
/// Again, yes.
pub trait ApiClient {
    fn client(&self) -> &Client;

    /// Send an event
    async fn execute(
        &self,
        action: impl OpenApiCall,
        call_id: &str,
    ) -> Result<Response, reqwest::Error> {
        let response = self
            .client()
            .post(action.get_url(call_id))
            .json(&action)
            .send()
            .await?;

        if !response.status().is_success() {
            let error = response.error_for_status_ref().unwrap_err();
            error!(
                "{} response from OpenAI API:\n{}",
                response.status(),
                response
                    .text()
                    .await
                    .unwrap_or_else(|err| format!("{:?}", err))
            );
            Err(error)
        } else {
            response.error_for_status()
        }
    }
}

pub trait OpenApiCall: Serialize {
    fn get_url(&self, call_id: &str) -> String;
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "lowercase", tag = "type")]
pub enum AcceptCall<'a> {
    Realtime(Realtime<'a>),
}

impl<'a> AcceptCall<'a> {
    pub fn new(prompt: &'a String, voice: Voice) -> Self {
        let audio = AudioConfig::with_voice(voice);
        Self::Realtime(Realtime {
            instructions: prompt,
            audio,
            ..Default::default()
        })
    }
}

#[derive(Serialize, Debug)]
pub struct Realtime<'a> {
    model: Option<String>,
    audio: AudioConfig,
    instructions: &'a str,
    tools: Vec<Tool>,
    max_output_tokens: MaxTokens,
}

impl<'a> Default for Realtime<'a> {
    fn default() -> Self {
        Self {
            model: Some(DEFAULT_MODEL.to_string()),
            audio: Default::default(),
            instructions: Default::default(),
            tools: Tool::all(),
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
pub struct AudioConfig {
    input: AudioInput,
    output: AudioOutput,
}

impl AudioConfig {
    fn with_voice(voice: Voice) -> Self {
        let output = AudioOutput::with_voice(voice);
        Self {
            output,
            ..Default::default()
        }
    }
}

#[derive(Debug, Serialize)]
struct AudioInput {
    format: AudioFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    transcription: Option<Transcription>,
}

impl Default for AudioInput {
    #[cfg(debug_assertions)]
    fn default() -> Self {
        Self {
            format: Default::default(),
            transcription: None,
        }
    }
    #[cfg(not(debug_assertions))]
    fn default() -> Self {
        Self {
            format: Default::default(),
            transcription: Some(Default::default()),
        }
    }
}

#[derive(Debug, Serialize)]
struct Transcription {
    language: String,
    model: TranscriptionModel,
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt: Option<String>,
}

impl Default for Transcription {
    fn default() -> Self {
        Self {
            language: "en".to_string(),
            model: Default::default(),
            prompt: Default::default(),
        }
    }
}

#[allow(dead_code, non_camel_case_types)]
#[derive(Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
enum TranscriptionModel {
    whisper_1,
    gpt_4o_mini_transcribe,
    gpt_4o_transcribe,
    gpt_4o_transcribe_diarize,
}

impl Default for TranscriptionModel {
    fn default() -> Self {
        Self::gpt_4o_mini_transcribe
    }
}

#[derive(Debug, Default, Serialize)]
struct AudioOutput {
    format: AudioFormat,
    voice: Voice,
}

impl AudioOutput {
    fn with_voice(voice: Voice) -> Self {
        Self {
            voice,
            ..Default::default()
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Voice {
    Alloy,
    Ash,
    Ballad,
    Coral,
    Echo,
    Sage,
    Shimmer,
    Verse,
    Marin,
    Cedar,
}

impl Default for Voice {
    fn default() -> Self {
        Self::Cedar
    }
}

impl TryFrom<&str> for Voice {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.to_lowercase().as_str() {
            "alloy" => Ok(Self::Alloy),
            "ash" => Ok(Self::Ash),
            "ballad" => Ok(Self::Ballad),
            "coral" => Ok(Self::Coral),
            "echo" => Ok(Self::Echo),
            "sage" => Ok(Self::Sage),
            "shimmer" => Ok(Self::Shimmer),
            "verse" => Ok(Self::Verse),
            "marin" => Ok(Self::Marin),
            "cedar" => Ok(Self::Cedar),
            _ => Err(()),
        }
    }
}

impl TryFrom<String> for Voice {
    type Error = ();

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
#[allow(dead_code)]
enum AudioFormat {
    #[serde(rename = "audio/pcm")]
    Pcm { rate: usize },
    #[serde(rename = "audio/pcma")]
    Alaw,
    #[serde(rename = "audio/pcmu")]
    Ulaw,
}

impl Default for AudioFormat {
    fn default() -> Self {
        Self::Alaw
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub enum MaxTokens {
    Tokens(u16),
    Inf,
}

impl Serialize for MaxTokens {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            MaxTokens::Tokens(int) => serializer.serialize_u16(*int),
            MaxTokens::Inf => serializer.serialize_str("inf"),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ReferCall {
    target_uri: String,
}

impl ReferCall {
    pub fn new(target_uri: String) -> Self {
        Self { target_uri }
    }
}

impl OpenApiCall for ReferCall {
    fn get_url(&self, call_id: &str) -> String {
        format!("{}/{}/refer", API_ROOT, call_id)
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
pub struct HangupCall;

impl OpenApiCall for HangupCall {
    fn get_url(&self, call_id: &str) -> String {
        format!("{}/{}/hangup", API_ROOT, call_id)
    }
}
