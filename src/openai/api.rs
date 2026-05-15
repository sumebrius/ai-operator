/// Client and serialisable structs for interacting with the OpenAI Rest API.
///
/// This is used for controlling the SIP call itself.
/// Definitions: https://platform.openai.com/docs/api-reference/realtime-calls
use crate::{
    config::{API_ROOT, DEFAULT_MODEL},
    openai::tools::Tool,
};
use std::{fmt, sync::Arc};

use serde::{Serialize, Serializer};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
};
use tokio_rustls::{
    TlsConnector,
    rustls::{ClientConfig, RootCertStore, pki_types::ServerName},
};
use webpki_roots::TLS_SERVER_ROOTS;

/// Again, could this just be a method on the single implementor?
/// Again, yes.
pub trait ApiClient {
    fn bearer_token(&self) -> &str;

    /// Send an event
    async fn execute(&self, action: impl OpenApiCall, call_id: &str) -> Result<(), ApiError> {
        let url = action.get_url(call_id);
        let body = serde_json::to_vec(&action)?;
        post_json(&url, self.bearer_token(), &body).await
    }
}

pub trait OpenApiCall: Serialize {
    fn get_url(&self, call_id: &str) -> String;
}

#[derive(Debug)]
pub struct ApiError {
    status: Option<u16>,
    message: String,
}

impl ApiError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            status: None,
            message: message.into(),
        }
    }

    fn status(status: u16, message: impl Into<String>) -> Self {
        Self {
            status: Some(status),
            message: message.into(),
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.status {
            Some(status) => write!(f, "{} response from OpenAI API: {}", status, self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for ApiError {}

impl From<serde_json::Error> for ApiError {
    fn from(value: serde_json::Error) -> Self {
        Self::new(format!("Unable to encode OpenAI API request: {}", value))
    }
}

impl From<std::io::Error> for ApiError {
    fn from(value: std::io::Error) -> Self {
        Self::new(format!("OpenAI API transport error: {}", value))
    }
}

struct Endpoint {
    tls: bool,
    host: String,
    authority: String,
    port: u16,
    path: String,
}

impl Endpoint {
    fn parse(url: &str) -> Result<Self, ApiError> {
        let (tls, rest, default_port) = if let Some(rest) = url.strip_prefix("https://") {
            (true, rest, 443)
        } else if let Some(rest) = url.strip_prefix("http://") {
            (false, rest, 80)
        } else {
            return Err(ApiError::new(format!(
                "Unsupported OpenAI API URL: {}",
                url
            )));
        };

        let (authority, path) = match rest.split_once('/') {
            Some((authority, path)) => (authority, format!("/{}", path)),
            None => (rest, "/".to_string()),
        };

        let (host, port) = match authority.rsplit_once(':') {
            Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) => {
                let port = port
                    .parse()
                    .map_err(|_| ApiError::new(format!("Invalid OpenAI API port: {}", port)))?;
                (host, port)
            }
            _ => (authority, default_port),
        };

        if host.is_empty() {
            return Err(ApiError::new(format!("Invalid OpenAI API URL: {}", url)));
        }

        Ok(Self {
            tls,
            host: host.to_string(),
            authority: authority.to_string(),
            port,
            path,
        })
    }
}

async fn post_json(url: &str, bearer_token: &str, body: &[u8]) -> Result<(), ApiError> {
    let endpoint = Endpoint::parse(url)?;
    let mut request = format!(
        concat!(
            "POST {} HTTP/1.1\r\n",
            "Host: {}\r\n",
            "Authorization: Bearer {}\r\n",
            "Content-Type: application/json\r\n",
            "Accept: */*\r\n",
            "User-Agent: ai-operator/{}\r\n",
            "Connection: close\r\n",
            "Content-Length: {}\r\n",
            "\r\n"
        ),
        endpoint.path,
        endpoint.authority,
        bearer_token,
        env!("CARGO_PKG_VERSION"),
        body.len()
    )
    .into_bytes();
    request.extend_from_slice(body);

    let tcp_stream = TcpStream::connect((endpoint.host.as_str(), endpoint.port)).await?;
    let response = if endpoint.tls {
        let root_store = RootCertStore::from_iter(TLS_SERVER_ROOTS.iter().cloned());
        let config = ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();
        let connector = TlsConnector::from(Arc::new(config));
        let server_name = ServerName::try_from(endpoint.host.clone())
            .map_err(|err| ApiError::new(format!("Invalid OpenAI API host: {}", err)))?;
        let mut stream = connector.connect(server_name, tcp_stream).await?;
        send_and_read(&mut stream, &request).await?
    } else {
        let mut stream = tcp_stream;
        send_and_read(&mut stream, &request).await?
    };

    handle_response(&response)
}

async fn send_and_read<S>(stream: &mut S, request: &[u8]) -> Result<Vec<u8>, ApiError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    stream.write_all(request).await?;
    stream.flush().await?;

    let mut response = Vec::with_capacity(4096);
    stream.read_to_end(&mut response).await?;
    Ok(response)
}

fn handle_response(response: &[u8]) -> Result<(), ApiError> {
    let header_end = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| ApiError::new("Malformed OpenAI API response"))?;
    let header = String::from_utf8_lossy(&response[..header_end]);
    let status = header
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse::<u16>().ok())
        .ok_or_else(|| ApiError::new("Malformed OpenAI API status line"))?;

    if (200..300).contains(&status) {
        return Ok(());
    }

    let body_start = header_end + 4;
    let body = String::from_utf8_lossy(response.get(body_start..).unwrap_or_default());
    error!("{} response from OpenAI API:\n{}", status, body);
    Err(ApiError::status(status, body))
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
#[derive(Default)]
enum TranscriptionModel {
    whisper_1,
    #[default]
    gpt_4o_mini_transcribe,
    gpt_4o_transcribe,
    gpt_4o_transcribe_diarize,
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
#[derive(Default)]
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
    #[default]
    Cedar,
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
#[derive(Default)]
enum AudioFormat {
    #[serde(rename = "audio/pcm")]
    Pcm { rate: usize },
    #[serde(rename = "audio/pcma")]
    #[default]
    Alaw,
    #[serde(rename = "audio/pcmu")]
    Ulaw,
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
