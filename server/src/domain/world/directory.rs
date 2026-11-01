use super::{World, WorldGenerationError, generate_basic_world};
use crate::domain::terrain::TerrainRegistry;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    fmt, fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldKind {
    ServerProvided,
    #[serde(alias = "tutorial")]
    PlayerCreated,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldStatus {
    Active,
    Paused,
}

pub const MAX_LEVEL_ID_LENGTH: usize = 64;
pub const MAX_WORLD_NAME_LENGTH: usize = 128;
pub const MAX_WORLDS_PER_USER: usize = 16;
pub const MAX_TOTAL_WORLDS: usize = 1024;

#[derive(Debug)]
pub struct WorldInstance {
    pub id: Uuid,
    pub owner_user_id: i64,
    pub kind: WorldKind,
    pub name: String,
    pub level_id: String,
    is_tutorial: bool,
    world: Mutex<Option<Arc<World>>>,
    status: Mutex<WorldStatus>,
}

impl WorldInstance {
    pub fn is_tutorial(&self) -> bool {
        self.is_tutorial
    }
    pub fn status(&self) -> WorldStatus {
        *self.status.lock().expect("world status lock poisoned")
    }

    pub fn pause(&self) {
        *self.status.lock().expect("world status lock poisoned") = WorldStatus::Paused;
    }

    pub fn resume(&self) {
        *self.status.lock().expect("world status lock poisoned") = WorldStatus::Active;
    }

    pub fn world(&self) -> Option<Arc<World>> {
        self.world
            .lock()
            .expect("world runtime lock poisoned")
            .clone()
    }
}

fn default_world_name(is_tutorial: bool, level_id: &str) -> String {
    if is_tutorial {
        "新手世界".to_owned()
    } else {
        level_id.to_owned()
    }
}

struct DirectoryState {
    worlds: BTreeMap<Uuid, Arc<WorldInstance>>,
    tutorial_by_user_and_level: HashMap<(i64, String), Uuid>,
}

/// Shared directory for live worlds and persisted snapshots.
pub struct WorldDirectory {
    terrains: Arc<TerrainRegistry>,
    storage_dir: Option<PathBuf>,
    state: Mutex<DirectoryState>,
}

impl WorldDirectory {
    pub fn new(terrains: Arc<TerrainRegistry>) -> Self {
        Self {
            terrains,
            storage_dir: None,
            state: Mutex::new(DirectoryState {
                worlds: BTreeMap::new(),
                tutorial_by_user_and_level: HashMap::new(),
            }),
        }
    }

    pub fn new_with_storage(
        terrains: Arc<TerrainRegistry>,
        storage_dir: impl Into<PathBuf>,
    ) -> Result<Self, WorldDirectoryError> {
        let storage_dir = storage_dir.into();
        fs::create_dir_all(&storage_dir).map_err(WorldDirectoryError::Storage)?;
        let storage_dir = fs::canonicalize(storage_dir).map_err(WorldDirectoryError::Storage)?;
        let mut persisted_state = DirectoryState {
            worlds: BTreeMap::new(),
            tutorial_by_user_and_level: HashMap::new(),
        };
        for entry in fs::read_dir(&storage_dir).map_err(WorldDirectoryError::Storage)? {
            let entry = entry.map_err(WorldDirectoryError::Storage)?;
            if entry
                .path()
                .extension()
                .and_then(|extension| extension.to_str())
                != Some("json")
            {
                continue;
            }
            let data = fs::read(entry.path()).map_err(WorldDirectoryError::Storage)?;
            let snapshot: PersistedWorld =
                serde_json::from_slice(&data).map_err(WorldDirectoryError::Serialization)?;
            let is_tutorial = snapshot.is_tutorial || snapshot.level_id == "tutorial";
            let instance = Arc::new(WorldInstance {
                id: snapshot.id,
                owner_user_id: snapshot.owner_user_id,
                kind: snapshot.kind,
                name: if snapshot.name.is_empty() {
                    default_world_name(is_tutorial, &snapshot.level_id)
                } else {
                    snapshot.name.clone()
                },
                level_id: snapshot.level_id.clone(),
                is_tutorial,
                world: Mutex::new(None),
                status: Mutex::new(WorldStatus::Paused),
            });
            if is_tutorial {
                persisted_state
                    .tutorial_by_user_and_level
                    .insert((snapshot.owner_user_id, snapshot.level_id), snapshot.id);
            }
            persisted_state.worlds.insert(snapshot.id, instance);
        }
        Ok(Self {
            terrains,
            storage_dir: Some(storage_dir),
            state: Mutex::new(persisted_state),
        })
    }

    /// 为用户创建或复用新手世界，因此重复请求不会产生多个新手世界。
    pub fn get_or_create_tutorial(
        &self,
        user_id: i64,
    ) -> Result<Arc<WorldInstance>, WorldDirectoryError> {
        let mut state = self.state.lock().expect("world directory lock poisoned");
        let tutorial_key = (user_id, "tutorial".to_string());
        if let Some(world_id) = state.tutorial_by_user_and_level.get(&tutorial_key) {
            if let Some(world) = state.worlds.get(world_id) {
                return Ok(Arc::clone(world));
            }
        }

        let world = Self::create_world_locked(
            &mut state,
            &self.terrains,
            user_id,
            WorldKind::PlayerCreated,
            "新手世界",
            "tutorial",
            true,
        )?;
        drop(state);
        if let Err(error) = self.persist_created_world(&world) {
            self.remove_world(world.id);
            return Err(error);
        }
        Ok(world)
    }

    pub fn create_player_world(
        &self,
        user_id: i64,
        level_id: impl Into<String>,
        name: impl Into<String>,
    ) -> Result<Arc<WorldInstance>, WorldDirectoryError> {
        self.create_player_world_with_ai_visibility(user_id, level_id, name, false)
    }

    pub fn create_player_world_with_ai_visibility(
        &self,
        user_id: i64,
        level_id: impl Into<String>,
        name: impl Into<String>,
        full_visibility: bool,
    ) -> Result<Arc<WorldInstance>, WorldDirectoryError> {
        let mut state = self.state.lock().expect("world directory lock poisoned");
        let level_id = level_id.into();
        let level_id = level_id.trim().to_owned();
        let name = name.into();
        let name = name.trim().to_owned();
        let world = Self::create_world_locked(
            &mut state,
            &self.terrains,
            user_id,
            WorldKind::PlayerCreated,
            if name.is_empty() { &level_id } else { &name },
            &level_id,
            false,
        )?;
        if let Some(runtime) = world
            .world
            .lock()
            .expect("world runtime lock poisoned")
            .as_mut()
        {
            Arc::make_mut(runtime).ai_full_visibility = full_visibility;
        }
        drop(state);
        if let Err(error) = self.persist_created_world(&world) {
            self.remove_world(world.id);
            return Err(error);
        }
        Ok(world)
    }

    fn remove_world(&self, world_id: Uuid) {
        let mut state = self.state.lock().expect("world directory lock poisoned");
        let Some(world) = state.worlds.remove(&world_id) else {
            return;
        };
        if world.is_tutorial {
            state
                .tutorial_by_user_and_level
                .retain(|_, id| *id != world_id);
        }
        if let Some(path) = self.snapshot_path(world_id) {
            if let Err(error) = fs::remove_file(path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(target: "world", %error, %world_id, "failed to remove rolled-back world snapshot");
            }
        }
    }

    pub fn get(&self, world_id: Uuid) -> Option<Arc<WorldInstance>> {
        self.state
            .lock()
            .expect("world directory lock poisoned")
            .worlds
            .get(&world_id)
            .cloned()
    }

    pub fn list_for_user(&self, user_id: i64) -> Vec<Arc<WorldInstance>> {
        self.state
            .lock()
            .expect("world directory lock poisoned")
            .worlds
            .values()
            .filter(|world| world.owner_user_id == user_id)
            .cloned()
            .collect()
    }

    pub fn active_worlds(&self) -> Vec<Arc<WorldInstance>> {
        self.state
            .lock()
            .expect("world directory lock poisoned")
            .worlds
            .values()
            .filter(|world| world.status() == WorldStatus::Active)
            .cloned()
            .collect()
    }

    pub fn set_status(&self, world_id: Uuid, status: WorldStatus) -> Option<Arc<WorldInstance>> {
        let world = self.get(world_id)?;
        let mut runtime = world.world.lock().expect("world runtime lock poisoned");
        match status {
            WorldStatus::Active => {
                if runtime.is_none() {
                    if self.storage_dir.is_some() {
                        if let Ok(loaded) = self.load_snapshot(world_id) {
                            *runtime = Some(Arc::new(loaded));
                        } else {
                            return None;
                        }
                    }
                }
                world.resume();
            }
            WorldStatus::Paused => {
                if let Some(current) = runtime.as_ref() {
                    if self.storage_dir.is_some() {
                        if self.save_snapshot(&world, current).is_err() {
                            return None;
                        }
                        *runtime = None;
                    }
                }
                world.pause();
            }
        }
        drop(runtime);
        Some(world)
    }

    /// Read a world regardless of whether it is loaded, without resuming its ticks.
    pub fn read_world(
        &self,
        world_id: Uuid,
        user_id: i64,
    ) -> Result<Arc<World>, WorldDirectoryError> {
        let world = self
            .get(world_id)
            .ok_or(WorldDirectoryError::WorldNotFound)?;
        if world.owner_user_id != user_id {
            return Err(WorldDirectoryError::Forbidden);
        }
        let runtime = world.world.lock().expect("world runtime lock poisoned");
        match runtime.as_ref() {
            Some(current) => Ok(Arc::clone(current)),
            None => self.load_snapshot(world_id).map(Arc::new),
        }
    }

    pub fn list_organisms(
        &self,
        world_id: Uuid,
        user_id: i64,
    ) -> Result<Vec<crate::domain::organism::OrganismInstance>, WorldDirectoryError> {
        Ok(self
            .read_world(world_id, user_id)?
            .organisms
            .values()
            .filter(|organism| organism.owner_user_id == user_id)
            .cloned()
            .collect())
    }

    pub fn add_organism(
        &self,
        world_id: Uuid,
        organism: crate::domain::organism::OrganismInstance,
    ) -> Result<(), WorldDirectoryError> {
        let world = self
            .get(world_id)
            .ok_or(WorldDirectoryError::WorldNotFound)?;
        if world.owner_user_id != organism.owner_user_id {
            return Err(WorldDirectoryError::Forbidden);
        }
        let mut runtime = world.world.lock().expect("world runtime lock poisoned");
        let mut updated = match runtime.as_ref() {
            Some(runtime) => runtime.as_ref().clone(),
            None => self.load_snapshot(world_id)?,
        };
        updated.organisms.insert(organism.id, organism);
        // Persist before publishing so a failed write never creates a phantom instance.
        self.save_snapshot(&world, &updated)?;
        if runtime.is_some() {
            *runtime = Some(Arc::new(updated));
        }
        Ok(())
    }

    /// Commit AI memory only if this invocation still owns the current revision.
    pub fn commit_ai(
        &self,
        world_id: Uuid,
        organism_id: Uuid,
        owner_user_id: i64,
        expected_calls: u64,
        report: crate::domain::organism::AiRunReport,
        memory: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<bool, WorldDirectoryError> {
        let world = self
            .get(world_id)
            .ok_or(WorldDirectoryError::WorldNotFound)?;
        if world.owner_user_id != owner_user_id {
            return Err(WorldDirectoryError::Forbidden);
        }
        let mut runtime = world.world.lock().expect("world runtime lock poisoned");
        if world.status() != WorldStatus::Active {
            return Ok(false);
        }
        let mut updated = match runtime.as_ref() {
            Some(value) => value.as_ref().clone(),
            None => self.load_snapshot(world_id)?,
        };
        let Some(organism) = updated.organisms.get_mut(&organism_id) else {
            return Ok(false);
        };
        if organism.owner_user_id != owner_user_id || organism.ai.calls != expected_calls {
            return Ok(false);
        }
        let commands = if report.ok {
            organism.ai.pending_commands = report.commands.clone();
            report.commands.clone()
        } else {
            // A failed AI update keeps the previous control signal active for this tick.
            organism.ai.pending_commands.clone()
        };
        let had_energy = organism
            .component_states
            .values()
            .any(|state| state.energy_mj > 0);
        let mut cost = 0u64;
        for (id, state) in &organism.component_states {
            if state.durability == 0 { continue; }
            let Some(part) = organism.composition.components.get(id) else { continue; };
            let builtin = crate::domain::organism_component::builtin_component_api(part.component.as_str());
            let api = state.properties.api.as_ref().unwrap_or(&builtin);
            // Rate-controlled organs already carry their base cost; passive organs
            // expose it separately. Fall back for snapshots predating passive costs.
            let base = api.base_energy_mj.or(builtin.base_energy_mj).unwrap_or(0)
                + api.jet.as_ref().map_or(0, |value| value.base_energy_mj)
                + api.absorption.as_ref().map_or(0, |value| value.base_energy_mj);
            cost = cost.saturating_add(u64::from(base));
            if let Some(command) = commands.iter().rev().find(|command| command.component_id == *id) {
                use crate::domain::organism_component::ComponentAction;
                let (spec, rate) = match &command.action {
                    ComponentAction::Jet { rate_mg_per_tick, .. } => (api.jet.as_ref(), *rate_mg_per_tick),
                    ComponentAction::AbsorbAir { rate_mg_per_tick } | ComponentAction::AbsorbMatter { rate_mg_per_tick } => (api.absorption.as_ref(), *rate_mg_per_tick),
                };
                if let Some(spec) = spec {
                    cost = cost.saturating_add(spec.energy_cost(rate) - u64::from(spec.base_energy_mj));
                }
            }
        }
        if let Some(root) = organism.composition.root_body {
            if let Some(state) = organism.component_states.get_mut(&root) {
                let spent = state.energy_mj.min(cost);
                state.energy_mj -= spent;
                cost -= spent;
            }
        }
        for state in organism.component_states.values_mut() {
            let spent = state.energy_mj.min(cost);
            state.energy_mj -= spent;
            cost -= spent;
            if cost == 0 { break; }
        }
        // Zero energy means the instance has no behavior left to execute.
        if had_energy
            && organism
                .component_states
                .values()
                .all(|state| state.energy_mj == 0)
        {
            organism.ai.pending_commands.clear();
        }
        if report.ok {
            if let Some(memory) = memory {
                organism.ai.memory = memory;
            }
        }
        organism.ai.calls = organism.ai.calls.saturating_add(1);
        organism.ai.history.push(report.clone());
        if organism.ai.history.len() > 256 {
            let trim = organism.ai.history.len() - 256;
            organism.ai.history.drain(..trim);
        }
        organism.ai.last_run = Some(report);
        self.save_snapshot(&world, &updated)?;
        if runtime.is_some() {
            *runtime = Some(Arc::new(updated));
        }
        Ok(true)
    }

    pub fn remove_for_user(&self, user_id: i64) {
        let mut state = self.state.lock().expect("world directory lock poisoned");
        let world_ids: Vec<_> = state
            .worlds
            .values()
            .filter(|world| world.owner_user_id == user_id)
            .map(|world| world.id)
            .collect();
        for world_id in world_ids {
            state.worlds.remove(&world_id);
            if let Some(path) = self.snapshot_path(world_id) {
                if let Err(error) = fs::remove_file(path)
                    && error.kind() != std::io::ErrorKind::NotFound
                {
                    tracing::warn!(target: "world", %error, %world_id, "failed to remove world snapshot");
                }
            }
        }
        state
            .tutorial_by_user_and_level
            .retain(|(owner_id, _), _| *owner_id != user_id);
    }

    fn create_world_locked(
        state: &mut DirectoryState,
        terrains: &TerrainRegistry,
        user_id: i64,
        kind: WorldKind,
        name: &str,
        level_id: &str,
        is_tutorial: bool,
    ) -> Result<Arc<WorldInstance>, WorldDirectoryError> {
        if level_id.trim().is_empty() {
            return Err(WorldDirectoryError::InvalidLevelId);
        }
        if level_id.chars().count() > MAX_LEVEL_ID_LENGTH {
            return Err(WorldDirectoryError::LevelIdTooLong);
        }
        if name.trim().is_empty() {
            return Err(WorldDirectoryError::InvalidWorldName);
        }
        if name.chars().count() > MAX_WORLD_NAME_LENGTH {
            return Err(WorldDirectoryError::WorldNameTooLong);
        }
        let user_world_count = state
            .worlds
            .values()
            .filter(|world| world.owner_user_id == user_id)
            .count();
        if user_world_count >= MAX_WORLDS_PER_USER {
            return Err(WorldDirectoryError::WorldLimitReached);
        }
        if state.worlds.len() >= MAX_TOTAL_WORLDS {
            return Err(WorldDirectoryError::WorldLimitReached);
        }

        let world = Arc::new(WorldInstance {
            id: Uuid::new_v4(),
            owner_user_id: user_id,
            kind,
            name: name.to_owned(),
            level_id: level_id.to_owned(),
            is_tutorial,
            world: Mutex::new(Some(Arc::new(generate_basic_world(terrains)?))),
            status: Mutex::new(WorldStatus::Active),
        });
        if is_tutorial {
            state
                .tutorial_by_user_and_level
                .insert((user_id, level_id.to_owned()), world.id);
        }
        state.worlds.insert(world.id, Arc::clone(&world));
        Ok(world)
    }

    fn persist_created_world(&self, world: &WorldInstance) -> Result<(), WorldDirectoryError> {
        if let Some(runtime) = world.world() {
            self.save_snapshot(world, &runtime)?;
        }
        Ok(())
    }

    fn snapshot_path(&self, world_id: Uuid) -> Option<PathBuf> {
        self.storage_dir
            .as_ref()
            .map(|directory| directory.join(format!("{world_id}.json")))
    }

    fn save_snapshot(
        &self,
        world: &WorldInstance,
        runtime: &World,
    ) -> Result<(), WorldDirectoryError> {
        let Some(path) = self.snapshot_path(world.id) else {
            return Ok(());
        };
        let snapshot = PersistedWorld {
            id: world.id,
            owner_user_id: world.owner_user_id,
            kind: world.kind,
            name: world.name.clone(),
            level_id: world.level_id.clone(),
            is_tutorial: world.is_tutorial,
            status: WorldStatus::Paused,
            world: runtime.clone(),
        };
        let data =
            serde_json::to_vec_pretty(&snapshot).map_err(WorldDirectoryError::Serialization)?;
        let temporary = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
        let result = fs::write(&temporary, data).and_then(|_| fs::rename(&temporary, &path));
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result.map_err(WorldDirectoryError::Storage)
    }

    fn load_snapshot(&self, world_id: Uuid) -> Result<World, WorldDirectoryError> {
        let path = self
            .snapshot_path(world_id)
            .ok_or(WorldDirectoryError::SnapshotUnavailable)?;
        let data = fs::read(path).map_err(WorldDirectoryError::Storage)?;
        let snapshot: PersistedWorld =
            serde_json::from_slice(&data).map_err(WorldDirectoryError::Serialization)?;
        let mut world = snapshot.world;
        for organism in world.organisms.values_mut() {
            organism.composition.id = organism.id;
            organism.composition.program_id =
                crate::domain::OrganismProgramId::from_uuid(organism.id);
        }
        Ok(world)
    }
}

#[derive(Deserialize, Serialize)]
struct PersistedWorld {
    id: Uuid,
    owner_user_id: i64,
    kind: WorldKind,
    #[serde(default)]
    name: String,
    level_id: String,
    #[serde(default)]
    is_tutorial: bool,
    status: WorldStatus,
    world: World,
}

#[derive(Debug)]
pub enum WorldDirectoryError {
    WorldNotFound,
    Forbidden,
    Generation(WorldGenerationError),
    InvalidLevelId,
    LevelIdTooLong,
    InvalidWorldName,
    WorldNameTooLong,
    WorldLimitReached,
    Storage(std::io::Error),
    Serialization(serde_json::Error),
    SnapshotUnavailable,
}

impl fmt::Display for WorldDirectoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WorldNotFound => formatter.write_str("world not found"),
            Self::Forbidden => formatter.write_str("world access forbidden"),
            Self::Generation(error) => write!(formatter, "failed to generate world: {error}"),
            Self::InvalidLevelId => formatter.write_str("world level id cannot be empty"),
            Self::LevelIdTooLong => write!(
                formatter,
                "world level id exceeds {MAX_LEVEL_ID_LENGTH} characters"
            ),
            Self::InvalidWorldName => formatter.write_str("world name cannot be empty"),
            Self::WorldNameTooLong => write!(
                formatter,
                "world name exceeds {MAX_WORLD_NAME_LENGTH} characters"
            ),
            Self::WorldLimitReached => formatter.write_str("world creation limit reached"),
            Self::Storage(error) => write!(formatter, "world storage error: {error}"),
            Self::Serialization(error) => write!(formatter, "world serialization error: {error}"),
            Self::SnapshotUnavailable => formatter.write_str("world snapshot is unavailable"),
        }
    }
}

