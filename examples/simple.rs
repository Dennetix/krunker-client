use std::time::Duration;

use krunker_client::{player::PlayerBuilder, Client};

#[tokio::main]
async fn main() {
    let client = Client::new().await.unwrap();
    let game_info = client
        .games()
        .await
        .unwrap()
        .get(0)
        .unwrap()
        .game_info()
        .await
        .unwrap();

    PlayerBuilder::new(&client)
        .connect(&game_info)
        .await
        .unwrap();

    loop {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
