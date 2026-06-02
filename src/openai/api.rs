/// Client and serialisable structs for interacting with the OpenAI Rest API.
///
/// This is used for controlling the SIP call itself.
/// Definitions: https://platform.openai.com/docs/api-reference/realtime-calls
use crate::{
    config::{API_ROOT, DEFAULT_MODEL},
    openai::tools::Tool,
};
use std::{fmt, sync::Arc};

use bytes::Bytes;
use http::{Request, StatusCode, header};
use http_body_util::{BodyExt, Full};
use hyper::client::conn::http1;
use hyper_util::rt::TokioIo;
use serde::{Serialize, Serializer};
use tokio::{net::TcpStream, sync::Mutex};
use tokio_rustls::{
    TlsConnector,
    rustls::{ClientConfig, RootCertStore, pki_types::ServerName},
};
use tokio_tungstenite::MaybeTlsStream;
use webpki_roots::TLS_SERVER_ROOTS;

type RequestBody = Full<Bytes>;
type ApiSender = http1::SendRequest<RequestBody>;

/// Again, could this just be a method on the single implementor?
/// Again, yes.
pub trait ApiClient {
    fn api_client(&self) -> &ApiHttpClient;

    /// Send an event
    async fn execute(&self, action: impl OpenApiCall, call_id: &str) -> Result<(), ApiError> {
        let url = action.get_url(call_id);
        let body = serde_json::to_vec(&action)?;
        self.api_client().post_json(&url, body).await
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

impl From<hyper::Error> for ApiError {
    fn from(value: hyper::Error) -> Self {
        Self::new(format!("OpenAI API HTTP error: {}", value))
    }
}

impl From<http::Error> for ApiError {
    fn from(value: http::Error) -> Self {
        Self::new(format!("Unable to build OpenAI API request: {}", value))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Endpoint {
    tls: bool,
    host: String,
    authority: String,
    port: u16,
}

struct ApiTarget {
    endpoint: Endpoint,
    path: String,
}

impl ApiTarget {
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
            endpoint: Endpoint {
                tls,
                host: host.to_string(),
                authority: authority.to_string(),
                port,
            },
            path,
        })
    }
}

struct ApiConnection {
    endpoint: Endpoint,
    sender: ApiSender,
}

pub struct ApiHttpClient {
    bearer_token: Arc<String>,
    connector: TlsConnector,
    connection: Mutex<Option<ApiConnection>>,
}

impl ApiHttpClient {
    pub fn new(bearer_token: Arc<String>) -> Self {
        let root_store = RootCertStore::from_iter(TLS_SERVER_ROOTS.iter().cloned());
        let config = ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        Self {
            bearer_token,
            connector: TlsConnector::from(Arc::new(config)),
            connection: Mutex::new(None),
        }
    }

    async fn post_json(&self, url: &str, body: Vec<u8>) -> Result<(), ApiError> {
        let target = ApiTarget::parse(url)?;
        let request = self.build_request(&target, body)?;
        let mut connection = self.connection.lock().await;

        self.ensure_connection(&mut connection, &target.endpoint)
            .await?;

        let response = match connection
            .as_mut()
            .expect("OpenAI API connection not initialised")
            .sender
            .send_request(request)
            .await
        {
            Ok(response) => response,
            Err(err) => {
                *connection = None;
                return Err(err.into());
            }
        };

        let status = response.status();
        let body = response.into_body().collect().await?.to_bytes();

        if connection
            .as_ref()
            .is_some_and(|connection| connection.sender.is_closed())
        {
            *connection = None;
        }

        handle_response(status, &body)
    }

    fn build_request(
        &self,
        target: &ApiTarget,
        body: Vec<u8>,
    ) -> Result<Request<RequestBody>, ApiError> {
        Ok(Request::builder()
            .method("POST")
            .uri(target.path.as_str())
            .header(header::HOST, target.endpoint.authority.as_str())
            .header(
                header::AUTHORIZATION,
                format!("Bearer {}", self.bearer_token),
            )
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ACCEPT, "*/*")
            .header(
                header::USER_AGENT,
                concat!("ai-operator/", env!("CARGO_PKG_VERSION")),
            )
            .body(Full::new(Bytes::from(body)))?)
    }

