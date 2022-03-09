use std::pin::Pin;

use futures_util::{stream::SplitSink, SinkExt, Stream, StreamExt};
use serde::Serialize;
use tokio::net::TcpStream;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        handshake::client::{generate_key, Request},
        Message,
    },
    MaybeTlsStream, WebSocketStream,
};

use crate::{Client, GameInfo};

type WSSink = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>;

pub enum SocketMessage {
    Message(String, Vec<serde_json::Value>),
    Error(Box<dyn std::error::Error>),
    Close,
}

pub struct Socket {
    ws_write: Option<WSSink>,
    prime: u16,
    num: u16,
}

impl Socket {
    pub fn new(client: &Client) -> Self {
        Self {
            ws_write: None,
            prime: client.prime,
            num: 0,
        }
    }

    pub async fn connect(
        &mut self,
        game_info: &GameInfo,
    ) -> Result<Pin<Box<dyn Stream<Item = SocketMessage> + Send>>, Box<dyn std::error::Error>> {
        self.num = 0;

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

        let decoded_read = ws_read
            .map(|msg| match msg {
                Ok(msg) => match msg {
                    Message::Binary(msg) => match Self::decode_message(&msg) {
                        Ok(decoded) => SocketMessage::Message(decoded.0, decoded.1),
                        Err(err) => SocketMessage::Error(err),
                    },
                    Message::Close(_) => SocketMessage::Close,
                    _ => SocketMessage::Error(
                        "Received unexpected non binary or close message.".into(),
                    ),
                },
                Err(err) => SocketMessage::Error(err.into()),
            })
            .boxed();

        Ok(decoded_read)
    }

    pub async fn send<D>(&mut self, msg: &D) -> Result<(), Box<dyn std::error::Error>>
    where
        D: Serialize + std::fmt::Debug,
    {
        if self.ws_write.is_some() {
            println!("{:?}", msg);
            let msg = self.encode_message(msg)?;
            self.ws_write
                .as_mut()
                .unwrap()
                .send(Message::Binary(msg))
                .await?;
        }

        Ok(())
    }

    pub fn encode_message<S>(&mut self, msg: &S) -> Result<Vec<u8>, Box<dyn std::error::Error>>
    where
        S: Serialize,
    {
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
