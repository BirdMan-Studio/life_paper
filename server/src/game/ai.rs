use crate::{
    config::AiConfig,
    domain::{
        MapPosition, World, WorldInstance,
        organism::{AiRunMetrics, AiRunReport, OrganismInstance, program_storage},
    },
};
use serde_json::{Map, Value, json};
use std::{
    collections::{HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Mutex as StdMutex},
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, Semaphore};

pub struct AiRuntime {
    pub config: AiConfig,
    gate: Semaphore,
    idle: StdMutex<Vec<super::ai_worker::Worker>>,
    snapshots: Arc<StdMutex<VecDeque<CachedSnapshot>>>,
    running: Arc<StdMutex<HashSet<PathBuf>>>,
}

pub struct AiContext {
    pub payload: Value,
    world: Arc<World>,
    organism: OrganismInstance,
    full_visibility: bool,
    elements: Arc<crate::domain::ElementRegistry>,
}

impl AiContext {
    pub fn new(
        metadata: &WorldInstance,
        world: Arc<World>,
        organism: &OrganismInstance,
        elements: Arc<crate::domain::ElementRegistry>,
    ) -> Self {
        let full_visibility = metadata.is_tutorial() || world.ai_full_visibility;
        let mut me = serde_json::to_value(organism).expect("serializable organism");
        me.as_object_mut().unwrap().remove("ai");
        me["rotation"] = json!(f64::from(organism.rotation_mdeg) / 1000.0);
        let components: Map<String, Value> = organism
            .composition
            .components
            .iter()
            .map(|(id, part)| {
                let mut value = serde_json::to_value(&organism.component_states[id])
                    .expect("serializable component state");
                value["id"] = json!(id);
                value["component"] = json!(part.component);
                value["placement"] = json!(part.placement);
                value["api"] = json!(
                    organism.component_states[id]
                        .properties
                        .api
                        .clone()
                        .unwrap_or_else(|| {
                            crate::domain::organism_component::builtin_component_api(
                                part.component.as_str(),
                            )
                        })
                );
                (id.to_string(), value)
            })
            .collect();
        me["components"] = json!(components);
        let payload = json!({
            "me": me,
            "world": {
                "id": metadata.id, "name": metadata.name, "level_id": metadata.level_id,
                "mode": if metadata.is_tutorial() { "tutorial" } else if full_visibility { "simple" } else { "normal" },
                "capabilities": { "full_map": full_visibility, "full_organisms": full_visibility }
            },
            "time": { "step": organism.ai.calls.saturating_add(1) },
            "tutorial": if metadata.is_tutorial() { json!({ "level_id": metadata.level_id }) } else { Value::Null },
            "memory": organism.ai.memory,
        });
        let mut context = Self {
            payload,
            world,
            organism: organism.clone(),
            full_visibility,
            elements,
        };
        let tile = context.query("get_tile", &json!(organism.position));
        context.payload["world"]["_tiles"] = json!([tile["data"]]);
        context
    }

    pub fn component_command(
        &self,
        params: &Value,
    ) -> Result<crate::domain::organism_component::ComponentCommand, Value> {
        use crate::domain::organism_component::{
            ComponentAction, ComponentCommand, builtin_component_api,
        };
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Request {
            component_id: uuid::Uuid,
            method: String,
            rate_mg_per_tick: Option<f64>,
            rate_ml_per_tick: Option<f64>,
            material: Option<String>,
            angle_deg: Option<f64>,
        }
        let error =
            |code: &str, message: &str| json!({"error": {"code": code, "message": message}});
        let request: Request = serde_json::from_value(params.clone())
            .map_err(|_| error("BAD_REQUEST", "invalid component command parameters"))?;
        let part = self
            .organism
            .composition
            .components
            .get(&request.component_id)
            .ok_or_else(|| error("FORBIDDEN", "component does not belong to this organism"))?;
        let state = &self.organism.component_states[&request.component_id];
        let api = state
            .properties
            .api
            .clone()
            .unwrap_or_else(|| builtin_component_api(part.component.as_str()));
        let spec = match request.method.as_str() {
            "jet" => api.jet.as_ref(),
            "absorb_air" | "absorb_matter" => api.absorption.as_ref(),
            _ => None,
        }
        .ok_or_else(|| {
            error(
                "BAD_REQUEST",
                "this component does not support the requested method",
            )
        })?;
        let is_air = request.method == "absorb_air";
        let rate_mg_per_tick = if is_air {
            if request.rate_mg_per_tick.is_some() {
                return Err(error("BAD_REQUEST", "absorb_air requires mL/tick"));
            }
            request.rate_ml_per_tick
        } else {
            if request.rate_ml_per_tick.is_some() {
                return Err(error("BAD_REQUEST", "this method requires mg/tick"));
            }
            request.rate_mg_per_tick
        }
        .ok_or_else(|| error("BAD_REQUEST", "missing rate"))?;
        let validation = if is_air {
            if rate_mg_per_tick.is_finite() && (0.0..=5.0).contains(&rate_mg_per_tick) {
                Ok(())
            } else {
                Err("air rate must be between 0 and 5 mL/tick")
            }
        } else {
            spec.validate(rate_mg_per_tick)
        };
        validation.map_err(|message| error("BAD_REQUEST", message))?;
        let angle_deg = request.angle_deg.unwrap_or(0.0);
        if !angle_deg.is_finite()
            || !(-5.0..=5.0).contains(&angle_deg)
            || (request.method != "jet" && request.angle_deg.is_some())
        {
            return Err(error(
                "BAD_REQUEST",
                "jet angle must be between -5 and 5 degrees",
            ));
        }
        let action = match request.method.as_str() {
            "jet" => {
                let material = request
                    .material
                    .filter(|id| self.elements.contains(id))
                    .ok_or_else(|| {
                        error("BAD_REQUEST", "material must be a registered substance ID")
                    })?;
                ComponentAction::Jet {
                    material,
                    rate_mg_per_tick,
                    angle_deg,
                }
            }
            _ if request.material.is_some() => {
                return Err(error(
                    "BAD_REQUEST",
                    "absorption does not accept a material parameter",
                ));
            }
            "absorb_air" => ComponentAction::AbsorbAir {
                rate_ml_per_tick: rate_mg_per_tick,
            },
            _ => ComponentAction::AbsorbMatter { rate_mg_per_tick },
        };
        Ok(ComponentCommand {
            component_id: request.component_id,
            action,
        })
    }

