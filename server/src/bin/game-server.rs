#[tokio::main]
async fn main() -> anyhow::Result<()> {
    server::startup::run_game().await
}
