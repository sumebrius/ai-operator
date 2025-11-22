use crate::{config::API_ROOT, openai::tools::Tool};
use reqwest::{Client, Response};
use serde::{Serialize, Serializer};

pub trait ApiClient {
    fn client(&self) -> &Client;

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
    pub fn new(prompt: &'a String) -> Self {
        Self::Realtime(Realtime {
            instructions: prompt,
            ..Default::default()
        })
    }

    pub fn transcribe_caller(mut self) -> Self {
        let Self::Realtime(ref mut rt) = self;
        rt.audio.input.transcription = Some(Transcription {
            ..Default::default()
        });
        self
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
            model: Some("gpt-realtime".to_string()),
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

#[derive(Debug, Default, Serialize)]
struct AudioInput {
    format: AudioFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    transcription: Option<Transcription>,
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

#[allow(dead_code)]
#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum Voice {
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
        Self::Marin
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
    pub fn new(target: &str) -> Self {
        let target_uri = target.to_string();
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
pub struct Hangup;

impl OpenApiCall for Hangup {
    fn get_url(&self, call_id: &str) -> String {
        format!("{}/{}/hangup", API_ROOT, call_id)
    }
}
