use std::{collections::VecDeque, f32::consts::PI, sync::Arc, time::Duration};

use tokio::{sync::Mutex, time};
use tracing::{debug, error};

use crate::{
    map::Map,
    messages::{MessageBuilder, MessageParser},
    socket::{Socket, SocketMessage},
    utils::{cell_to_position, Error, Vec3},
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

pub struct PlayerBuilder {
    client: Arc<Mutex<Client>>,
    tick_interval: Duration,
    account: Option<Account>,
}

impl PlayerBuilder {
    pub fn new(client: Arc<Mutex<Client>>) -> Self {
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

    pub async fn connect(&self, game: &Game) -> Result<Arc<Mutex<Player>>, Error> {
        let mut socket = Socket::new(&self.client).await;
        socket.connect(game).await?;

        let player = Arc::new(Mutex::new(Player {
            client: self.client.clone(),
            socket,
            game: game.clone(),
            map: None,
            tick: 0,
            tick_interval: self.tick_interval,
            account: self.account.clone(),
            id: None,
            disconnected: false,
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
const WALK_TO_DISTANCE_XZ_THRESHOLD: f32 = 2.6;
const WALK_TO_DISTANCE_Y_THRESHOLD: f32 = 8.25;

pub struct Player {
    client: Arc<Mutex<Client>>,
    socket: Socket,

    game: Game,
    map: Option<Map>,
    tick: u32,

    tick_interval: Duration,
    account: Option<Account>,

    id: Option<String>,
    disconnected: bool,
    ready: bool,
    in_game: bool,
    walking: bool,
    position: Vec3,
    rotation: f32,
    state_buffer: VecDeque<State>,
}

impl Player {
    pub async fn enter(&mut self) -> Result<(), Error> {
        if self.in_game || self.disconnected {
            return Err("Player already in game or disconnected".into());
        }

        self.socket.send(&MessageBuilder::enter()).await?;
        Ok(())
    }

    pub async fn walk_to(&mut self, position: &Vec3) -> Result<(), Error> {
        if !self.in_game || self.disconnected {
            return Err("Player not in game or disconnected".into());
        }

        if let Some(map) = &self.map {
            if let (Some(start_cell), Some(end_cell)) = (
                map.closest_walkable_cell(&self.position),
                map.closest_walkable_cell(position),
            ) {
                if let Some(path) = map.find_path(&start_cell, &end_cell) {
                    let mut interval = time::interval(self.tick_interval);

                    let bounds = map.bounds;

                    self.walk(true).await?;

                    let mut last_cell = path[0];
                    'outer: for cell in path.iter().skip(1) {
                        let cell_pos = cell_to_position(&bounds, cell);

                        debug!("Moving to cell {:?}", cell);

                        loop {
                            if self.disconnected {
                                break 'outer;
                            }

                            if self.in_game {
                                if let Err(err) = self.tick().await {
                                    return Err(err);
                                }
                            } else {
                                return Err("Game ended or Player died".into());
                            }

                            self.look_at(&cell_pos);

                            interval.tick().await;

                            if self
                                .position
                                .max_diff_xz(&cell_pos, WALK_TO_DISTANCE_XZ_THRESHOLD)
                                && (last_cell.1 >= cell.1
                                    || self
                                        .position
                                        .max_diff_y(&cell_pos, WALK_TO_DISTANCE_Y_THRESHOLD))
                            {
                                debug!("Arrived at cell {:?}", cell);
                                break;
                            }
                        }

                        last_cell = *cell;
                    }

                    debug!("Arrived at end cell");
                    self.walk(false).await?;

                    Ok(())
                } else {
                    Err("No path found".into())
                }
            } else {
                Err("Position not walkable".into())
            }
        } else {
            Err("Map information not available".into())
        }
    }

    pub async fn walk(&mut self, state: bool) -> Result<(), Error> {
        if !self.in_game || self.disconnected {
            return Err("Player not in game or disconnected".into());
        }

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
    }

    pub async fn shoot(&mut self, state: bool) -> Result<(), Error> {
        if !self.in_game || self.disconnected {
            return Err("Player not in game or disconnected".into());
        }

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

    pub fn look_at(&mut self, position: &Vec3) {
        self.rotation(
            (position.z - self.position.z).atan2(position.x - self.position.x) + PI / 2.0,
        );
    }

    pub async fn disconnect(&mut self) -> Result<(), Error> {
        self.ready = false;
        self.in_game = false;

        if !self.disconnected {
            self.disconnected = true;
            self.socket.close().await?;
        }

        Ok(())
    }

    pub fn in_game(&self) -> bool {
        self.in_game
    }

    pub fn map(&self) -> Option<&Map> {
        self.map.as_ref()
    }

    fn run_tick(this: Arc<Mutex<Self>>) {
        tokio::spawn(async move {
            let mut interval = time::interval(this.lock().await.tick_interval);
            loop {
                interval.tick().await;

                let mut this_lock = this.lock().await;

                if this_lock.disconnected {
                    break;
                }

                if let Err(err) = this_lock.tick().await {
                    error!("Failed to execute player tick: {}", err);
                }
            }
        });
    }

    async fn tick(&mut self) -> Result<(), Error> {
        if self.in_game {
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
        }

        for msg in self.socket.get_messages().await {
            match msg {
                SocketMessage::Message(msg_type, msg) => {
                    if let Err(err) = self.process_message(&msg_type, msg).await {
                        error!("Failed to process server message '{}': {}", msg_type, err);
                    }
                }
                _ => (),
            }
        }

        Ok(())
    }

    async fn process_message(
        &mut self,
        msg_type: &str,
        msg: Vec<serde_json::Value>,
    ) -> Result<(), Error> {
        match msg_type {
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
                self.game.update_info().await?;
                self.map = self
                    .client
                    .lock()
                    .await
                    .maps
                    .iter()
                    .find(|map| map.name == self.game.map)
                    .cloned();
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
