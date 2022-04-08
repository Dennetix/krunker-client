use std::time::Duration;

use krunker_client::{player::PlayerBuilder, Client};
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() {
    // logging
    tracing::subscriber::set_global_default(
        FmtSubscriber::builder()
            .with_max_level(Level::INFO)
            .finish(),
    )
    .expect("Failed to set default subscriber");

    let client = Client::new().await.unwrap();

    loop {
        let games = {
            let client_lock = client.lock().await;
            let maps = client_lock.available_maps();
            client_lock
                .games()
                .await
                .unwrap()
                .into_iter()
                .filter(|g| {
                    g.players == 0
                        && !g.custom
                        && maps.contains(&g.map)
                        && g.mode == 0
                        && g.region == "de-fra"
                })
                .collect::<Vec<_>>()
        };

        let game = games.get(0).unwrap();

        info!("{}", game.id);

        let player = PlayerBuilder::new(client.clone())
            .connect(game)
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_secs(20)).await;

        {
            let mut player_lock = player.lock().await;
            for spawn in player_lock.map().unwrap().spawns() {
                if let Err(err) = player_lock.walk_to(&spawn).await {
                    error!("{:?}", err);
                    break;
                }
            }
            player_lock.disconnect().await.unwrap();
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