    pub fn query(&self, method: &str, params: &Value) -> Value {
        let bad = |message: &str| json!({"error": {"code": "BAD_REQUEST", "message": message}});
        match method {
            "get_tiles" => {
                let Some(points) = params["points"]
                    .as_array()
                    .filter(|points| (1..=256).contains(&points.len()))
                else {
                    return bad("get_tiles requires between 1 and 256 points");
                };
                let mut tiles = Vec::with_capacity(points.len());
                for point in points {
                    let tile = self.query("get_tile", point);
                    if tile.get("error").is_some() {
                        return tile;
                    }
                    tiles.push(tile["data"].clone());
                }
                json!({"data": tiles})
            }
            "get_tile" => {
                let Some(x) = params["x"].as_i64().and_then(|x| i32::try_from(x).ok()) else {
                    return bad("x must be a signed 32-bit integer");
                };
                let Some(y) = params["y"].as_i64().and_then(|y| i32::try_from(y).ok()) else {
                    return bad("y must be a signed 32-bit integer");
                };
                let position = MapPosition { x, y };
                if !self.full_visibility && position != self.organism.position {
                    return json!({"error": {"code": "FORBIDDEN", "message": "this tile is outside your perception"}});
                }
                let properties = self.world.ground_properties(position);
                json!({"data": {"x": x, "y": y, "terrain": self.world.terrain_at(position),
                    "height": properties.height, "moisture": properties.moisture,
                    "nutrients": properties.nutrients, "temperature": properties.temperature,
                    "matter_mg": self.world.matter.at(&self.world, position)}})
            }
            "get_organisms" => {
                let Some(offset) = params["offset"]
                    .as_u64()
                    .and_then(|x| usize::try_from(x).ok())
                else {
                    return bad("invalid offset");
                };
                let Some(limit) = params["limit"].as_u64().filter(|x| (1..=100).contains(x)) else {
                    return bad("limit must be between 1 and 100");
                };
                let organisms: Vec<_> = self.world.organisms.values()
                    .filter(|other| self.full_visibility || other.id == self.organism.id)
                    .skip(offset).take(limit as usize)
                    .map(|other| json!({"id": other.id, "name": other.name, "position": other.position, "rotation": f64::from(other.rotation_mdeg) / 1000.0}))
                    .collect();
                json!({"data": organisms})
            }
            _ => bad("unknown game.world method"),
        }
    }
}

struct ProgramSnapshot(PathBuf);
impl Drop for ProgramSnapshot {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.0) {
            tracing::warn!(%error, "failed to remove AI program snapshot");
        }
    }
}

struct CachedSnapshot {
    source: PathBuf,
    version: String,
    snapshot: Arc<ProgramSnapshot>,
}

#[derive(Debug)]
pub struct AiBusy;
impl std::fmt::Display for AiBusy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AI workers are busy or this instance is already running")
    }
}
impl std::error::Error for AiBusy {}

struct RunningProgram {
    source: PathBuf,
    running: Arc<StdMutex<HashSet<PathBuf>>>,
}
impl Drop for RunningProgram {
    fn drop(&mut self) {
        self.running.lock().unwrap().remove(&self.source);
    }
}

pub struct AiOutput {
    pub report: AiRunReport,
    pub memory: Option<Map<String, Value>>,
}

impl AiRuntime {
    pub fn new(config: AiConfig) -> anyhow::Result<Self> {
        anyhow::ensure!(
            !config.python.trim().is_empty(),
            "ai.python cannot be empty"
        );
        anyhow::ensure!(
            (50..=60_000).contains(&config.timeout_ms),
            "ai.timeout_ms must be 50..60000"
        );
        anyhow::ensure!(
            (1..=4096).contains(&config.max_queries),
            "ai.max_queries must be 1..4096"
        );
        anyhow::ensure!(
            (1..=16).contains(&config.workers),
            "ai.workers must be 1..16"
        );
        anyhow::ensure!(
            (1..=64).contains(&config.cache_entries),
            "ai.cache_entries must be 1..64"
        );
        Ok(Self {
            gate: Semaphore::new(config.workers),
            config,
            idle: StdMutex::new(Vec::new()),
            snapshots: Arc::new(StdMutex::new(VecDeque::new())),
            running: Arc::new(StdMutex::new(HashSet::new())),
        })
    }

    pub async fn warm_up(&self) -> anyhow::Result<()> {
        if !self.config.enabled {
            return Ok(());
        }
        while self.idle.lock().unwrap().len() < self.config.workers {
            let worker = super::ai_worker::Worker::start(&self.config).await?;
            self.idle.lock().unwrap().push(worker);
        }
        Ok(())
    }

