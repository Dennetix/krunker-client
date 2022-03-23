use std::sync::Arc;

use futures_util::{stream::SplitSink, SinkExt, StreamExt};
use serde::Serialize;
use tokio::{net::TcpStream, sync::Mutex};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        handshake::client::{generate_key, Request},
        Message,
    },
    MaybeTlsStream, WebSocketStream,
};

use crate::{Client, Game};

type WSSink = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>;

#[derive(Debug)]
pub enum SocketMessage {
    Message(String, Vec<serde_json::Value>),
    Error(Box<dyn std::error::Error + Sync + Send>),
    Close,
}

pub struct Socket {
    ws_write: Option<WSSink>,
    messages: Arc<Mutex<Vec<SocketMessage>>>,
    prime: u16,
    num: u16,
}

impl Socket {
    pub fn new(client: &Client) -> Self {
        Self {
            ws_write: None,
            messages: Arc::new(Mutex::new(vec![])),
            prime: client.prime,
            num: 0,
        }
    }

    pub async fn connect(
        &mut self,
        game: &Game,
    ) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
        let game_info = game.game_info().await?;

        let req = Request::builder()
            .header("Host", game_info.host.clone())
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header("Sec-WebSocket-Key", generate_key())
            .header("Origin", "https://krunker.io")
            .uri(format!(
                "wss://{}/ws?gameId={}&clientKey={}",
                game_info.host, game_info.game_id, game_info.client_id
            ))
            .body(())?;

        let (ws_stream, _) = connect_async(req).await?;
        let (ws_write, ws_read) = ws_stream.split();

        self.ws_write = Some(ws_write);
        self.num = 0;

        let messages = self.messages.clone();
        messages.lock().await.clear();
        tokio::spawn(async move {
            ws_read
                .for_each(|msg| async {
                    match msg {
                        Ok(msg) => match msg {
                            Message::Binary(msg) => match Self::decode_message(&msg) {
                                Ok(decoded) => messages
                                    .lock()
                                    .await
                                    .push(SocketMessage::Message(decoded.0, decoded.1)),
                                Err(err) => messages.lock().await.push(SocketMessage::Error(err)),
                            },
                            Message::Close(_) => messages.lock().await.push(SocketMessage::Close),
                            _ => messages.lock().await.push(SocketMessage::Error(
                                "Received unexpected non binary or close message.".into(),
                            )),
                        },
                        Err(err) => messages.lock().await.push(SocketMessage::Error(err.into())),
                    }
                })
                .await;
        });

        Ok(())
    }

    pub async fn send<S: Serialize>(
        &mut self,
        msg: &S,
    ) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
        let msg = self.encode_message(msg)?;
        self.ws_write
            .as_mut()
            .ok_or("Socket not open")?
            .send(Message::Binary(msg))
            .await?;

        Ok(())
    }

    pub async fn close(&mut self) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
        if let Some(ws_write) = self.ws_write.as_mut() {
            ws_write.close().await?;
            self.ws_write = None;
        }
        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        self.ws_write.is_some()
    }

    pub async fn get_messages(&mut self) -> Vec<SocketMessage> {
        self.messages.lock().await.drain(..).collect()
    }

    pub fn encode_message<S: Serialize>(
        &mut self,
        msg: &S,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Sync + Send>> {
        // Encode the actual data with msgpack
        let mut encoded = rmp_serde::encode::to_vec(msg)?;

        // Rotate num by the prime every message
        self.num = (self.num + self.prime) & 0xFF;
        // Append the 2 padding bytes to the message
        encoded.push(((self.num >> 4) & 0xF) as u8);
        encoded.push((self.num & 0xF) as u8);

        Ok(encoded)
    }

    pub fn decode_message(
        msg: &[u8],
    ) -> Result<(String, Vec<serde_json::Value>), Box<dyn std::error::Error + Send + Sync>> {
        // Decode the message without the last two padding bytes wich are unused in the game
        let mut decoded =
            rmp_serde::decode::from_slice::<serde_json::Value>(&msg[..msg.len() - 2])?;
        let decoded = decoded.as_array_mut().unwrap();

        Ok((
            decoded.first().unwrap().as_str().unwrap().to_owned(),
            decoded[1..].to_vec(),
        ))
    }
}
