use crate::{
    domain::{World, WorldDirectory, WorldStatus},
    game::ai::{AiContext, AiRuntime},
};
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tokio::{task::JoinSet, time::Instant};
use uuid::Uuid;

const TICK_INTERVAL: Duration = Duration::from_secs(1);

struct WorldTick {
    snapshot: Arc<World>,
    pending: VecDeque<Uuid>,
    running: usize,
    next_tick: Instant,
}

pub async fn run(
    worlds: Arc<WorldDirectory>,
    runtime: Arc<AiRuntime>,
    elements: Arc<crate::domain::ElementRegistry>,
    instance_storage: Arc<PathBuf>,
    model_storage_lock: Arc<tokio::sync::Mutex<()>>,
) {
    tracing::info!(target: "game", "game loop started");
    let mut ticks: HashMap<Uuid, WorldTick> = HashMap::new();
    let mut ready = VecDeque::new();
    let mut jobs = JoinSet::new();
    let mut job_worlds = HashMap::new();
    let mut poll = tokio::time::interval(Duration::from_millis(50));
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = poll.tick() => {},
            Some(result) = jobs.join_next_with_id(), if !jobs.is_empty() => {
                let task_id = match result {
                    Ok((task_id, ())) => task_id,
                    Err(error) => {
                        tracing::warn!(target: "game", %error, "AI scheduling task failed");
                        error.id()
                    }
                };
                if let Some(id) = job_worlds.remove(&task_id) {
                    if let Some(tick) = ticks.get_mut(&id) { tick.running -= 1; }
                }
            }
        }
        if !runtime.config.enabled {
            continue;
        }

        // Each world owns its tick barrier; a slow world cannot delay another
        // world's next tick. Pending work stays as IDs rather than spawned tasks.
        for metadata in worlds.active_worlds() {
            let id = metadata.id;
            if ticks.get(&id).is_some_and(|tick| {
                tick.running > 0 || !tick.pending.is_empty() || Instant::now() < tick.next_tick
            }) {
                continue;
            }
            let Some(snapshot) = metadata.world() else {
                continue;
            };
            let pending = snapshot
                .organisms
                .values()
                .filter(|organism| {
                    organism
                        .component_states
                        .values()
                        .any(|state| state.energy_mj > 0)
                })
                .map(|organism| organism.id)
                .collect::<VecDeque<_>>();
            let has_work = !pending.is_empty();
            ticks.insert(
                id,
                WorldTick {
                    snapshot,
                    pending,
                    running: 0,
                    next_tick: Instant::now() + TICK_INTERVAL,
                },
            );
            if has_work && !ready.contains(&id) {
                ready.push_back(id);
            }
        }
        ticks.retain(|id, tick| {
            if worlds
                .get(*id)
                .is_none_or(|world| world.status() != WorldStatus::Active)
            {
                tick.pending.clear();
                return tick.running > 0;
            }
            true
        });

        // Round-robin worlds so a large organism list cannot monopolize the pool.
        while jobs.len() < runtime.config.workers {
            let Some(id) = ready.pop_front() else {
                break;
            };
            let Some(tick) = ticks.get_mut(&id) else {
                continue;
            };
            let Some(metadata) = worlds
                .get(id)
                .filter(|world| world.status() == WorldStatus::Active)
            else {
                continue;
            };
            let Some(organism_id) = tick.pending.pop_front() else {
                continue;
            };
            if !tick.pending.is_empty() {
                ready.push_back(id);
            }
            tick.running += 1;
            let snapshot = tick.snapshot.clone();
            let runtime = runtime.clone();
            let worlds = worlds.clone();
            let elements = elements.clone();
            let source = instance_storage.join(organism_id.to_string());
            let storage_lock = model_storage_lock.clone();
            let handle = jobs.spawn(async move {
                let organism = &snapshot.organisms[&organism_id];
                let expected_calls = organism.ai.calls;
                let owner_user_id = organism.owner_user_id;
                let context = AiContext::new(&metadata, snapshot.clone(), organism, elements);
                match runtime.run_scheduled(&source, storage_lock, context).await {
                    Ok(output) => {
                        // Snapshot persistence is blocking; keep it off Tokio's executor.
                        match tokio::task::spawn_blocking(move || worlds.commit_ai(id, organism_id, owner_user_id, expected_calls, output.report, output.memory)).await {
                            Ok(Ok(_)) => {},
                            result => tracing::warn!(target: "game", ?result, %organism_id, "failed to commit tick AI result"),
                        }
                    }
                    Err(error) => tracing::warn!(target: "game", %organism_id, %error, "tick AI execution failed"),
                }
            });
            job_worlds.insert(handle.id(), id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::AiConfig,
        domain::{BiologicalOrganism, OrganismProgramId, organism::OrganismInstance},
    };

    #[tokio::test]
    async fn slow_world_does_not_block_fast_world() {
        let root = std::env::temp_dir().join(format!("life-runner-{}", Uuid::new_v4()));
        let worlds = Arc::new(WorldDirectory::new(Arc::new(
            crate::domain::create_default_terrain_registry().unwrap(),
        )));
        let a = worlds.create_player_world(7, "first", "first").unwrap();
        let b = worlds.create_player_world(8, "second", "second").unwrap();
        let (slow, fast) = if a.id < b.id { (a, b) } else { (b, a) };
        let registry = crate::domain::create_default_component_registry().unwrap();
        let mut fast_id = Uuid::nil();
        let mut slow_ids = Vec::new();
        for (metadata, count, code) in [
            (
                &slow,
                10,
                "import time\ndef main(game):\n    time.sleep(10)\n",
            ),
            (&fast, 1, "def main(game):\n    game.log('fast')\n"),
        ] {
            for _ in 0..count {
                let mut model = BiologicalOrganism::new(OrganismProgramId::new());
                model.add_component("small_body_1", &registry).unwrap();
                let organism = OrganismInstance::from_model(
                    model.id,
                    metadata.owner_user_id,
                    "test".into(),
                    model,
                    &registry,
                )
                .unwrap();
                let folder = root.join(organism.id.to_string());
                std::fs::create_dir_all(&folder).unwrap();
                std::fs::write(folder.join("main.py"), code).unwrap();
                if metadata.id == fast.id {
                    fast_id = organism.id;
                } else {
                    slow_ids.push(organism.id);
                }
                worlds.add_organism(metadata.id, organism).unwrap();
            }
        }
        let runtime = Arc::new(
            AiRuntime::new(AiConfig {
                enabled: true,
                workers: 2,
                timeout_ms: 4000,
                ..Default::default()
            })
            .unwrap(),
        );
        runtime.warm_up().await.unwrap();
        let task = tokio::spawn(run(
            worlds.clone(),
            runtime.clone(),
            Arc::new(crate::domain::create_default_element_registry().unwrap()),
            Arc::new(root.clone()),
            Arc::new(tokio::sync::Mutex::new(())),
        ));
        let outcome = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if fast.world().unwrap().organisms[&fast_id].ai.calls > 0 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        task.abort();
        let _ = task.await;
        runtime.shutdown().await;
        assert!(
            outcome.is_ok(),
            "fast world must complete before any slow script times out"
        );
        let snapshot = slow.world().unwrap();
        assert!(
            slow_ids
                .iter()
                .all(|id| snapshot.organisms[id].ai.calls == 0)
        );
        assert_eq!(
            fast.world().unwrap().organisms[&fast_id]
                .ai
                .last_run
                .as_ref()
                .unwrap()
                .logs,
            "fast\n"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
