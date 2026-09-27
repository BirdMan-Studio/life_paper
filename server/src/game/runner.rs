use std::time::Duration;

/// 临时游戏循环。
///
/// 后续可以在每次循环中处理游戏命令、推进 Tick 并发布游戏事件。
pub async fn run() {
    tracing::info!(target: "game", "game loop started");

    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}
