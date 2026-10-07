use crate::domain::WorldDirectory;
use std::sync::Arc;
use std::time::Duration;

/// 临时游戏循环。
///
/// 后续可以在每次循环中处理游戏命令、推进 Tick 并发布游戏事件。
pub async fn run(worlds: Arc<WorldDirectory>) {
    tracing::info!(target: "game", "game loop started");

    loop {
        let active_worlds = worlds.active_worlds();
        tracing::debug!(target: "game", active_world_count = active_worlds.len(), "active worlds tick");

        // 后续在这里推进每个 active_world 的生物、物质和环境状态。
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}