    pub async fn shutdown(&self) {
        let workers = std::mem::take(&mut *self.idle.lock().unwrap());
        for mut worker in workers {
            worker.stop().await;
        }
    }

    async fn snapshot(
        &self,
        source: &Path,
        storage_lock: Arc<Mutex<()>>,
    ) -> anyhow::Result<(Arc<ProgramSnapshot>, String, bool)> {
        let guard = storage_lock.lock_owned().await;
        let source = source.to_path_buf();
        let cache = self.snapshots.clone();
        let limit = self.config.cache_entries;
        Ok(tokio::task::spawn_blocking(move || {
            let _guard = guard;
            program_storage::ensure_entry(&source)?;
            let version = program_storage::fingerprint(&source)?;
            let mut cache = cache.lock().unwrap();
            if let Some(index) = cache
                .iter()
                .position(|entry| entry.source == source && entry.version == version)
            {
                let entry = cache.remove(index).unwrap();
                let result = (entry.snapshot.clone(), version, true);
                cache.push_back(entry);
                return Ok::<_, std::io::Error>(result);
            }
            let path = std::env::temp_dir().join(format!("life-paper-ai-{}", uuid::Uuid::new_v4()));
            program_storage::clone_program(&source, &path)?;
            let snapshot = Arc::new(ProgramSnapshot(path));
            cache.retain(|entry| entry.source != source);
            cache.push_back(CachedSnapshot {
                source,
                version: version.clone(),
                snapshot: snapshot.clone(),
            });
            while cache.len() > limit {
                cache.pop_front();
            }
            Ok((snapshot, version, false))
        })
        .await??)
    }

    #[cfg(test)]
    async fn run(
        &self,
        source: &Path,
        storage_lock: Arc<Mutex<()>>,
        context: AiContext,
    ) -> anyhow::Result<AiOutput> {
        self.run_with_capacity(source, storage_lock, context, false)
            .await
    }

    pub async fn run_scheduled(
        &self,
        source: &Path,
        storage_lock: Arc<Mutex<()>>,
        context: AiContext,
    ) -> anyhow::Result<AiOutput> {
        self.run_with_capacity(source, storage_lock, context, true)
            .await
    }

    async fn run_with_capacity(
        &self,
        source: &Path,
        storage_lock: Arc<Mutex<()>>,
        context: AiContext,
        wait: bool,
    ) -> anyhow::Result<AiOutput> {
        anyhow::ensure!(self.config.enabled, "AI execution is disabled");
        let total = Instant::now();
        let _permit = if wait {
            self.gate.acquire().await.map_err(|_| AiBusy)?
        } else {
            self.gate.try_acquire().map_err(|_| AiBusy)?
        };
        let source = source.to_path_buf();
        if !self.running.lock().unwrap().insert(source.clone()) {
            return Err(AiBusy.into());
        }
        let _running = RunningProgram {
            source: source.clone(),
            running: self.running.clone(),
        };
        let snapshot_started = Instant::now();
        let (snapshot, version, hit) = self.snapshot(&source, storage_lock).await?;
        let mut metrics = AiRunMetrics {
            code_version: version.clone(),
            snapshot_cache_hit: hit,
            snapshot_us: snapshot_started.elapsed().as_micros() as u64,
            ..Default::default()
        };
        let mut idle = self.idle.lock().unwrap().pop();
        if idle.as_mut().is_some_and(|worker| !worker.is_alive()) {
            idle = None;
        }
        metrics.worker_reused = idle.is_some();
        let startup = Instant::now();
        let mut worker = match idle {
            Some(worker) => worker,
            None => super::ai_worker::Worker::start(&self.config).await?,
        };
        if !metrics.worker_reused {
            metrics.startup_us = startup.elapsed().as_micros() as u64;
        }
        metrics.worker_pid = worker.pid();
        let exchange = Instant::now();
        let result = tokio::time::timeout(
            Duration::from_millis(self.config.timeout_ms),
            worker.execute(
                &context,
                &snapshot.0,
                &version,
                self.config.max_queries,
                &mut metrics,
            ),
        )
        .await;
        metrics.roundtrip_us = exchange.elapsed().as_micros() as u64;
        let mut output = match result {
            Ok(Ok((output, reusable))) => {
                if reusable {
                    self.idle.lock().unwrap().push(worker);
                } else {
                    worker.stop().await;
                }
                output
            }
            failure => {
                worker.stop().await;
                let error = match failure {
                    Ok(Err(error)) => error.to_string(),
                    _ => "AI execution timed out".into(),
                };
                AiOutput {
                    report: AiRunReport {
                        ok: false,
                        logs: String::new(),
                        error: Some(error),
                        duration_ms: 0,
                        metrics: Default::default(),
                        commands: Vec::new(),
                    },
                    memory: None,
                }
            }
        };
        metrics.total_us = total.elapsed().as_micros() as u64;
        output.report.duration_ms = metrics.total_us / 1000;
        output.report.metrics = metrics;
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        BiologicalOrganism, OrganismProgramId, WorldDirectory, create_default_component_registry,
        create_default_terrain_registry,
    };
    use uuid::Uuid;