impl std::error::Error for WorldDirectoryError {}

impl From<WorldGenerationError> for WorldDirectoryError {
    fn from(error: WorldGenerationError) -> Self {
        Self::Generation(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::create_default_terrain_registry;

    fn test_organism(user_id: i64) -> crate::domain::organism::OrganismInstance {
        let registry = crate::domain::create_default_component_registry().unwrap();
        let mut model =
            crate::domain::BiologicalOrganism::new(crate::domain::OrganismProgramId::new());
        model.add_component("small_body_1", &registry).unwrap();
        crate::domain::organism::OrganismInstance::from_model(
            model.id,
            user_id,
            "test".into(),
            model,
            &registry,
        )
        .unwrap()
    }

    #[test]
    fn ai_without_commands_consumes_energy_on_success_and_failure() {
        let directory = WorldDirectory::new(Arc::new(create_default_terrain_registry().unwrap()));
        let world = directory.create_player_world(7, "energy", "energy").unwrap();
        let registry = crate::domain::create_default_component_registry().unwrap();
        let mut model = crate::domain::BiologicalOrganism::new(crate::domain::OrganismProgramId::new());
        let root = model.add_component("small_body_1", &registry).unwrap();
        for name in ["jet_organ_1", "absorption_organ_1", "skin_1", "photosensor_organ_1"] {
            let part = model.add_component(name, &registry).unwrap();
            model.connect(root, part, &registry).unwrap();
        }
        let mut organism = crate::domain::organism::OrganismInstance::from_model(model.id, 7, "energy".into(), model, &registry).unwrap();
        organism.component_states.get_mut(&root).unwrap().energy_mj = 24;
        let id = organism.id;
        directory.add_organism(world.id, organism).unwrap();
        for (calls, ok) in [(0, true), (1, false)] {
            let report = crate::domain::organism::AiRunReport {
                ok, logs: "1\n".into(), error: (!ok).then(|| "test error".into()),
                duration_ms: 0, commands: vec![], metrics: Default::default(),
            };
            assert!(directory.commit_ai(world.id, id, 7, calls, report.clone(), None).unwrap());
            let current = directory.read_world(world.id, 7).unwrap();
            let instance = &current.organisms[&id];
            assert_eq!(instance.component_states[&root].energy_mj, 12 * (1 - calls));
            assert_eq!(instance.ai.last_run.as_ref(), Some(&report));
            assert!(instance.ai.pending_commands.is_empty());
        }
        world.pause();
        let report = crate::domain::organism::AiRunReport {
            ok: true, logs: String::new(), error: None, duration_ms: 0,
            commands: vec![], metrics: Default::default(),
        };
        assert!(!directory.commit_ai(world.id, id, 7, 2, report, None).unwrap());
        assert_eq!(directory.read_world(world.id, 7).unwrap().organisms[&id].ai.calls, 2);
    }

    #[test]
    fn ai_commit_waiting_on_runtime_lock_cannot_write_after_pause() {
        let directory = Arc::new(WorldDirectory::new(Arc::new(create_default_terrain_registry().unwrap())));
        let world = directory.create_player_world(7, "pause", "pause").unwrap();
        let organism = test_organism(7);
        let id = organism.id;
        directory.add_organism(world.id, organism).unwrap();
        let before = world.world().unwrap();
        let guard = world.world.lock().unwrap();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (finished_tx, finished_rx) = std::sync::mpsc::channel();
        let task_directory = directory.clone();
        let world_id = world.id;
        let worker = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            let report = crate::domain::organism::AiRunReport {
                ok: true, logs: "should not commit".into(), error: None,
                duration_ms: 0, commands: vec![], metrics: Default::default(),
            };
            let result = task_directory.commit_ai(world_id, id, 7, 0, report, Some(Default::default())).unwrap();
            finished_tx.send(result).unwrap();
        });
        started_rx.recv().unwrap();
        assert!(matches!(finished_rx.recv_timeout(std::time::Duration::from_millis(50)), Err(std::sync::mpsc::RecvTimeoutError::Timeout)));
        // Simulate set_status's transition while it owns the same runtime lock.
        world.pause();
        drop(guard);
        assert!(!finished_rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap());
        worker.join().unwrap();
        assert_eq!(directory.read_world(world.id, 7).unwrap(), before);
    }

