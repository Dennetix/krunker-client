use std::{f32::consts::PI, time::Duration};

use krunker_client::{player::PlayerBuilder, Client, Game};

#[tokio::main]
async fn main() {
    let client = Client::new().await.unwrap();
    let games: Vec<Game> = client
        .games()
        .await
        .unwrap()
        .into_iter()
        .filter(|g| g.players == 0 && !g.custom && g.map == "Kanji")
        .collect();

    let game = games.get(1).unwrap();

    println!("{}", game.id);

    let players = vec![PlayerBuilder::new(&client).connect(&game).await.unwrap()];

    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        for player in players.iter() {
            let mut lock = player.lock().await;
            if lock.in_game() {
                lock.rotate(PI).await.unwrap();
                lock.walk(true).await.unwrap();
            }
        }
        tokio::time::sleep(Duration::from_secs(4)).await;
        for player in players.iter() {
            let mut lock = player.lock().await;
            if lock.in_game() {
                lock.walk(false).await.unwrap();
            }
        }
    }
}