    async fn ensure_connection(
        &self,
        connection: &mut Option<ApiConnection>,
        endpoint: &Endpoint,
    ) -> Result<(), ApiError> {
        if connection.as_ref().is_none_or(|connection| {
            connection.endpoint != *endpoint || connection.sender.is_closed()
        }) {
            *connection = Some(self.connect(endpoint).await?);
        }

        if let Some(existing) = connection.as_mut() {
            if let Err(err) = existing.sender.ready().await {
                debug!("Reconnecting OpenAI API HTTP session: {}", err);
                *connection = Some(self.connect(endpoint).await?);
                connection
                    .as_mut()
                    .expect("OpenAI API connection not initialised")
                    .sender
                    .ready()
                    .await?;
            }
        }

        Ok(())
    }

    async fn connect(&self, endpoint: &Endpoint) -> Result<ApiConnection, ApiError> {
        let tcp_stream = TcpStream::connect((endpoint.host.as_str(), endpoint.port)).await?;
        let stream = if endpoint.tls {
            let server_name = ServerName::try_from(endpoint.host.clone())
                .map_err(|err| ApiError::new(format!("Invalid OpenAI API host: {}", err)))?;
            MaybeTlsStream::Rustls(self.connector.connect(server_name, tcp_stream).await?)
        } else {
            MaybeTlsStream::Plain(tcp_stream)
        };

        let (sender, connection) = http1::handshake(TokioIo::new(stream)).await?;
        tokio::spawn(async move {
            if let Err(err) = connection.await {
                debug!("OpenAI API HTTP connection ended: {}", err);
            }
        });

        Ok(ApiConnection {
            endpoint: endpoint.clone(),
            sender,
        })
    }
}

fn handle_response(status: StatusCode, body: &[u8]) -> Result<(), ApiError> {
    if status.is_success() {
        Ok(())
    } else {
        let body = String::from_utf8_lossy(body);
        error!("{} response from OpenAI API:\n{}", status, body);
        Err(ApiError::status(status.as_u16(), body))
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
        time::timeout,
    };

    #[tokio::test]
    async fn decodes_chunked_error_body() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            read_request(&mut stream).await;
            stream
                .write_all(
                    b"HTTP/1.1 400 Bad Request\r\n\
                    Transfer-Encoding: chunked\r\n\
                    Content-Type: application/json\r\n\
                    \r\n\
                    f\r\n{\"error\":\"bad\"}\r\n\
                    0\r\n\
                    \r\n",
                )
                .await
                .unwrap();
        });

        let client = ApiHttpClient::new(Arc::new("test-token".to_string()));
        let err = client
            .post_json(&format!("http://{}/test", addr), b"{}".to_vec())
            .await
            .unwrap_err();

        let err = err.to_string();
        assert!(err.contains(r#"{"error":"bad"}"#));
        assert!(!err.contains("\nf\n"));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn reuses_open_http_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            for _ in 0..2 {
                read_request(&mut stream).await;
                stream
                    .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
                    .await
                    .unwrap();
            }
        });

        let client = ApiHttpClient::new(Arc::new("test-token".to_string()));
        timeout(Duration::from_secs(2), async {
            client
                .post_json(&format!("http://{}/first", addr), b"{}".to_vec())
                .await
                .unwrap();
            client
                .post_json(&format!("http://{}/second", addr), b"{}".to_vec())
                .await
                .unwrap();
        })
        .await
        .unwrap();
        server.await.unwrap();
    }

    async fn read_request(stream: &mut TcpStream) -> Vec<u8> {
        let mut buffer = Vec::new();
        loop {
            let mut chunk = [0; 1024];
            let read = stream.read(&mut chunk).await.unwrap();
            assert_ne!(read, 0, "connection closed before request completed");
            buffer.extend_from_slice(&chunk[..read]);

            if let Some(header_end) = find_header_end(&buffer) {
                let content_length = content_length(&buffer[..header_end]);
                let request_end = header_end + 4 + content_length;
                while buffer.len() < request_end {
                    let read = stream.read(&mut chunk).await.unwrap();
                    assert_ne!(read, 0, "connection closed before body completed");
                    buffer.extend_from_slice(&chunk[..read]);
                }
                return buffer;
            }
        }
    }

    fn find_header_end(buffer: &[u8]) -> Option<usize> {
        buffer.windows(4).position(|window| window == b"\r\n\r\n")
    }

    fn content_length(headers: &[u8]) -> usize {
        String::from_utf8_lossy(headers)
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse().unwrap())
            })
            .unwrap_or_default()
    }
}