    #[test]
    fn organisms_survive_restart_and_paused_world_creation_with_isolation() {
        let storage = std::env::temp_dir().join(format!("life-paper-organisms-{}", Uuid::new_v4()));
        let terrains = Arc::new(create_default_terrain_registry().unwrap());
        let directory = WorldDirectory::new_with_storage(terrains.clone(), &storage).unwrap();
        let world = directory.create_player_world(7, "test", "test").unwrap();
        let other = directory.create_player_world(7, "other", "other").unwrap();
        let mut organism = test_organism(7);
        let component = *organism.component_states.keys().next().unwrap();
        let state = organism.component_states.get_mut(&component).unwrap();
        state.durability = 42;
        state.container.non_gas_contents_mg.insert(
            crate::domain::element::ElementId::new("water").unwrap(),
            100,
        );
        directory.add_organism(world.id, organism.clone()).unwrap();
        assert!(directory.list_organisms(other.id, 7).unwrap().is_empty());
        assert!(matches!(
            directory.list_organisms(world.id, 8),
            Err(WorldDirectoryError::Forbidden)
        ));
        assert!(matches!(
            directory.add_organism(world.id, test_organism(8)),
            Err(WorldDirectoryError::Forbidden)
        ));
        assert!(matches!(
            directory.list_organisms(Uuid::new_v4(), 7),
            Err(WorldDirectoryError::WorldNotFound)
        ));
        drop(directory);
        let restored = WorldDirectory::new_with_storage(terrains, &storage).unwrap();
        assert_eq!(
            restored.list_organisms(world.id, 7).unwrap(),
            vec![organism.clone()]
        );
        assert!(restored.get(world.id).unwrap().world().is_none());
        let second = test_organism(7);
        restored.add_organism(world.id, second.clone()).unwrap();
        assert!(restored.get(world.id).unwrap().world().is_none());
        restored.set_status(world.id, WorldStatus::Active).unwrap();
        let current = restored.get(world.id).unwrap().world().unwrap();
        assert_eq!(current.organisms[&organism.id], organism);
        assert_eq!(current.organisms[&second.id], second);
        restored.set_status(world.id, WorldStatus::Paused).unwrap();
        assert_eq!(restored.list_organisms(world.id, 7).unwrap().len(), 2);
        fs::remove_dir_all(storage).unwrap();
    }

