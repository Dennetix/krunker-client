use std::{collections::VecDeque, f32::consts::PI, sync::Arc, time::Duration};

use tokio::{sync::Mutex, time};

use crate::{
    messages::{MessageBuilder, MessageParser},
    socket::{Socket, SocketMessage},
    utils::Vec3,
    Client, Game,
};

#[derive(Debug, Clone)]
pub struct Account {
    pub username: String,
    pub password: String,
}

#[derive(Debug)]
struct State {
    tick: u32,
    position: Vec3,
    rotation: f32,
    walking: bool,
}

pub struct PlayerBuilder<'a> {
    client: &'a Client,
    tick_interval: Duration,
    account: Option<Account>,
}

impl<'a> PlayerBuilder<'a> {
    pub fn new(client: &'a Client) -> Self {
        Self {
            client,
            tick_interval: Duration::from_millis(66),
            account: None,
        }
    }

    pub fn tick_interval(mut self, tick_interval: Duration) -> Self {
        self.tick_interval = tick_interval;
        self
    }

    pub fn account(mut self, account: Account) -> Self {
        self.account = Some(account);
        self
    }

    pub async fn connect(
        &self,
        game: &Game,
    ) -> Result<Arc<Mutex<Player>>, Box<dyn std::error::Error + Sync + Send>> {
        let mut socket = Socket::new(self.client);
        socket.connect(game).await?;

        let player = Arc::new(Mutex::new(Player {
            socket,
            tick: 0,
            tick_interval: self.tick_interval,
            account: self.account.clone(),
            id: None,
            ready: false,
            in_game: false,
            walking: false,
            position: Vec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            rotation: 0.0,
            state_buffer: VecDeque::new(),
        }));

        Player::run_tick(player.clone());

        Ok(player)
    }
}

const MOVEMENT_SPEED: f32 = 0.0000459;

pub struct Player {
    socket: Socket,
    tick: u32,

    tick_interval: Duration,
    account: Option<Account>,

    id: Option<String>,
    ready: bool,
    in_game: bool,
    walking: bool,
    position: Vec3,
    rotation: f32,
    state_buffer: VecDeque<State>,
}

impl Player {
    pub async fn enter(&mut self) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
        if !self.in_game {
            self.socket.send(&MessageBuilder::enter()).await?;
            Ok(())
        } else {
            Err("Player already in game".into())
        }
    }

    pub async fn walk(
        &mut self,
        state: bool,
    ) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
        if self.in_game {
            self.walking = state;
            self.socket
                .send(&MessageBuilder::tick(
                    self.tick,
                    &self.tick_interval,
                    None,
                    Some(format!("{{\"0-4\": {}}}", if state { 1 } else { -1 })),
                )?)
                .await?;
            self.tick += 1;
            Ok(())
        } else {
            Err("Player not in game".into())
        }
    }

    pub async fn shoot(
        &mut self,
        state: bool,
    ) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
        if self.in_game {
            self.socket
                .send(&MessageBuilder::tick(
                    self.tick,
                    &self.tick_interval,
                    None,
                    Some(format!(
                        "{{\"0-5\": {s}, \"0-6\": {s}}}",
                        s = if state { 1 } else { 0 }
                    )),
                )?)
                .await?;
            self.tick += 1;
            Ok(())
        } else {
            Err("Player not in game".into())
        }
    }

    pub fn rotation(&mut self, rotation: f32) {
        self.rotation = rotation;
        if self.rotation > 2.0 * PI {
            self.rotation -= 2.0 * PI;
        } else if self.rotation < 0.0 {
            self.rotation += 2.0 * PI;
        }
    }

    pub fn rotate(&mut self, rotation: f32) {
        self.rotation(self.rotation + rotation);
    }

    pub fn in_game(&self) -> bool {
        self.in_game
    }

    fn run_tick(this: Arc<Mutex<Self>>) {
        tokio::spawn(async move {
            let mut interval = time::interval(this.lock().await.tick_interval);
            loop {
                interval.tick().await;

                let mut this_lock = this.lock().await;

                if this_lock.in_game {
                    if let Err(err) = this_lock.tick().await {
                        println!("Failed to execute player tick: {}", err);
                    }
                }

                for msg in this_lock.socket.get_messages().await {
                    match msg {
                        SocketMessage::Message(msg_type, msg) => {
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

    async fn tick(&mut self) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
        self.socket
            .send(&MessageBuilder::tick(
                self.tick,
                &self.tick_interval,
                Some(self.rotation),
                None,
            )?)
            .await?;
        self.tick += 1;

        if self.walking {
            let dist = self.tick_interval.as_micros() as f32 * MOVEMENT_SPEED;
            self.position.x += dist * self.rotation.sin();
            self.position.z += dist * -self.rotation.cos();
        }

        self.state_buffer.push_back(State {
            tick: self.tick,
            position: self.position,
            rotation: self.rotation,
            walking: self.walking,
        });

        Ok(())
    }

    async fn process_message(
        &mut self,
        msg_type: String,
        msg: Vec<serde_json::Value>,
    ) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
        match msg_type.as_str() {
            // ping
            "pi" => {
                self.socket.send(&MessageBuilder::pong()).await?;
            }
            // requires response to initialize the connection
            "load" => {
                self.socket.send(&MessageBuilder::load()).await?;
            }
            // includes player id
            "io-init" => {
                self.id = Some(MessageParser::io_init(&msg)?);
            }
            // sent after connect and at the start of every game
            "init" => {
                if self.ready {
                    self.enter().await?;
                }
            }
            // sent after the server has sent all the necessary information after connect
            "ready" => {
                if let Some(account) = self.account.as_mut() {
                    self.socket.send(&MessageBuilder::login(account)).await?;
                } else {
                    self.ready = true;
                    self.enter().await?;
                }
            }
            // spawn in game
            "0" => {
                if let Some(spawn_position) =
                    MessageParser::spawn_position(&msg, self.id.as_ref().ok_or("Id not set")?)?
                {
                    self.in_game = true;
                    self.walking = false;
                    self.position = spawn_position;

                    self.socket.send(&MessageBuilder::init_tick()).await?;
                    self.tick = 1;
                }
            }
            // player update
            "l" => {
                let state = MessageParser::player_state(&msg)?;
                if state.is_dead {
                    self.in_game = false;
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    self.enter().await?;
                } else if let (Some(tick), Some(position)) = (state.tick, state.position) {
                    self.state_buffer.retain(|s| s.tick >= tick);

                    if let Some(past_state) = self.state_buffer.front() {
                        // Reconciliate the position if there is too much difference between the states
                        if !position.max_diff_xz(&past_state.position, 0.5) {
                            self.position = position;
                            for state in self.state_buffer.iter_mut() {
                                if state.walking {
                                    let dist =
                                        self.tick_interval.as_micros() as f32 * MOVEMENT_SPEED;
                                    self.position.x += dist * state.rotation.sin();
                                    self.position.z += dist * -state.rotation.cos();
                                }

                                state.position = self.position;
                            }
                        }
                    }
                } else {
                    return Err("Didn't receive position on player update".into());
                }
            }
            // game has ended
            "end" => {
                self.in_game = false;
            }
            _ => (),
        }

        Ok(())
    }
}
