use krunker_client::{player::Player, Client};

#[tokio::main]
async fn main() {
    let client = Client::new().await.unwrap();
    let games = client.games().await.unwrap();
    let game = games.get(0).unwrap();

    Player::new(&client)
        .join(&game.game_info().await.unwrap())
        .await
        .unwrap();
}
