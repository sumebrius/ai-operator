/// Active and serialisable structs for interacting with the OpenAI WebSocket API
///
/// This is used for actually interacting with the running model.
use crate::config::WS_URI;
use futures_util::StreamExt;
use tokio_tungstenite::tungstenite;

pub mod client_event;
pub mod server_event;

/// Could this just be a method on the single implementor directly?
///
/// Yeah, actually.
pub trait WebsocketClient {
    fn get_token(&self) -> &str;

    async fn connect_websocket(
        &self,
        call_id: &str,
    ) -> Result<(client_event::MessageSink, server_event::MessageSource), tungstenite::Error> {
        let uri = http::Uri::try_from(format!("{}?call_id={}", WS_URI, call_id))
            .expect("Fucky WS URI or Header");
        let request = tungstenite::ClientRequestBuilder::new(uri)
            .with_header("Authorization", format!("Bearer {}", self.get_token()));
        let (stream, _) = tokio_tungstenite::connect_async(request)
            .await
            .inspect_err(|err| {
                error!("Error starting websocket connection: {}", err);
            })?;

        info!("Websocket control session initialised");
        let (raw_tx, rx) = stream.split();
        Ok((
            client_event::MessageSink::new(raw_tx),
            server_event::MessageSource::new(rx, call_id),
        ))
    }
}