    #[test]
    fn failed_organism_snapshot_keeps_runtime_unchanged() {
        let storage =
            std::env::temp_dir().join(format!("life-paper-organism-failure-{}", Uuid::new_v4()));
        let terrains = Arc::new(create_default_terrain_registry().unwrap());
        let directory = WorldDirectory::new_with_storage(terrains, &storage).unwrap();
        let world = directory.create_player_world(7, "test", "test").unwrap();
        fs::remove_dir_all(&storage).unwrap();
        fs::write(&storage, b"blocked").unwrap();
        assert!(matches!(
            directory.add_organism(world.id, test_organism(7)),
            Err(WorldDirectoryError::Storage(_))
        ));
        assert!(directory.list_organisms(world.id, 7).unwrap().is_empty());
        fs::remove_file(storage).unwrap();
    }

    #[test]
    fn legacy_world_without_organisms_still_loads() {
        let terrains = create_default_terrain_registry().unwrap();
        let world = generate_basic_world(&terrains).unwrap();
        let mut value = serde_json::to_value(&world).unwrap();
        value.as_object_mut().unwrap().remove("organisms");
        value.as_object_mut().unwrap().remove("ai_full_visibility");
        let restored: World = serde_json::from_value(value).unwrap();
        assert!(restored.organisms.is_empty());
        assert_eq!(restored, world);
    }

