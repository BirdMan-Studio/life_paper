#[tokio::main]
async fn main() -> anyhow::Result<()> {
    server::startup::run(false).await
}
