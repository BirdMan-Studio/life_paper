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

pub struct AiCommit {
    pub organism_id: Uuid,
    pub expected_calls: u64,
    pub report: crate::domain::organism::AiRunReport,
    pub memory: Option<serde_json::Map<String, serde_json::Value>>,
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
                // Invalidate in-flight work from before a pause/resume transition.
                if let Some(current) = runtime.as_ref() {
                    *runtime = Some(Arc::new(current.as_ref().clone()));
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

    #[cfg(test)]
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
        let Some(snapshot) = world.world() else {
            return Ok(false);
        };
        self.commit_tick(
            world_id,
            &snapshot,
            vec![AiCommit {
                organism_id,
                expected_calls,
                report,
                memory,
            }],
        )
    }

    /// Atomically publish one completed simulation tick, including AI and physics.
    pub fn commit_tick(
        &self,
        world_id: Uuid,
        snapshot: &Arc<World>,
        results: Vec<AiCommit>,
    ) -> Result<bool, WorldDirectoryError> {
        let world = self
            .get(world_id)
            .ok_or(WorldDirectoryError::WorldNotFound)?;
        let mut runtime = world.world.lock().expect("world runtime lock poisoned");
        if world.status() != WorldStatus::Active {
            return Ok(false);
        }
        let Some(current) = runtime
            .as_ref()
            .filter(|current| Arc::ptr_eq(current, snapshot))
        else {
            return Ok(false);
        };
        let mut updated = current.as_ref().clone();
        let mut seen = std::collections::HashSet::new();
        for result in results {
            let Some(organism) = updated.organisms.get_mut(&result.organism_id) else {
                return Ok(false);
            };
            if organism.ai.calls != result.expected_calls || !seen.insert(result.organism_id) {
                return Ok(false);
            }
            let report = result.report;
            if report.ok {
                organism.ai.pending_commands = report.commands.clone();
                if let Some(memory) = result.memory {
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
        }
        let mut matter = std::mem::take(&mut updated.matter);
        matter.advance_tick();
        for organism in updated.organisms.values_mut() {
            crate::game::dynamics::advance_with_matter(
                organism,
                snapshot,
                &self.terrains,
                &mut matter,
            );
        }
        updated.matter = matter;
        self.save_snapshot(&world, &updated)?;
        if runtime.is_some() {
            *runtime = Some(Arc::new(updated));
        }
        Ok(true)
    }

    pub fn place_organism(
        &self,
        world_id: Uuid,
        organism: &mut crate::domain::organism::OrganismInstance,
    ) -> Result<(), WorldDirectoryError> {
        let world = self.read_world(world_id, organism.owner_user_id)?;
        crate::game::dynamics::place(organism, &world, &self.terrains);
        Ok(())
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
        let world = directory
            .create_player_world(7, "energy", "energy")
            .unwrap();
        let registry = crate::domain::create_default_component_registry().unwrap();
        let mut model =
            crate::domain::BiologicalOrganism::new(crate::domain::OrganismProgramId::new());
        let root = model.add_component("small_body_1", &registry).unwrap();
        for name in [
            "jet_organ_1",
            "absorption_organ_1",
            "skin_1",
            "photosensor_organ_1",
        ] {
            let part = model.add_component(name, &registry).unwrap();
            model.connect(root, part, &registry).unwrap();
        }
        let mut organism = crate::domain::organism::OrganismInstance::from_model(
            model.id,
            7,
            "energy".into(),
            model,
            &registry,
        )
        .unwrap();
        organism.component_states.get_mut(&root).unwrap().energy_mj = 24;
        let id = organism.id;
        directory.add_organism(world.id, organism).unwrap();
        for (calls, ok) in [(0, true), (1, false)] {
            let report = crate::domain::organism::AiRunReport {
                ok,
                logs: "1\n".into(),
                error: (!ok).then(|| "test error".into()),
                duration_ms: 0,
                commands: vec![],
                metrics: Default::default(),
            };
            assert!(
                directory
                    .commit_ai(world.id, id, 7, calls, report.clone(), None)
                    .unwrap()
            );
            let current = directory.read_world(world.id, 7).unwrap();
            let instance = &current.organisms[&id];
            assert_eq!(instance.component_states[&root].energy_mj, 12 * (1 - calls));
            assert_eq!(instance.ai.last_run.as_ref(), Some(&report));
            assert!(instance.ai.pending_commands.is_empty());
        }
        world.pause();
        let report = crate::domain::organism::AiRunReport {
            ok: true,
            logs: String::new(),
            error: None,
            duration_ms: 0,
            commands: vec![],
            metrics: Default::default(),
        };
        assert!(
            !directory
                .commit_ai(world.id, id, 7, 2, report, None)
                .unwrap()
        );
        assert_eq!(
            directory.read_world(world.id, 7).unwrap().organisms[&id]
                .ai
                .calls,
            2
        );
    }

    #[test]
    fn tick_rejects_changed_world_and_pause_resume_snapshots() {
        let directory = WorldDirectory::new(Arc::new(create_default_terrain_registry().unwrap()));
        let world = directory.create_player_world(7, "tick", "tick").unwrap();
        let before_add = world.world().unwrap();
        let organism = test_organism(7);
        let id = organism.id;
        directory.add_organism(world.id, organism).unwrap();
        assert!(
            !directory
                .commit_tick(world.id, &before_add, vec![])
                .unwrap()
        );
        assert!(world.world().unwrap().organisms.contains_key(&id));
        let before_pause = world.world().unwrap();
        directory.set_status(world.id, WorldStatus::Paused).unwrap();
        assert!(
            !directory
                .commit_tick(world.id, &before_pause, vec![])
                .unwrap()
        );
        directory.set_status(world.id, WorldStatus::Active).unwrap();
        assert!(
            !directory
                .commit_tick(world.id, &before_pause, vec![])
                .unwrap()
        );
        let fresh = world.world().unwrap();
        assert!(directory.commit_tick(world.id, &fresh, vec![]).unwrap());
        assert!(!directory.commit_tick(world.id, &fresh, vec![]).unwrap());
    }

    #[test]
    fn ai_completion_order_does_not_change_tick_and_motion_survives_restart() {
        let storage = std::env::temp_dir().join(format!("life-motion-{}", Uuid::new_v4()));
        let terrains = Arc::new(create_default_terrain_registry().unwrap());
        let directory = WorldDirectory::new_with_storage(terrains.clone(), &storage).unwrap();
        let first = directory.create_player_world(7, "one", "one").unwrap();
        let second = directory.create_player_world(7, "two", "two").unwrap();
        let organisms = [test_organism(7), test_organism(7)];
        for world in [&first, &second] {
            for organism in &organisms {
                directory.add_organism(world.id, organism.clone()).unwrap();
            }
            let mut ids = organisms.iter().map(|o| o.id).collect::<Vec<_>>();
            if world.id == second.id {
                ids.reverse();
            }
            let results = ids
                .into_iter()
                .map(|id| AiCommit {
                    organism_id: id,
                    expected_calls: 0,
                    report: crate::domain::organism::AiRunReport {
                        ok: true,
                        logs: String::new(),
                        error: None,
                        duration_ms: 0,
                        commands: vec![],
                        metrics: Default::default(),
                    },
                    memory: Some(Default::default()),
                })
                .collect();
            assert!(
                directory
                    .commit_tick(world.id, &world.world().unwrap(), results)
                    .unwrap()
            );
        }
        let expected = first.world().unwrap();
        assert_eq!(expected, second.world().unwrap());
        directory.set_status(first.id, WorldStatus::Paused).unwrap();
        let restored = WorldDirectory::new_with_storage(terrains, &storage).unwrap();
        assert_eq!(restored.read_world(first.id, 7).unwrap(), expected);
        restored.set_status(first.id, WorldStatus::Active).unwrap();
        assert_eq!(restored.get(first.id).unwrap().world().unwrap(), expected);
        fs::remove_dir_all(storage).unwrap();
    }

    #[test]
    fn ai_commit_waiting_on_runtime_lock_cannot_write_after_pause() {
        let directory = Arc::new(WorldDirectory::new(Arc::new(
            create_default_terrain_registry().unwrap(),
        )));
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
                ok: true,
                logs: "should not commit".into(),
                error: None,
                duration_ms: 0,
                commands: vec![],
                metrics: Default::default(),
            };
            let result = task_directory
                .commit_ai(world_id, id, 7, 0, report, Some(Default::default()))
                .unwrap();
            finished_tx.send(result).unwrap();
        });
        started_rx.recv().unwrap();
        assert!(matches!(
            finished_rx.recv_timeout(std::time::Duration::from_millis(50)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ));
        // Simulate set_status's transition while it owns the same runtime lock.
        world.pause();
        drop(guard);
        assert!(
            !finished_rx
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap()
        );
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
            100.0,
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
    fn absorption_and_world_matter_commit_restart_and_rollback_together() {
        let storage = std::env::temp_dir().join(format!("life-paper-matter-{}", Uuid::new_v4()));
        let terrains = Arc::new(create_default_terrain_registry().unwrap());
        let directory = WorldDirectory::new_with_storage(terrains.clone(), &storage).unwrap();
        let world = directory
            .create_player_world(7, "matter", "matter")
            .unwrap();
        let registry = crate::domain::create_default_component_registry().unwrap();
        let mut model =
            crate::domain::BiologicalOrganism::new(crate::domain::OrganismProgramId::new());
        let root = model.add_component("small_body_1", &registry).unwrap();
        let organ = model
            .add_component("absorption_organ_1", &registry)
            .unwrap();
        model.connect(root, organ, &registry).unwrap();
        let mut organism = crate::domain::organism::OrganismInstance::from_model(
            model.id,
            7,
            "matter".into(),
            model,
            &registry,
        )
        .unwrap();
        organism.position = crate::domain::MapPosition { x: 200, y: 200 };
        organism
            .ai
            .pending_commands
            .push(crate::domain::organism_component::ComponentCommand {
                component_id: organ,
                action: crate::domain::organism_component::ComponentAction::AbsorbMatter {
                    rate_mg_per_tick: 50.0,
                },
            });
        let first_id = organism.id;
        directory.add_organism(world.id, organism.clone()).unwrap();
        organism.id = Uuid::new_v4();
        organism.composition.id = organism.id;
        organism.composition.program_id = crate::domain::OrganismProgramId::from_uuid(organism.id);
        directory.add_organism(world.id, organism).unwrap();
        directory.set_status(world.id, WorldStatus::Active).unwrap();
        let before = world.world().unwrap();
        assert!(directory.commit_tick(world.id, &before, vec![]).unwrap());
        let after = world.world().unwrap();
        assert_eq!(after.matter.tick, 1);
        assert_eq!(after.matter.records.len(), 2);
        assert!(after.matter.records.iter().all(|record| {
            let source = record.source.as_ref().unwrap();
            after.organisms.contains_key(&source.organism_id)
                && source.component_id == organ
                && record.tick == after.matter.tick
                && record.expires_at_tick.unwrap() > record.tick
        }));
        assert!(
            (after.organisms[&first_id].component_states[&root]
                .container
                .non_gas_contents_mg
                .values()
                .sum::<f64>()
                - 50.0)
                .abs()
                < 1e-9
        );
        let removed: f64 = after
            .matter
            .records
            .iter()
            .flat_map(|r| r.absorbed_mg.values())
            .sum();
        assert!((removed - 100.0).abs() < 1e-9);
        let record = &after.matter.records[0];
        let initial_mass: f64 = before
            .matter
            .area(&before, record.center, record.radius_px)
            .unwrap()
            .total_mg
            .values()
            .sum();
        let final_mass: f64 = after
            .matter
            .area(&after, record.center, record.radius_px)
            .unwrap()
            .total_mg
            .values()
            .sum();
        assert!((initial_mass - final_mass - removed).abs() < 1e-8);
        let restarted = WorldDirectory::new_with_storage(terrains, &storage).unwrap();
        assert_eq!(
            restarted.read_world(world.id, 7).unwrap().as_ref(),
            after.as_ref()
        );
        fs::remove_dir_all(&storage).unwrap();
        fs::write(&storage, b"blocked").unwrap();
        assert!(directory.commit_tick(world.id, &after, vec![]).is_err());
        assert_eq!(world.world().unwrap().as_ref(), after.as_ref());
        fs::remove_file(storage).unwrap();
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