    #[test]
    fn restarted_worlds_are_listed_and_readable_without_resuming_or_recreating() {
        let storage =
            std::env::temp_dir().join(format!("life-paper-world-read-{}", Uuid::new_v4()));
        let terrains = Arc::new(create_default_terrain_registry().unwrap());
        let directory = WorldDirectory::new_with_storage(terrains.clone(), &storage).unwrap();
        let tutorial = directory.get_or_create_tutorial(7).unwrap();
        let player = directory
            .create_player_world(7, "custom", "Saved world")
            .unwrap();
        let original = directory.read_world(tutorial.id, 7).unwrap();
        let original_snapshot = fs::read(storage.join(format!("{}.json", tutorial.id))).unwrap();
        drop(directory);

        let restarted = WorldDirectory::new_with_storage(terrains, &storage).unwrap();
        assert_eq!(restarted.list_for_user(7).len(), 2);
        assert!(restarted.list_for_user(8).is_empty());
        assert_eq!(restarted.get(player.id).unwrap().name, "Saved world");
        assert!(restarted.get(tutorial.id).unwrap().world().is_none());
        let loaded = restarted.read_world(tutorial.id, 7).unwrap();
        assert_eq!(loaded.ground_config(), original.ground_config());
        assert_eq!(loaded.terrain_layers(), original.terrain_layers());
        assert!(
            restarted
                .read_world(player.id, 7)
                .unwrap()
                .ground_config()
                .is_some()
        );
        assert!(matches!(
            restarted.read_world(tutorial.id, 8),
            Err(WorldDirectoryError::Forbidden)
        ));
        assert!(matches!(
            restarted.read_world(Uuid::new_v4(), 7),
            Err(WorldDirectoryError::WorldNotFound)
        ));
        assert!(restarted.get(tutorial.id).unwrap().world().is_none());
        assert_eq!(
            restarted.get(tutorial.id).unwrap().status(),
            WorldStatus::Paused
        );
        assert!(restarted.active_worlds().is_empty());
        assert_eq!(restarted.get_or_create_tutorial(7).unwrap().id, tutorial.id);
        assert_eq!(restarted.list_for_user(7).len(), 2);
        assert_eq!(
            fs::read(storage.join(format!("{}.json", tutorial.id))).unwrap(),
            original_snapshot
        );
        restarted
            .set_status(tutorial.id, WorldStatus::Active)
            .unwrap();
        assert_eq!(restarted.read_world(tutorial.id, 7).unwrap(), original);
        restarted
            .set_status(tutorial.id, WorldStatus::Paused)
            .unwrap();
        assert_eq!(restarted.read_world(tutorial.id, 7).unwrap(), original);
        fs::remove_dir_all(storage).unwrap();
    }

