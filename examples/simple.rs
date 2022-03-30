use std::{f32::consts::PI, time::Duration};

use krunker_client::{player::PlayerBuilder, Client, Game};

#[tokio::main]
async fn main() {
    let client = Client::new().await.unwrap();
    //    let games: Vec<Game> = client
    //         .games()
    //         .await
    //         .unwrap()
    //         .into_iter()
    //         .filter(|g| {
    //             g.players == 0
    //                 && !g.custom
    //                 && g.map == "Industry"
    //                 && g.mode == "0"
    //                 && g.region == "de-fra"
    //         })
    //         .collect();

    //     let game = games.get(0).unwrap();

    //     println!("{}", game.id);

    //     let players = vec![PlayerBuilder::new(&client).connect(&game).await.unwrap()];

    //     tokio::time::sleep(Duration::from_secs(2)).await;
    //     for player in players.iter() {
    //         let mut lock = player.lock().await;
    //         if lock.in_game() {
    //             lock.rotate(PI);
    //         }
    //     }

    //     tokio::time::sleep(Duration::from_secs(1)).await;
    //     for player in players.iter() {
    //         let mut lock = player.lock().await;
    //         if lock.in_game() {
    //             lock.walk(true).await.unwrap();
    //         }
    //     }

    //     tokio::time::sleep(Duration::from_secs_f32(1.25)).await;

    //     let mut state = true;
    //     loop {
    //         tokio::time::sleep(Duration::from_secs(1)).await;
    //         for player in players.iter() {
    //             let mut lock = player.lock().await;
    //             if lock.in_game() {
    //                 lock.rotate(PI);
    //                 lock.shoot(state).await.unwrap();
    //                 state = !state;
    //             }
    //         }
    //     }
}
