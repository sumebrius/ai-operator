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

    pub async fn send(&mut self, message: &impl Serialize) -> Result<(), tungstenite::Error> {
        let item = tungstenite::Message::text(to_string(message).expect("Bad JSON serialisation"));
        self.0.send(item).await
    }
}

#[derive(Serialize)]
#[serde(tag = "type")]
pub enum Response {
    #[serde(rename = "response.create")]
    Create,
}

#[derive(Serialize)]
#[serde(tag = "type")]
pub enum Conversation {
    #[serde(rename = "conversation.item.create")]
    ItemCreate(ConversationItem),
}

#[derive(Serialize)]
pub struct ConversationItem {
    item: ConversationItemType,
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

impl From<FunctionCallOutput> for Conversation {
    fn from(value: FunctionCallOutput) -> Self {
        Conversation::ItemCreate(ConversationItem {
            item: ConversationItemType::FunctionCallOutput(value),
        })
    }
}
