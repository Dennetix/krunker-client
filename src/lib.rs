pub mod player;
pub mod socket;

use std::str::from_utf8;

use regex::Regex;
use serde::{Deserialize, Serialize};

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
    i: String, // Map name,
    g: u8,     // Mode
}

#[derive(Debug, Clone)]
pub struct Client {
    pub(crate) prime: u16,
    pub(crate) client_key: String,
}

impl Client {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let req_client = reqwest::Client::new();

        //TODO: get key on the client
        let client_key: String = req_client
            .get("https://api.sys32.dev/v3/key")
            .send()
            .await?
            .text()
            .await?;

        // Get the source to extract the prime number for rotating the padding bytes
        // TODO: See if there is a way to get it without the source
        let source: String = req_client
            .get("https://api.sys32.dev/v3/source")
            .send()
            .await?
            .text()
            .await?;

        let prime = Regex::new(r"JSON\.parse\('(\d+)'\)")?
            .captures(&source)
            .expect("Could not extract prime number from source code")
            .get(1)
            .expect("Could not extract prime number from source code")
            .as_str()
            .parse::<u16>()?;

        println!("{}", prime);

        Ok(Self { prime, client_key })
    }

    pub async fn games(&self) -> Result<Vec<Game>, Box<dyn std::error::Error>> {
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
                custom: if game.4.c == 0 { false } else { true },
                version: game.4.v.clone(),
                map: game.4.i.clone(),
                mode: game.4.g.to_string(), // TODO: actually convert into mode name
            })
            .collect();

        Ok(games)
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
    pub async fn generate_token(&self) -> Result<String, Box<dyn std::error::Error>> {
        let req_client = reqwest::Client::new();

        let token: serde_json::Value = req_client
            .get("https://matchmaker.krunker.io/generate-token")
            .header("client-key", &self.client_key)
            .send()
            .await?
            .json()
            .await?;

        // TODO: hash the token on the client
        let hashed_token: Vec<u8> = req_client
            .post("https://api.sys32.dev/v3/token")
            .json(&serde_json::json!(token))
            .send()
            .await?
            .json()
            .await?;

        Ok(from_utf8(&hashed_token)?.to_string())
    }

    pub async fn game_info(&self) -> Result<GameInfo, Box<dyn std::error::Error>> {
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
