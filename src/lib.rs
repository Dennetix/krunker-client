pub mod map;
pub mod messages;
pub mod player;
pub mod socket;
pub mod utils;

use std::{str::from_utf8, sync::Arc};

use futures_util::future::try_join_all;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::info;

use crate::{
    map::{Map, RawMap},
    utils::Error,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawGameInfo {
    #[serde(rename = "c")]
    custom: u8,
    #[serde(rename = "v")]
    version: String,
    #[serde(rename = "i")]
    map: String,
    #[serde(rename = "g")]
    mode: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawGame(
    String, // Game id
    String, // Region
    u8,     // players
    u8,     // max players
    RawGameInfo,
);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawGameList {
    games: Vec<RawGame>,
}

#[derive(Debug, Clone)]
pub struct Client {
    pub(crate) prime: u16,
    pub(crate) client_key: String,
    maps: Vec<Map>,
}

impl Client {
    pub async fn new() -> Result<Arc<Mutex<Self>>, Error> {
        info!("Downloading krunker source...");

        let req_client = reqwest::Client::new();

        let (source, client_key) = tokio::join!(
            async {
                // Get the source to extract the prime number for rotating the padding bytes
                req_client
                    .get("https://api.sys32.dev/v3/source")
                    .send()
                    .await
                    .unwrap()
                    .text()
                    .await
            },
            async {
                // TODO: get key on the client
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

        Ok(Arc::new(Mutex::new(Self {
            prime,
            client_key: client_key?,
            maps: Self::load_maps(&source).await?,
        })))
    }

    async fn load_maps(source: &str) -> Result<Vec<Map>, Error> {
        let maps = Regex::new(r#"\{"name":"[^"]+",[^']+"#)?
            .find_iter(source)
            .skip(1)
            .filter_map(|map| {
                let raw_map = serde_json::from_str::<RawMap>(map.as_str());
                match raw_map {
                    Ok(raw_map) => {
                        if raw_map.config.modes.contains(&0) {
                            Some(Ok(raw_map))
                        } else {
                            None
                        }
                    }
                    Err(err) => Some(Err(err)),
                }
            })
            .collect::<Vec<_>>();

        info!("Parsing {} maps...", maps.len());

        let tasks = maps
            .into_iter()
            .map(|map| {
                let raw_map = map?;
                Ok(tokio::spawn(async move { Map::new(&raw_map) }))
            })
            .collect::<Result<Vec<_>, Error>>()?;

        try_join_all(tasks)
            .await?
            .into_iter()
            .collect::<Result<Vec<_>, Error>>()
    }

    pub async fn games(&self) -> Result<Vec<Game>, Error> {
        let req_client = reqwest::Client::new();
        let raw_games: RawGameList = req_client
            .get("https://matchmaker.krunker.io/game-list")
            .query(&[("hostname", "krunker.io")])
            .send()
            .await?
            .json()
            .await?;

        let games: Vec<Game> = raw_games
            .games
            .into_iter()
            .map(|game| Game {
                client_key: self.client_key.clone(),
                id: game.0,
                region: game.1,
                players: game.2,
                max_players: game.3,
                custom: game.4.custom != 0,
                version: game.4.version,
                map: game.4.map,
                mode: game.4.mode,
            })
            .collect();

        Ok(games)
    }

    pub fn available_maps(&self) -> Vec<String> {
        self.maps
            .iter()
            .map(|map| map.name.clone())
            .collect::<Vec<_>>()
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
    pub mode: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameConnectInfo {
    pub host: String,
    #[serde(rename = "clientId")]
    pub client_id: String,
    #[serde(rename = "gameId")]
    pub game_id: String,
}

impl Game {
    pub async fn validation_token(&self) -> Result<String, Error> {
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

    pub async fn connect_info(&self) -> Result<GameConnectInfo, Error> {
        let req_client = reqwest::Client::new();
        let game_info: GameConnectInfo = req_client
            .get("https://matchmaker.krunker.io/seek-game")
            .header("Origin", "https://krunker.io")
            .query(&[
                ("hostname", "krunker.io"),
                ("region", &self.region),
                ("autoChangeGame", "false"),
                ("validationToken", &self.validation_token().await?),
                ("game", &self.id),
                ("dataQuery", &format!("{{\"v\":\"{}\"}}", self.version)),
            ])
            .send()
            .await?
            .json()
            .await?;

        Ok(game_info)
    }

    pub async fn update_info(&mut self) -> Result<(), Error> {
        let req_client = reqwest::Client::new();
        let raw_game: RawGame = req_client
            .get("https://matchmaker.krunker.io/game-info")
            .query(&[("game", &self.id)])
            .send()
            .await?
            .json()
            .await?;

        self.players = raw_game.2;
        self.mode = raw_game.4.mode;
        self.map = raw_game.4.map;

        Ok(())
    }
}
