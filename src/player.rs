use std::{sync::Arc, time::Duration};

use tokio::{sync::Mutex, time};

use crate::{
    socket::{Socket, SocketMessage},
    Client, GameInfo,
};

pub struct PlayerBuilder<'a> {
    client: &'a Client,
    tick_interval: Duration,
}

impl<'a> PlayerBuilder<'a> {
    pub fn new(client: &'a Client) -> Self {
        Self {
            client,
            tick_interval: Duration::from_millis(40),
        }
    }

    pub fn tick_interval(mut self, tick_interval: Duration) -> Self {
        self.tick_interval = tick_interval;
        self
    }

    pub async fn connect(
        &self,
        game_info: &GameInfo,
    ) -> Result<Arc<Mutex<Player>>, Box<dyn std::error::Error + Sync + Send>> {
        let mut socket = Socket::new(self.client);
        socket.connect(game_info).await?;

        let player = Arc::new(Mutex::new(Player {
            socket,
            tick_number: 0,
        }));

        Player::run_tick(player.clone(), self.tick_interval);

        Ok(player)
    }
}

pub struct Player {
    socket: Socket,
    tick_number: u32,
}

impl Player {
    fn run_tick(this: Arc<Mutex<Self>>, tick_interval: Duration) {
        tokio::spawn(async move {
            let mut interval = time::interval(tick_interval);
            interval.tick().await;

            loop {
                interval.tick().await;

                let mut this_lock = this.lock().await;

                this_lock.tick_number += 1;

                for msg in this_lock.socket.get_messages().await {
                    match msg {
                        SocketMessage::Message(msg_type, msg) => {
                            println!("{} {:?}", msg_type, msg);
                            if let Err(err) = this_lock.process_message(msg_type, msg).await {
                                println!("Failed to process server message: {}", err);
                            }
                        }
                        _ => (),
                    }
                }
            }
        });
    }

    pub async fn process_message(
        &mut self,
        msg_type: String,
        _: Vec<serde_json::Value>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match msg_type.as_str() {
            "pi" => {
                self.socket.send(&serde_json::json!(["po"])).await?;
            }
            _ => (),
        }

        Ok(())
    }
}
