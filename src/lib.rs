pub mod map;
pub mod messages;
pub mod player;
pub mod socket;
pub mod utils;

use std::str::from_utf8;

use map::Map;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GETGameList {
    games: Vec<(
        String, // Game id
        String, // Region
        u8,     // players
        u8,     // max players
        GETGameListGameInfo,
    )>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GETGameListGameInfo {
    c: u8,     // 1 if custom game
    v: String, // Version
    i: String, // Map name
    g: u8,     // Mode
}

#[derive(Debug, Clone)]
pub struct Client {
    pub(crate) prime: u16,
    pub(crate) client_key: String,
    maps: Vec<Map>,
}

impl Client {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        let req_client = reqwest::Client::new();

        let (source, client_key) = tokio::join!(
            async {
                //TODO: get key on the client
                req_client
                    .get("https://api.sys32.dev/v3/source")
                    .send()
                    .await
                    .unwrap()
                    .text()
                    .await
            },
            async {
                // Get the source to extract the prime number for rotating the padding bytes
                // TODO: See if there is a way to get it without the source
                req_client
                    .get("https://api.sys32.dev/v3/key")
                    .send()
                    .await
                    .unwrap()
                    .text()
                    .await
            }
        );

        let source = source?;

        let prime = Regex::new(r"JSON\.parse\('(\d+)'\)")?
            .captures(&source)
            .ok_or("Could not extract prime number from source code")?
            .get(1)
            .ok_or("Could not extract prime number from source code")?
            .as_str()
            .parse::<u16>()?;

        let mut map_counter = -1;
        let tasks = Regex::new(r#"\{"name":"[^"]+",[^']+"#)?
            .find_iter(&source)
            .skip(1)
            .map(|m| {
                map_counter += 1;
                let map_json = m.as_str().to_owned();
                tokio::spawn(async move { Map::new(map_counter as u32, &map_json) })
            })
            .collect::<Vec<JoinHandle<Result<Map, Box<dyn std::error::Error + Sync + Send>>>>>();

        println!("Loading {} maps", map_counter + 1);

        let mut maps = Vec::<Map>::with_capacity(map_counter as usize + 1);
        for task in tasks {
            maps.push(task.await??);
        }

        Ok(Self {
            prime,
            client_key: client_key?,
            maps,
        })
    }

    pub async fn games(&self) -> Result<Vec<Game>, Box<dyn std::error::Error + Sync + Send>> {
        let req_client = reqwest::Client::new();
        let games: GETGameList = req_client
            .get("https://matchmaker.krunker.io/game-list")
            .query(&[("hostname", "krunker.io")])
            .send()
            .await?
            .json()
            .await?;

        let games: Vec<Game> = games
            .games
            .iter()
            .map(|game| Game {
                client_key: self.client_key.clone(),
                id: game.0.clone(),
                region: game.1.clone(),
                players: game.2,
                max_players: game.3,
                custom: game.4.c != 0,
                version: game.4.v.clone(),
                map: game.4.i.clone(),
                mode: game.4.g.to_string(), // TODO: actually convert into mode name
            })
            .collect();

        Ok(games)
    }

    pub fn map(&self, id: Option<u32>, name: Option<&str>) -> Option<&Map> {
        self.maps.iter().find(|map| {
            if let Some(id) = id {
                map.id == id
            } else if let Some(name) = name {
                map.name == name
            } else {
                false
            }
        })
    }
}

#[derive(Debug, Clone)]
pub struct Game {
    pub client_key: String,
    pub id: String,
    pub region: String,
    pub version: String,
    pub players: u8,
    pub max_players: u8,
    pub custom: bool,
    pub map: String,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameInfo {
    pub host: String,
    #[serde(rename = "clientId")]
    pub client_id: String,
    #[serde(rename = "gameId")]
    pub game_id: String,
}

impl Game {
    pub async fn generate_token(&self) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
        let req_client = reqwest::Client::new();

        let token: serde_json::Value = req_client
            .get("https://matchmaker.krunker.io/generate-token")
            .header("client-key", &self.client_key)
            .send()
            .await?
            .json()
            .await?;

        // TODO: hash the token on the client
        let token_hash: Vec<u8> = req_client
            .post("https://api.sys32.dev/v3/token")
            .json(&serde_json::json!(token))
            .send()
            .await?
            .json()
            .await?;

        Ok(from_utf8(&token_hash)?.to_string())
    }

    pub async fn game_info(&self) -> Result<GameInfo, Box<dyn std::error::Error + Sync + Send>> {
        let req_client = reqwest::Client::new();
        let game_info: GameInfo = req_client
            .get("https://matchmaker.krunker.io/seek-game")
            .header("Origin", "https://krunker.io")
            .query(&[
                ("hostname", "krunker.io"),
                ("region", &self.region),
                ("autoChangeGame", "false"),
                ("validationToken", &self.generate_token().await?),
                ("game", &self.id),
                ("dataQuery", &format!("{{\"v\":\"{}\"}}", self.version)),
            ])
            .send()
            .await?
            .json()
            .await?;

        Ok(game_info)
    }
}
