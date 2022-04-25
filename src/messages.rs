use std::time::Duration;

use serde_json::{json, Value};

use crate::{
    player::Account,
    utils::{Error, Vec3},
};

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

    pub fn enter() -> Value {
        json!([
            "en",
            [
                0,
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
    ) -> Result<Value, Error> {
        let rotation = if let Some(rotation) = rotation {
            json!([0, (rotation * -1000.0).round() as i32])
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
            0,
            num_tick,
            dt.to_string(),
            2,
            rotation,
            state
        ]))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PlayerState {
    pub is_dead: bool,
    pub tick: Option<u32>,
    pub position: Option<Vec3>,
}

pub struct MessageParser;

impl MessageParser {
    pub fn io_init(msg: &[Value]) -> Result<String, Error> {
        Ok(msg
            .first()
            .ok_or("Wrong Message Type")?
            .as_str()
            .ok_or("Wrong Message Type")?
            .to_owned())
    }

    pub fn spawn_position(msg: &[Value], id: &str) -> Result<Option<Vec3>, Error> {
        let positions = msg
            .first()
            .ok_or("Wrong Message Type")?
            .as_array()
            .ok_or("Wrong Message Type")?;

        let id_index = positions.iter().position(|p| {
            if let Some(p) = p.as_str() {
                p == id
            } else {
                false
            }
        });

        if let Some(id_index) = id_index {
            Ok(Some(Vec3 {
                x: positions
                    .get(id_index + 2)
                    .ok_or("Wrong Message Type")?
                    .as_f64()
                    .ok_or("Position x has wrong type")? as f32,
                y: positions
                    .get(id_index + 3)
                    .ok_or("Wrong Message Type")?
                    .as_f64()
                    .ok_or("Position y has wrong type")? as f32,
                z: positions
                    .get(id_index + 4)
                    .ok_or("Wrong Message Type")?
                    .as_f64()
                    .ok_or("Position z has wrong type")? as f32,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn player_state(msg: &[Value]) -> Result<PlayerState, Error> {
        let first = msg.first().ok_or("Wrong Message Type")?;

        if let Some(first) = first.as_i64() {
            if first == 0 {
                Ok(PlayerState {
                    is_dead: true,
                    tick: None,
                    position: None,
                })
            } else {
                Err("Wrong Message Type".into())
            }
        } else if let Some(first) = first.as_array() {
            Ok(PlayerState {
                is_dead: false,
                tick: Some(
                    first
                        .get(0)
                        .ok_or("Wrong Message Type")?
                        .as_i64()
                        .ok_or("Tick has wrong type")? as u32,
                ),
                position: Some(Vec3 {
                    x: first
                        .get(2)
                        .ok_or("Wrong Message Type")?
                        .as_f64()
                        .ok_or("Position x has wrong type")? as f32,
                    y: first
                        .get(3)
                        .ok_or("Wrong Message Type")?
                        .as_f64()
                        .ok_or("Position y has wrong type")? as f32,
                    z: first
                        .get(4)
                        .ok_or("Wrong Message Type")?
                        .as_f64()
                        .ok_or("Position z has wrong type")? as f32,
                }),
            })
        } else {
            Err("Wrong Message Type".into())
        }
    }

    pub fn error(msg: &[Value]) -> String {
        msg.first()
            .unwrap_or(&Value::String(String::from("")))
            .as_str()
            .unwrap_or("")
            .to_owned()
    }
}