    struct Fixture {
        root: PathBuf,
        worlds: Arc<WorldDirectory>,
        metadata: Arc<WorldInstance>,
        id: Uuid,
    }
    impl Fixture {
        fn new(tutorial: bool, full: bool) -> Self {
            Self::with_components(tutorial, full, &[])
        }
        fn with_components(tutorial: bool, full: bool, extra: &[&str]) -> Self {
            let root = std::env::temp_dir().join(format!("life-ai-test-{}", Uuid::new_v4()));
            let worlds = Arc::new(
                WorldDirectory::new_with_storage(
                    Arc::new(create_default_terrain_registry().unwrap()),
                    root.join("worlds"),
                )
                .unwrap(),
            );
            let metadata = if tutorial {
                worlds.get_or_create_tutorial(7).unwrap()
            } else {
                worlds
                    .create_player_world_with_ai_visibility(7, "test", "test", full)
                    .unwrap()
            };
            let components = create_default_component_registry().unwrap();
            let mut model = BiologicalOrganism::new(OrganismProgramId::new());
            let body = model.add_component("small_body_1", &components).unwrap();
            for component in extra {
                let part = model.add_component(*component, &components).unwrap();
                model.connect(body, part, &components).unwrap();
            }
            let organism =
                OrganismInstance::from_model(model.id, 7, "测试生物".into(), model, &components)
                    .unwrap();
            let id = organism.id;
            worlds.add_organism(metadata.id, organism).unwrap();
            std::fs::create_dir(root.join("program")).unwrap();
            Self {
                root,
                worlds,
                metadata,
                id,
            }
        }
        fn context(&self) -> AiContext {
            let world = self.worlds.read_world(self.metadata.id, 7).unwrap();
            let organism = world.organisms[&self.id].clone();
            AiContext::new(
                &self.metadata,
                world,
                &organism,
                Arc::new(crate::domain::create_default_element_registry().unwrap()),
            )
        }
        async fn execute(&self, code: &str, timeout_ms: u64, max_queries: usize) -> AiOutput {
            std::fs::write(self.root.join("program/main.py"), code).unwrap();
            let mut runtime = AiRuntime::new(AiConfig {
                enabled: true,
                timeout_ms: 5000,
                max_queries,
                workers: 1,
                python: std::env::var("LIFE_PAPER_TEST_PYTHON").unwrap_or_else(|_| "python".into()),
                ..Default::default()
            })
            .unwrap();
            // Test execution deadlines independently of interpreter startup under load.
            runtime.warm_up().await.unwrap();
            runtime.config.timeout_ms = timeout_ms;
            let output = runtime
                .run(
                    &self.root.join("program"),
                    Arc::new(Mutex::new(())),
                    self.context(),
                )
                .await
                .unwrap();
            runtime.shutdown().await;
            output
        }
        fn commit(&self, expected: u64, output: AiOutput) -> bool {
            self.worlds
                .commit_ai(
                    self.metadata.id,
                    self.id,
                    7,
                    expected,
                    output.report,
                    output.memory,
                )
                .unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[tokio::test]
    async fn python_entry_queries_real_terrain_and_memory_survives_restart() {
        let f = Fixture::new(true, false);
        let code = include_str!("../../python/main.py");
        let output = f.execute(code, 5000, 256).await;
        assert!(output.report.ok, "{:?}", output.report);
        assert!(output.report.logs.contains("测试生物 wall 1"));
        assert_eq!(output.memory.as_ref().unwrap()["calls"], 1);
        assert!(f.commit(0, output));
        let output = f.execute(code, 5000, 256).await;
        assert!(output.report.ok, "{:?}", output.report);
        assert_eq!(output.memory.as_ref().unwrap()["calls"], 2);
        assert!(f.commit(1, output));
        let restored = WorldDirectory::new_with_storage(
            Arc::new(create_default_terrain_registry().unwrap()),
            f.root.join("worlds"),
        )
        .unwrap();
        let organisms = restored.list_organisms(f.metadata.id, 7).unwrap();
        assert_eq!(organisms[0].ai.memory["calls"], 2);
        assert_eq!(organisms[0].ai.calls, 2);
    }

    #[test]
    fn query_permissions_are_checked_by_server_and_other_ai_state_is_private() {
        let f = Fixture::new(false, false);
        let context = f.context();
        assert_eq!(
            context.query("get_tile", &json!({"x": 1, "y": 0}))["error"]["code"],
            "FORBIDDEN"
        );
        assert_eq!(
            context.query("get_tile", &json!({"x": 0, "y": 0}))["data"]["terrain"],
            "wall"
        );
        assert!(
            context
                .query("get_tile", &json!({"x": i64::MAX, "y": 0}))
                .get("error")
                .is_some()
        );
        let others = context.query("get_organisms", &json!({"offset": 0, "limit": 100}));
        assert_eq!(others["data"].as_array().unwrap().len(), 1);
        assert!(others["data"][0].get("ai").is_none());
        assert!(others["data"][0].get("component_states").is_none());
        let full = Fixture::new(false, true);
        assert_eq!(full.context().payload["world"]["mode"], "simple");
        assert!(
            full.context()
                .query("get_tile", &json!({"x": 10, "y": 10}))
                .get("data")
                .is_some()
        );
        let restored = WorldDirectory::new_with_storage(
            Arc::new(create_default_terrain_registry().unwrap()),
            full.root.join("worlds"),
        )
        .unwrap();
        assert!(
            restored
                .read_world(full.metadata.id, 7)
                .unwrap()
                .ai_full_visibility
        );
    }

    #[tokio::test]
    async fn sdk_is_read_only_and_inaccessible_tiles_raise_permission_error() {
        let f = Fixture::new(false, false);
        let output = f.execute("def main(game):\n    try:\n        game.me.position.x = 42\n    except AttributeError:\n        pass\n    else:\n        raise AssertionError('mutable me')\n    try:\n        game.world.get_tile(1, 0)\n    except PermissionError:\n        game.memory['restricted'] = True\n    assert game.tutorial is None\n    assert game.time.step == 1\n    assert len(game.me.components) == 1\n    assert game.world.get_organisms()[0].id == game.me.id\n", 5000, 256).await;
        assert!(output.report.ok, "{:?}", output.report);
        assert_eq!(output.memory.unwrap()["restricted"], true);
    }

    #[tokio::test]
    async fn errors_do_not_commit_partial_memory_and_old_results_cannot_overwrite_newer_state() {
        let f = Fixture::new(true, false);
        assert!(
            f.commit(
                0,
                f.execute("def main(game):\n    game.memory['value'] = 7\n", 5000, 256)
                    .await
            )
        );
        let output = f.execute("def main(game):\n    game.memory['value'] = 99\n    print('before failure')\n    raise ValueError('test error')\n", 5000, 256).await;
        assert!(!output.report.ok);
        assert!(output.report.logs.contains("before failure"));
        assert!(output.report.error.as_ref().unwrap().contains("test error"));
        assert!(output.memory.is_none());
        assert!(f.commit(1, output));
        assert_eq!(f.context().payload["memory"]["value"], 7);
        let stale = f
            .execute(
                "def main(game):\n    game.memory['value'] = 88\n",
                5000,
                256,
            )
            .await;
        assert!(!f.commit(0, stale));
        assert_eq!(f.context().payload["memory"]["value"], 7);
    }

    #[tokio::test]
    async fn stops_infinite_loops_and_rejects_query_budget_overruns() {
        let f = Fixture::new(true, false);
        let output = f
            .execute("def main(game):\n    while True:\n        pass\n", 500, 256)
            .await;
        assert!(!output.report.ok);
        assert_eq!(
            output.report.error.as_deref(),
            Some("AI execution timed out")
        );
        assert!(output.report.duration_ms < 5000);
        let output = f
            .execute(
                "def main(game):\n    for index in range(3):\n        game.world.get_tile(index + 1, 0)\n",
                5000,
                2,
            )
            .await;
        assert!(!output.report.ok);
        assert!(
            output
                .report
                .error
                .unwrap()
                .contains("query budget exceeded")
        );
    }

    #[tokio::test]
    async fn imports_helpers_from_the_instance_and_reloads_after_edits() {
        let f = Fixture::new(true, false);
        std::fs::write(f.root.join("program/brain.py"), "VALUE = 7\n").unwrap();
        let code = "import brain\ndef main(game):\n    game.memory['value'] = brain.VALUE\n";
        let output = f.execute(code, 5000, 256).await;
        assert!(output.report.ok, "{:?}", output.report);
        assert_eq!(output.memory.unwrap()["value"], 7);
        std::fs::write(f.root.join("program/brain.py"), "VALUE = 8\n").unwrap();
        let output = f.execute(code, 5000, 256).await;
        assert!(output.report.ok, "{:?}", output.report);
        assert_eq!(output.memory.unwrap()["value"], 8);
        assert!(!f.root.join("program/__pycache__").exists());
    }

    #[tokio::test]
    async fn validates_entry_memory_and_limits_logs() {
        let f = Fixture::new(true, false);
        for code in [
            "pass\n",
            "def main():\n    pass\n",
            "async def main(game):\n    pass\n",
            "def main(game):\n    game.memory['bad'] = float('nan')\n",
            "def main(game):\n    game.memory['huge'] = 'x' * 20000\n",
        ] {
            let output = f.execute(code, 5000, 256).await;
            assert!(!output.report.ok, "{code}");
            assert!(output.memory.is_none());
        }
        let output = f
            .execute("def main(game):\n    print('x' * 1000000)\n", 5000, 256)
            .await;
        assert!(output.report.ok, "{:?}", output.report);
        assert_eq!(output.report.logs.len(), 16384);
    }

    #[tokio::test]
    async fn failed_save_keeps_ai_memory_and_report_unchanged() {
        let f = Fixture::new(true, false);
        let output = f
            .execute("def main(game):\n    game.memory['value'] = 7\n", 5000, 256)
            .await;
        assert!(output.report.ok);
        let before = f.worlds.read_world(f.metadata.id, 7).unwrap();
        std::fs::remove_dir_all(f.root.join("worlds")).unwrap();
        std::fs::write(f.root.join("worlds"), b"blocked").unwrap();
        assert!(
            f.worlds
                .commit_ai(f.metadata.id, f.id, 7, 0, output.report, output.memory)
                .is_err()
        );
        assert_eq!(f.worlds.read_world(f.metadata.id, 7).unwrap(), before);
    }

    #[tokio::test]
    async fn pooled_calls_cache_code_but_reset_modules_and_invalidate_same_size_edits() {
        let f = Fixture::new(true, false);
        let other = Fixture::new(true, false);
        let runtime = test_runtime(5000, 256, 1);
        let code = "import brain\ncount = 0\ndef main(game):\n    global count\n    count += 1\n    brain.count += 1\n    assert count == brain.count == 1\n    game.memory['value'] = brain.VALUE\n    game.memory['calls'] = game.memory.get('calls', 0) + 1\n";
        for fixture in [&f, &other] {
            std::fs::write(fixture.root.join("program/main.py"), code).unwrap();
            std::fs::write(
                fixture.root.join("program/brain.py"),
                "count = 0\nVALUE = 7\n",
            )
            .unwrap();
        }
        let cold = run_fixture(&runtime, &f).await;
        assert!(cold.report.ok, "{:?}", cold.report);
        let cold_metrics = cold.report.metrics.clone();
        assert!(!cold_metrics.code_cache_hit);
        assert!(f.commit(0, cold));
        let warm = run_fixture(&runtime, &f).await;
        assert!(warm.report.ok, "{:?}", warm.report);
        assert_eq!(warm.memory.as_ref().unwrap()["calls"], 2);
        assert!(warm.report.metrics.snapshot_cache_hit);
        assert!(warm.report.metrics.code_cache_hit);
        assert!(warm.report.metrics.worker_reused);
        assert_eq!(cold_metrics.worker_pid, warm.report.metrics.worker_pid);
        eprintln!(
            "AI cold total={}us startup={}us; warm total={}us snapshot={}us load={}us execution={}us",
            cold_metrics.total_us,
            cold_metrics.startup_us,
            warm.report.metrics.total_us,
            warm.report.metrics.snapshot_us,
            warm.report.metrics.load_us,
            warm.report.metrics.execution_us
        );
        let isolated = run_fixture(&runtime, &other).await;
        assert!(isolated.report.ok, "{:?}", isolated.report);
        assert_eq!(isolated.memory.unwrap()["calls"], 1);
        let evicted = run_fixture(&runtime, &f).await;
        assert!(evicted.report.ok, "{:?}", evicted.report);
        assert!(!evicted.report.metrics.snapshot_cache_hit);
        assert!(!evicted.report.metrics.code_cache_hit);
        std::fs::write(f.root.join("program/brain.py"), "count = 0\nVALUE = 8\n").unwrap();
        let edited = run_fixture(&runtime, &f).await;
        assert!(edited.report.ok, "{:?}", edited.report);
        assert_eq!(edited.memory.unwrap()["value"], 8);
        assert_ne!(
            cold_metrics.code_version,
            edited.report.metrics.code_version
        );
        assert!(!edited.report.metrics.snapshot_cache_hit);
        assert!(!edited.report.metrics.code_cache_hit);
        runtime.shutdown().await;
    }

    fn test_runtime(timeout_ms: u64, max_queries: usize, cache_entries: usize) -> AiRuntime {
        AiRuntime::new(AiConfig {
            enabled: true,
            python: std::env::var("LIFE_PAPER_TEST_PYTHON").unwrap_or_else(|_| "python".into()),
            timeout_ms,
            max_queries,
            cache_entries,
            ..Default::default()
        })
        .unwrap()
    }

    async fn run_fixture(runtime: &AiRuntime, f: &Fixture) -> AiOutput {
        runtime
            .run(
                &f.root.join("program"),
                Arc::new(Mutex::new(())),
                f.context(),
            )
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn batch_queries_preserve_order_cache_duplicates_and_enforce_budget() {
        let f = Fixture::new(true, false);
        let runtime = test_runtime(5000, 2, 8);
        std::fs::write(f.root.join("program/main.py"), "def main(game):\n    tiles = game.world.get_tiles([(2, 0), (0, 0), (1, 0), (2, 0)])\n    assert [tile.x for tile in tiles] == [2, 0, 1, 2]\n    for _ in range(10):\n        assert game.world.get_tile(2, 0).x == 2\n    assert game.world.get_tiles([(1, 0)])[0].x == 1\n").unwrap();
        let output = run_fixture(&runtime, &f).await;
        assert!(output.report.ok, "{:?}", output.report);
        std::fs::write(
            f.root.join("program/main.py"),
            "def main(game):\n    game.world.get_tiles([(1, 0), (2, 0), (3, 0)])\n",
        )
        .unwrap();
        let output = run_fixture(&runtime, &f).await;
        assert!(!output.report.ok);
        assert!(
            output
                .report
                .error
                .unwrap()
                .contains("query budget exceeded")
        );
        let normal = Fixture::new(false, false);
        let context = normal.context();
        let denied = context.query(
            "get_tiles",
            &json!({"points": [{"x": 0, "y": 0}, {"x": 1, "y": 0}]}),
        );
        assert_eq!(denied["error"]["code"], "FORBIDDEN");
        assert!(denied.get("data").is_none());
        runtime.shutdown().await;
    }

    #[tokio::test]
    async fn worker_is_replaced_after_timeout_and_packages_reload() {
        let f = Fixture::new(true, false);
        let runtime = test_runtime(1000, 256, 8);
        std::fs::write(
            f.root.join("program/main.py"),
            "def main(game):\n    while True: pass\n",
        )
        .unwrap();
        let timed_out = run_fixture(&runtime, &f).await;
        assert!(!timed_out.report.ok);
        assert!(timed_out.report.error.unwrap().contains("timed out"));
        std::fs::create_dir(f.root.join("program/pkg")).unwrap();
        std::fs::write(
            f.root.join("program/pkg/__init__.py"),
            "from .helper import VALUE\n",
        )
        .unwrap();
        std::fs::write(f.root.join("program/pkg/helper.py"), "VALUE = 9\n").unwrap();
        std::fs::create_dir(f.root.join("program/namespace")).unwrap();
        std::fs::write(f.root.join("program/namespace/helper.py"), "VALUE = 4\n").unwrap();
        std::fs::write(f.root.join("program/main.py"), "import pkg\nimport namespace.helper\ndef main(game):\n    assert pkg.VALUE == 9\n    assert namespace.helper.VALUE == 4\n").unwrap();
        let output = run_fixture(&runtime, &f).await;
        assert!(output.report.ok, "{:?}", output.report);
        assert!(!output.report.metrics.worker_reused);
        assert_ne!(
            timed_out.report.metrics.worker_pid,
            output.report.metrics.worker_pid
        );
        let output = run_fixture(&runtime, &f).await;
        assert!(output.report.ok, "{:?}", output.report);
        assert!(output.report.metrics.code_cache_hit);
        runtime.shutdown().await;
    }

    #[tokio::test]
    async fn pool_limits_concurrency_and_releases_cancelled_instances() {
        let f = Fixture::new(true, false);
        let other = Fixture::new(true, false);
        let runtime = Arc::new(test_runtime(5000, 256, 8));
        runtime.warm_up().await.unwrap();
        for fixture in [&f, &other] {
            std::fs::write(
                fixture.root.join("program/main.py"),
                "import time\ndef main(game):\n    time.sleep(0.1)\n",
            )
            .unwrap();
        }
        let lock = Arc::new(Mutex::new(()));
        let guard = lock.clone().lock_owned().await;
        let start = |fixture: &Fixture| {
            let runtime = runtime.clone();
            let source = fixture.root.join("program");
            let context = fixture.context();
            let lock = lock.clone();
            tokio::spawn(async move { runtime.run(&source, lock, context).await })
        };
        let first = start(&f);
        tokio::time::timeout(Duration::from_secs(2), async {
            while runtime.running.lock().unwrap().len() != 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let duplicate = runtime
            .run(&f.root.join("program"), lock.clone(), f.context())
            .await
            .err()
            .unwrap();
        assert!(duplicate.is::<AiBusy>());
        let second = start(&other);
        tokio::time::timeout(Duration::from_secs(2), async {
            while runtime.running.lock().unwrap().len() != 2 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let full = runtime
            .run(&f.root.join("program"), lock.clone(), f.context())
            .await
            .err()
            .unwrap();
        assert!(full.is::<AiBusy>());
        first.abort();
        assert!(first.await.err().unwrap().is_cancelled());
        assert!(
            !runtime
                .running
                .lock()
                .unwrap()
                .contains(&f.root.join("program"))
        );
        let replacement = start(&f);
        drop(guard);
        let a = replacement.await.unwrap().unwrap();
        let b = second.await.unwrap().unwrap();
        assert!(a.report.ok, "{:?}", a.report);
        assert!(b.report.ok, "{:?}", b.report);
        assert_ne!(a.report.metrics.worker_pid, b.report.metrics.worker_pid);
        runtime.shutdown().await;
    }

    #[tokio::test]
    async fn component_controls_preserve_requested_rates_and_commit_independent_outputs() {
        use crate::domain::organism_component::ComponentAction;
        let f = Fixture::with_components(
            true,
            false,
            &["jet_organ_1", "absorption_organ_1", "photosensor_organ_1"],
        );
        let code = "def main(game):\n    parts = {part.component: part for part in game.me.components.values()}\n    jet = parts['jet_organ_1']\n    absorber = parts['absorption_organ_1']\n    assert not hasattr(parts['small_body_1'], 'jet')\n    assert jet.jet('water', 31) == 31\n    assert absorber.absorb_air(2.5) == 2.5\n    assert absorber.absorb_matter(13.25) == 13.25\n    assert jet.durability == 50\n    assert game.me.components[jet.id] is jet\n    try:\n        jet.durability = 0\n        raise AssertionError('state must remain read-only')\n    except AttributeError:\n        pass\n    try:\n        parts['photosensor_organ_1'].get_vision()\n        raise AssertionError('vision is not simulated yet')\n    except NotImplementedError:\n        pass\n    game.memory['controlled'] = True\n";
        let output = f.execute(code, 5000, 256).await;
        assert!(output.report.ok, "{:?}", output.report);
        assert_eq!(output.report.commands.len(), 2);
        assert!(
            matches!(&output.report.commands[0].action, ComponentAction::Jet { material, rate_mg_per_tick, angle_deg } if material == "water" && *rate_mg_per_tick == 31.0 && *angle_deg == 0.0)
        );
        assert_eq!(
            output.report.commands[1].action,
            ComponentAction::AbsorbMatter {
                rate_mg_per_tick: 13.25
            }
        );
        let commands = output.report.commands.clone();
        assert!(f.commit(0, output));
        let world = f.worlds.read_world(f.metadata.id, 7).unwrap();
        let organism = &world.organisms[&f.id];
        assert_eq!(organism.ai.pending_commands, commands);
        let motion = organism.motion.as_ref().unwrap();
        assert_eq!(
            motion.velocity_x.hypot(motion.velocity_y),
            0.0,
            "empty water inventory must not generate thrust"
        );
        assert!(
            organism
                .component_states
                .values()
                .all(|state| state.energy_mj <= state.properties.energy_capacity_mj)
        );
        let restored = WorldDirectory::new_with_storage(
            Arc::new(create_default_terrain_registry().unwrap()),
            f.root.join("worlds"),
        )
        .unwrap();
        assert_eq!(
            restored.list_organisms(f.metadata.id, 7).unwrap()[0]
                .ai
                .pending_commands,
            commands
        );
        let failed = f.execute("def main(game):\n    jet = next(part for part in game.me.components.values() if part.component == 'jet_organ_1')\n    jet.jet('water', 200)\n    game.memory['controlled'] = False\n    raise RuntimeError('abort')\n", 5000, 256).await;
        assert!(!failed.report.ok);
        assert!(failed.report.commands.is_empty());
        assert!(f.commit(1, failed));
        let current = f.context();
        assert_eq!(current.organism.ai.pending_commands, commands);
        assert_eq!(current.organism.ai.memory["controlled"], true);
        let output = f.execute("def main(game):\n    pass\n", 5000, 256).await;
        assert!(f.commit(2, output));
        assert!(f.context().organism.ai.pending_commands.is_empty());
    }

    #[tokio::test]
    async fn python_jet_records_deflection_and_rejects_invalid_angles() {
        use crate::domain::organism_component::ComponentAction;
        let f = Fixture::with_components(true, false, &["jet_organ_1"]);
        let output = f.execute("def main(game):\n    jet = next(p for p in game.me.components.values() if p.component == 'jet_organ_1')\n    for angle in [True, float('nan'), '3', 5.1, -5.1]:\n        try:\n            jet.jet('water', 31, angle=angle)\n        except (TypeError, ValueError):\n            pass\n        else:\n            raise AssertionError('invalid angle accepted')\n    assert jet.jet('water', 31, angle=2.6) == 31\n", 5000, 256).await;
        assert!(output.report.ok, "{:?}", output.report);
        assert!(matches!(
            output.report.commands[0].action,
            ComponentAction::Jet { angle_deg: 2.6, .. }
        ));
    }

    #[test]
    fn component_recording_validates_parameters_but_defers_durability_to_the_game_loop() {
        let f = Fixture::with_components(true, false, &["jet_organ_1", "absorption_organ_1"]);
        let mut context = f.context();
        let absorber = context
            .organism
            .composition
            .components
            .values()
            .find(|p| p.component.as_str() == "absorption_organ_1")
            .unwrap()
            .id;
        for rate in [0.0, 0.5, 2.7, 5.0] {
            assert!(context.component_command(&json!({"component_id": absorber, "method": "absorb_air", "rate_ml_per_tick": rate})).is_ok());
        }
        for rate in [-1.0, 5.1] {
            assert!(context.component_command(&json!({"component_id": absorber, "method": "absorb_air", "rate_ml_per_tick": rate})).is_err());
        }
        assert!(context.component_command(&json!({"component_id": absorber, "method": "absorb_air", "rate_mg_per_tick": 2})).is_err());
        let id = context
            .organism
            .composition
            .components
            .values()
            .find(|part| part.component.as_str() == "jet_organ_1")
            .unwrap()
            .id;
        let valid = json!({"component_id": id, "method": "jet", "material": "water", "rate_mg_per_tick": 31});
        assert!(context.component_command(&valid).is_ok());
        for angle in [-5.0, -2.4, 0.0, 5.0] {
            let mut request = valid.clone();
            request["angle_deg"] = json!(angle);
            assert!(context.component_command(&request).is_ok());
        }
        for angle in [-5.1, 5.1] {
            let mut request = valid.clone();
            request["angle_deg"] = json!(angle);
            assert!(context.component_command(&request).is_err());
        }
        for (field, value) in [
            ("component_id", json!(Uuid::new_v4())),
            ("method", json!("absorb_air")),
            ("material", json!("unknown_substance")),
            ("rate_mg_per_tick", json!(-1)),
            ("rate_mg_per_tick", json!(201)),
            ("rate_mg_per_tick", json!(true)),
            ("forged", json!(true)),
        ] {
            let mut request = valid.clone();
            request[field] = value;
            assert!(context.component_command(&request).is_err(), "{request}");
        }
        context
            .organism
            .component_states
            .get_mut(&id)
            .unwrap()
            .durability = 0;
        assert!(context.component_command(&valid).is_ok());
    }

    #[tokio::test]
    async fn latest_component_output_overwrites_the_rate_and_material_including_zero() {
        use crate::domain::organism_component::ComponentAction;
        let f = Fixture::with_components(true, false, &["jet_organ_1"]);
        // Resolve the legacy origin-on-wall spawn before checking zero thrust.
        let initial = f.metadata.world().unwrap();
        assert!(
            f.worlds
                .commit_tick(f.metadata.id, &initial, vec![])
                .unwrap()
        );
        let before = f.context().organism;
        let output = f.execute("def main(game):\n    jet = next(part for part in game.me.components.values() if part.component == 'jet_organ_1')\n    jet.jet('water', 31)\n    jet.jet('oxygen', 0)\n", 5000, 256).await;
        assert!(output.report.ok, "{:?}", output.report);
        assert_eq!(output.report.commands.len(), 1);
        assert_eq!(
            output.report.commands[0].action,
            ComponentAction::Jet {
                material: "oxygen".into(),
                rate_mg_per_tick: 0.0,
                angle_deg: 0.0,
            }
        );
        let commands = output.report.commands.clone();
        assert!(f.commit(0, output));
        let after = f.context().organism;
        assert_eq!(after.ai.pending_commands, commands);
        let root = before.composition.root_body.unwrap();
        for (id, state) in &before.component_states {
            let expected = if *id == root {
                state.energy_mj.saturating_sub(5)
            } else {
                state.energy_mj
            };
            assert_eq!(after.component_states[id].energy_mj, expected);
        }
        assert_eq!(before.position, after.position);
        assert_eq!(before.rotation_mdeg, after.rotation_mdeg);
    }

    #[tokio::test]
    async fn disabled_runtime_rejects_calls_before_starting_python() {
        let f = Fixture::new(true, false);
        let runtime = AiRuntime::new(AiConfig::default()).unwrap();
        let error = runtime
            .run(
                &f.root.join("program"),
                Arc::new(Mutex::new(())),
                f.context(),
            )
            .await
            .err()
            .unwrap();
        assert!(error.to_string().contains("disabled"));
        assert!(!f.root.join("program/main.py").exists());
    }
}
