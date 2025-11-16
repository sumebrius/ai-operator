use crate::config::WS_URI;
use futures_util::{StreamExt, stream::SplitStream};
use tokio::net::TcpStream;
use tokio_tungstenite::{self, MaybeTlsStream, WebSocketStream, tungstenite};

pub mod client_event;
pub mod server_event;

pub trait WebsocketClient {
    fn get_token(&self) -> &str;

    async fn connect_ws(
        &self,
        call_id: &str,
    ) -> Result<
        (
            client_event::MessageSink,
            SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
        ),
        tungstenite::Error,
    > {
        let uri = http::Uri::try_from(format!("{}?call_id={}", WS_URI, call_id))
            .expect("Fucky WS URI or Header");
        let request = tungstenite::ClientRequestBuilder::new(uri)
            .with_header("Authorization", format!("Bearer {}", self.get_token()));
        let (stream, _) = tokio_tungstenite::connect_async(request).await?;

        let (raw_tx, rx) = stream.split();
        Ok((client_event::MessageSink::new(raw_tx), rx))
    }
}