    #[test]
    fn creates_one_isolated_tutorial_world_per_user() {
        let terrains = Arc::new(create_default_terrain_registry().unwrap());
        let directory = WorldDirectory::new(terrains);
        let first = directory.get_or_create_tutorial(7).unwrap();
        let same = directory.get_or_create_tutorial(7).unwrap();
        let other = directory.get_or_create_tutorial(8).unwrap();

        assert_eq!(first.id, same.id);
        assert_ne!(first.id, other.id);
        assert_eq!(directory.list_for_user(7).len(), 1);
        assert_eq!(directory.list_for_user(8).len(), 1);
        assert_eq!(first.kind, WorldKind::PlayerCreated);
        assert_eq!(first.level_id, "tutorial");
        assert_eq!(first.world().unwrap().terrain_layers().len(), 3);
        assert_eq!(directory.active_worlds().len(), 2);

        first.pause();
        assert_eq!(first.status(), WorldStatus::Paused);
        assert_eq!(directory.active_worlds().len(), 1);
        first.resume();
        assert_eq!(directory.active_worlds().len(), 2);
    }

    #[test]
    fn reads_legacy_tutorial_kind_as_player_created() {
        let kind: WorldKind = serde_json::from_str("\"tutorial\"").unwrap();

        assert_eq!(kind, WorldKind::PlayerCreated);
        assert_eq!(serde_json::to_string(&kind).unwrap(), "\"player_created\"");
    }

