use std::fmt::Display;

use futures_util::{SinkExt, stream::SplitSink};
use serde::Serialize;
use serde_json::to_string;
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite};

type WsSink = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, tungstenite::Message>;
pub struct MessageSink(WsSink);

impl MessageSink {
    pub fn new(sink: WsSink) -> Self {
        Self(sink)
    }

    pub async fn send<S: Serialize + Display>(
        &mut self,
        message: &S,
    ) -> Result<(), tungstenite::Error> {
        info!("Sending client event: {}", message);
        let item = tungstenite::Message::text(to_string(message).expect("Bad JSON serialisation"));
        self.0
            .send(item)
            .await
            .inspect_err(|err| error!("Error sending event: {:?}", err))
    }
}

#[derive(Serialize)]
#[serde(tag = "type")]
pub enum Response {
    #[serde(rename = "response.create")]
    Create,
}
impl Display for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Create Response")
    }
}

#[derive(Serialize)]
#[serde(tag = "type")]
pub enum Conversation {
    #[serde(rename = "conversation.item.create")]
    ItemCreate(ConversationItem),
}
impl Display for Conversation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Conversation::ItemCreate(item) => item.fmt(f),
        }
    }
}

#[derive(Serialize)]
pub struct ConversationItem {
    item: ConversationItemType,
}
impl Display for ConversationItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.item {
            ConversationItemType::FunctionCallOutput(output) => {
                write!(f, "Function response: {}", output.output)
            }
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "type")]
pub enum ConversationItemType {
    #[serde(rename = "function_call_output")]
    FunctionCallOutput(FunctionCallOutput),
}

#[derive(Serialize)]
pub struct FunctionCallOutput {
    pub output: String,
    pub call_id: String,
}

impl FunctionCallOutput {
    pub fn error(&mut self, message: &str) {
        self.output = serde_json::json!({"error": format!("{:?}", message)}).to_string();
    }
}

impl From<FunctionCallOutput> for Conversation {
    fn from(value: FunctionCallOutput) -> Self {
        Conversation::ItemCreate(ConversationItem {
            item: ConversationItemType::FunctionCallOutput(value),
        })
    }
}
