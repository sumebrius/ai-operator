use crate::config::WS_URI;
use futures_util::StreamExt;
use tokio::net::TcpStream;
use tokio_tungstenite::{
    self, MaybeTlsStream, WebSocketStream,
    tungstenite::{self, Message},
};

pub trait WebsocketClient {
    fn get_token(&self) -> &str;

    async fn connect_ws(
        &self,
        call_id: &str,
    ) -> Result<
        (
            futures_util::stream::SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
            futures_util::stream::SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
        ),
        tungstenite::Error,
    > {
        let uri = http::Uri::try_from(format!("{}?call_id={}", WS_URI, call_id))
            .expect("Fucky WS URI or Header");
        let request = tungstenite::ClientRequestBuilder::new(uri)
            .with_header("Authorization", format!("Bearer {}", self.get_token()));
        let (stream, _) = tokio_tungstenite::connect_async(request).await?;
        Ok(stream.split())
    }
}
