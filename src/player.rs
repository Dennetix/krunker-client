use futures_util::StreamExt;

use crate::{
    socket::{Socket, SocketMessage},
    Client, GameInfo,
};

pub struct Player {
    socket: Socket,
}

impl Player {
    pub fn new(client: &Client) -> Self {
        Self {
            socket: Socket::new(client),
        }
    }

    pub async fn join(&mut self, game_info: &GameInfo) -> Result<(), Box<dyn std::error::Error>> {
        self.socket
            .connect(game_info)
            .await?
            .fold(self, |this, msg| async move {
                match msg {
                    SocketMessage::Message(msg_type, msg) => {
                        println!("{} {:?}", msg_type, msg);
                        match msg_type.as_str() {
                            "pi" => {
                                this.socket
                                    .send(&serde_json::json!(["po"]))
                                    .await
                                    .expect("Could not send pong");
                            }
                            _ => (),
                        }
                    }
                    _ => (),
                }
                this
            })
            .await;

        Ok(())
    }
}