    #[test]
    fn rejects_blank_player_world_level_id() {
        let terrains = Arc::new(create_default_terrain_registry().unwrap());
        let directory = WorldDirectory::new(terrains);

        assert!(matches!(
            directory.create_player_world(7, "  ", "测试世界"),
            Err(WorldDirectoryError::InvalidLevelId)
        ));

        assert!(matches!(
            directory.create_player_world(7, "x".repeat(MAX_LEVEL_ID_LENGTH + 1), "测试世界"),
            Err(WorldDirectoryError::LevelIdTooLong)
        ));
    }

    #[test]
    fn rolls_back_directory_entry_when_initial_snapshot_fails() {
        let storage_path =
            std::env::temp_dir().join(format!("life-paper-world-storage-file-{}", Uuid::new_v4()));
        fs::write(&storage_path, b"not a directory").unwrap();
        let terrains = Arc::new(create_default_terrain_registry().unwrap());
        let directory = WorldDirectory {
            terrains,
            storage_dir: Some(storage_path.clone()),
            state: Mutex::new(DirectoryState {
                worlds: BTreeMap::new(),
                tutorial_by_user_and_level: HashMap::new(),
            }),
        };

        assert!(matches!(
            directory.create_player_world(7, "test_level", "测试世界"),
            Err(WorldDirectoryError::Storage(_))
        ));
        assert!(directory.list_for_user(7).is_empty());
        assert!(directory.active_worlds().is_empty());

        fs::remove_file(storage_path).unwrap();
    }

    #[test]
    fn pauses_to_disk_and_reloads_without_runtime_memory() {
        let storage_dir = std::env::temp_dir().join(format!("life-paper-world-{}", Uuid::new_v4()));
        let terrains = Arc::new(create_default_terrain_registry().unwrap());
        let directory =
            WorldDirectory::new_with_storage(Arc::clone(&terrains), &storage_dir).unwrap();
        let world = directory
            .create_player_world(7, "tutorial_variant", "测试世界")
            .unwrap();
        let world_id = world.id;

        assert!(world.world().is_some());
        directory.set_status(world_id, WorldStatus::Paused).unwrap();
        assert!(world.world().is_none());

        let reloaded = WorldDirectory::new_with_storage(terrains, &storage_dir).unwrap();
        let restored = reloaded.get(world_id).unwrap();
        assert_eq!(restored.status(), WorldStatus::Paused);
        assert!(restored.world().is_none());
        reloaded.set_status(world_id, WorldStatus::Active).unwrap();
        assert!(restored.world().is_some());

        fs::remove_dir_all(storage_dir).unwrap();
    }
}
