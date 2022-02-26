use krunker_client::Client;

#[tokio::main]
async fn main() {
    let games = Client::new().await.unwrap().games().await.unwrap();
    let game = games.get(0).unwrap();

    println!("{:?}", game.generate_uri().await)
}
