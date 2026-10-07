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

/// 运行中的世界目录。世界内容本身不可变，后续可在实例内部增加游戏状态。
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
        self.persist_created_world(&world)?;
        Ok(world)
    }

    pub fn create_player_world(
        &self,
        user_id: i64,
        level_id: impl Into<String>,
        name: impl Into<String>,
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
        self.persist_created_world(&world)?;
        Ok(world)
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
        match status {
            WorldStatus::Active => {
                if world.world().is_none() {
                    if self.storage_dir.is_some() {
                        if let Ok(runtime) = self.load_snapshot(world_id) {
                            *world.world.lock().expect("world runtime lock poisoned") =
                                Some(Arc::new(runtime));
                        } else {
                            return None;
                        }
                    }
                }
                world.resume();
            }
            WorldStatus::Paused => {
                if let Some(runtime) = world.world() {
                    if self.storage_dir.is_some() {
                        if self.save_snapshot(&world, &runtime).is_err() {
                            return None;
                        }
                        *world.world.lock().expect("world runtime lock poisoned") = None;
                    }
                }
                world.pause();
            }
        }
        Some(world)
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
        fs::write(path, data).map_err(WorldDirectoryError::Storage)
    }

    fn load_snapshot(&self, world_id: Uuid) -> Result<World, WorldDirectoryError> {
        let path = self
            .snapshot_path(world_id)
            .ok_or(WorldDirectoryError::SnapshotUnavailable)?;
        let data = fs::read(path).map_err(WorldDirectoryError::Storage)?;
        let snapshot: PersistedWorld =
            serde_json::from_slice(&data).map_err(WorldDirectoryError::Serialization)?;
        Ok(snapshot.world)
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
