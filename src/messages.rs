use std::time::Duration;

use serde_json::{json, Value};

use crate::player::Account;

pub struct MessageBuilder;

impl MessageBuilder {
    pub fn pong() -> Value {
        json!(["po"])
    }

    pub fn load() -> Value {
        json!(["load", ()])
    }

    pub fn login(account: &Account) -> Value {
        json!(["a", 1, [account.username, account.password, ()], ()])
    }

    pub fn enter(class: u16) -> Value {
        json!([
            "en",
            [
                class,
                2482,
                [-1, -1],
                -1,
                -1,
                2,
                0,
                0,
                1,
                -1,
                -1,
                1,
                0,
                -1,
                -1,
                -1,
                -1,
                -1,
                -1,
                0,
                -1,
                -1,
                1,
                1,
                1,
                1,
                -1
            ],
            16,
            18,
            false
        ])
    }

    pub fn init_tick() -> Value {
        json!(["q", 0, 0, "3000", 2, [0, 0], { "0-4": -1, "0-5": 0, "0-6": 0, "0-7": 0, "0-8": 0, "0-9": 0, "0-10": 0, "0-11": 0, "0-12": 0, "0-13": 0, "0-14": 0 }])
    }

    pub fn tick(
        num_tick: u32,
        tick_interval: &Duration,
        rotation: Option<f32>,
        state_str: Option<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Sync + Send>> {
        let rotation = if let Some(rotation) = rotation {
            json!([0 as i32, (rotation * -1000.0).round() as i32])
        } else {
            json!(())
        };

        let state = if let Some(state_str) = state_str {
            serde_json::from_str(&state_str)?
        } else {
            json!(())
        };

        let dt = ((tick_interval.as_micros() as f32 / 10.0).round() as i32).min(3333);
        Ok(json!([
            "q",
            0 as i32,
            num_tick,
            dt.to_string(),
            2 as i32,
            rotation,
            state
        ]))
    }
}

pub struct MessageParser;

impl MessageParser {
    pub fn io_init(msg: &Vec<Value>) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
        Ok(msg
            .first()
            .ok_or("Wrong Message Type")?
            .as_str()
            .ok_or("Wrong Message Type")?
            .to_owned())
    }

    pub fn spawn_position(
        msg: &Vec<Value>,
        id: &str,
    ) -> Result<(f32, f32), Box<dyn std::error::Error + Sync + Send>> {
        let positions = msg
            .first()
            .ok_or("Wrong Message Type")?
            .as_array()
            .ok_or("Wrong Message Type")?;

        let id_index = positions
            .iter()
            .position(|p| {
                if let Some(p) = p.as_str() {
                    p == id
                } else {
                    false
                }
            })
            .ok_or("Could not find id in spawn message")?;

        Ok((
            positions
                .get(id_index + 2)
                .ok_or("Wrong Message Type")?
                .as_f64()
                .ok_or("Position has wrong type")? as f32,
            positions
                .get(id_index + 4)
                .ok_or("Wrong Message Type")?
                .as_f64()
                .ok_or("Position has wrong type")? as f32,
        ))
    }

    pub fn player_update(
        msg: &Vec<Value>,
    ) -> Result<(bool, Option<(f32, f32)>), Box<dyn std::error::Error + Sync + Send>> {
        let first = msg.first().ok_or("Wrong Message Type")?;

        if let Some(first) = first.as_i64() {
            if first == 0 {
                Ok((true, None))
            } else {
                Err("Wrong Message Type".into())
            }
        } else if let Some(first) = first.as_array() {
            Ok((
                false,
                Some((
                    first
                        .get(2)
                        .ok_or("Wrong Message Type")?
                        .as_f64()
                        .ok_or("Position has wrong type")? as f32,
                    first
                        .get(4)
                        .ok_or("Wrong Message Type")?
                        .as_f64()
                        .ok_or("Position has wrong type")? as f32,
                )),
            ))
        } else {
            Err("Wrong Message Type".into())
        }
    }
}
