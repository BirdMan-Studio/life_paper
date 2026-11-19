use super::error::ApiError;
use crate::game_state::{GameIdentity, GameState, SpawnProgram};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

const MAX_USER_BYTES: u64 = crate::domain::organism::program_storage::MAX_ACCOUNT_STORAGE_BYTES;
const MAX_USER_ENTRIES: u64 = 32768;
const MAX_PROGRAM_ENTRIES: u64 = 1024;

#[derive(Default, Clone, Copy)]
struct Usage {
    bytes: u64,
    entries: u64,
}

fn metadata(path: &Path) -> Result<Option<fs::Metadata>, ApiError> {
    match fs::symlink_metadata(path) {
        Ok(value) => {
            let mut linked = value.file_type().is_symlink();
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                linked |= value.file_attributes() & 0x400 != 0;
            }
            if linked || !(value.is_file() || value.is_dir()) {
                return Err(ApiError::BadRequest("不支持的代码目录项".into()));
            }
            Ok(Some(value))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(storage_error(error)),
    }
}

fn storage_error(error: std::io::Error) -> ApiError {
    tracing::error!(%error, "failed to measure program storage quota");
    ApiError::Internal
}

fn usage(root: &Path) -> Result<Usage, ApiError> {
    let mut result = Usage::default();
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        let Some(info) = metadata(&path)? else {
            continue;
        };
        result.entries += 1;
        if info.is_file() {
            result.bytes = result.bytes.saturating_add(info.len());
        } else {
            for entry in fs::read_dir(path).map_err(storage_error)? {
                pending.push(entry.map_err(storage_error)?.path());
            }
        }
    }
    Ok(result)
}

// Called while holding organism_model_storage_lock, including the eventual write.
pub async fn user_roots(state: &GameState, user_id: i64) -> Result<Vec<PathBuf>, ApiError> {
    let mut roots = BTreeSet::new();
    if let GameIdentity::Embedded(central) = &state.identity {
        for model in central
            .organism_models
            .list_for_user(user_id)
            .await
            .map_err(|_| ApiError::Internal)?
        {
            roots.insert(central.organism_model_storage.join(model.id.to_string()));
        }
    }
    let worlds = state.worlds.clone();
    let storage = state.organism_instance_storage.clone();
    tokio::task::spawn_blocking(move || {
        for world in worlds.list_for_user(user_id) {
            let snapshot = worlds
                .read_world(world.id, user_id)
                .map_err(super::world::map_world_error)?;
            for organism in snapshot
                .organisms
                .values()
                .filter(|item| item.owner_user_id == user_id)
            {
                roots.insert(storage.join(organism.id.to_string()));
            }
        }
        Ok(roots.into_iter().collect())
    })
    .await
    .map_err(|_| ApiError::Internal)?
}

fn total_usage(roots: &[PathBuf]) -> Result<Usage, ApiError> {
    let mut total = Usage::default();
    for root in roots {
        let current = usage(root)?;
        total.bytes = total.bytes.saturating_add(current.bytes);
        total.entries = total.entries.saturating_add(current.entries);
    }
    Ok(total)
}

pub fn check_write(
    roots: &[PathBuf],
    root: &Path,
    target: &Path,
    bytes: Option<u64>,
) -> Result<(), ApiError> {
    let total = total_usage(roots)?;
    let current = metadata(target)?;
    let previous = current
        .as_ref()
        .filter(|info| info.is_file())
        .map_or(0, fs::Metadata::len);
    let mut added_entries = 0;
    let mut path = target;
    while path.starts_with(root) && metadata(path)?.is_none() {
        added_entries += 1;
        let Some(parent) = path.parent() else { break };
        path = parent;
    }
    let next_bytes = total
        .bytes
        .saturating_sub(previous)
        .saturating_add(bytes.unwrap_or(0));
    if (next_bytes > MAX_USER_BYTES && next_bytes > total.bytes)
        || (added_entries > 0
            && (total.entries.saturating_add(added_entries) > MAX_USER_ENTRIES
                || usage(root)?.entries.saturating_add(added_entries) > MAX_PROGRAM_ENTRIES + 1))
    {
        return Err(ApiError::StorageQuotaExceeded);
    }
    Ok(())
}

pub async fn check_upload(
    state: &GameState,
    user_id: i64,
    root: PathBuf,
    target: PathBuf,
    bytes: Option<u64>,
) -> Result<(), ApiError> {
    let roots = user_roots(state, user_id).await?;
    tokio::task::spawn_blocking(move || check_write(&roots, &root, &target, bytes))
        .await
        .map_err(|_| ApiError::Internal)?
}

pub fn check_copy(roots: &[PathBuf], program: &SpawnProgram) -> Result<(), ApiError> {
    let mut added = match program {
        SpawnProgram::Directory(source) => usage(source)?,
        SpawnProgram::Bundle(bundle) => {
            use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
            let mut result = Usage {
                entries: 1,
                ..Usage::default()
            };
            let mut paths = BTreeSet::new();
            for (path, contents) in &bundle.files {
                result.bytes = result.bytes.saturating_add(
                    URL_SAFE_NO_PAD
                        .decode(contents)
                        .map_err(|_| ApiError::BadRequest("无效的模型文件编码".into()))?
                        .len() as u64,
                );
                paths.insert(PathBuf::from(path.replace('\\', "/")));
            }
            paths.extend(
                bundle
                    .directories
                    .iter()
                    .map(|path| PathBuf::from(path.replace('\\', "/"))),
            );
            for path in paths.clone() {
                for parent in path
                    .ancestors()
                    .skip(1)
                    .filter(|p| !p.as_os_str().is_empty())
                {
                    paths.insert(parent.to_path_buf());
                }
            }
            result.entries += paths.len() as u64;
            result
        }
    };
    let has_entry = match program {
        SpawnProgram::Directory(source) => source.join("main.py").is_file(),
        SpawnProgram::Bundle(bundle) => bundle.files.keys().any(|path| {
            if cfg!(windows) {
                path.eq_ignore_ascii_case("main.py")
            } else {
                path == "main.py"
            }
        }),
    };
    if !has_entry {
        added.entries += 1;
        added.bytes += include_bytes!("../../python/main.py").len() as u64;
    }
    let total = total_usage(roots)?;
    if total.bytes.saturating_add(added.bytes) > MAX_USER_BYTES
        || total.entries.saturating_add(added.entries) > MAX_USER_ENTRIES
        || added.entries > MAX_PROGRAM_ENTRIES + 1
    {
        return Err(ApiError::StorageQuotaExceeded);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Storage(PathBuf);
    impl Storage {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("life-quota-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Storage {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn sized(path: &Path, size: u64) {
        fs::File::create(path).unwrap().set_len(size).unwrap();
    }

    #[test]
    fn cumulative_account_quota_counts_multiple_programs_and_replacement_deltas() {
        let storage = Storage::new();
        let first = storage.0.join("model");
        let second = storage.0.join("instance");
        fs::create_dir_all(first.join("lib")).unwrap();
        fs::create_dir_all(&second).unwrap();
        sized(&first.join("lib/large.py"), MAX_USER_BYTES - 4);
        fs::write(second.join("main.py"), b"code").unwrap();
        let roots = vec![first.clone(), second.clone()];
        let target = second.join("main.py");
        assert!(check_write(&roots, &second, &target, Some(4)).is_ok());
        assert!(matches!(
            check_write(&roots, &second, &target, Some(5)),
            Err(ApiError::StorageQuotaExceeded)
        ));
        assert!(check_write(&roots, &second, &target, Some(3)).is_ok());
        let missing = first.join("new/helper.py");
        assert!(matches!(
            check_write(&roots, &first, &missing, Some(1)),
            Err(ApiError::StorageQuotaExceeded)
        ));
        assert!(!first.join("new").exists());
        assert_eq!(fs::read(&target).unwrap(), b"code");
        assert!(
            check_write(
                &[second.clone()],
                &second,
                &second.join("other.py"),
                Some(1)
            )
            .is_ok()
        );
        fs::remove_file(first.join("lib/large.py")).unwrap();
        assert!(check_write(&roots, &first, &missing, Some(1)).is_ok());
    }

    #[test]
    fn existing_over_quota_accounts_can_shrink_but_cannot_grow() {
        let storage = Storage::new();
        let file = storage.0.join("main.py");
        sized(&file, MAX_USER_BYTES + 10);
        let roots = vec![storage.0.clone()];
        assert!(check_write(&roots, &storage.0, &file, Some(MAX_USER_BYTES + 9)).is_ok());
        assert!(matches!(
            check_write(&roots, &storage.0, &file, Some(MAX_USER_BYTES + 11)),
            Err(ApiError::StorageQuotaExceeded)
        ));
    }

    #[test]
    fn empty_files_and_implicit_directories_cannot_bypass_entry_limits() {
        let storage = Storage::new();
        for index in 0..1023 {
            fs::write(storage.0.join(format!("{index}.py")), []).unwrap();
        }
        let roots = vec![storage.0.clone()];
        assert!(check_write(&roots, &storage.0, &storage.0.join("last.py"), Some(0)).is_ok());
        assert!(matches!(
            check_write(
                &roots,
                &storage.0,
                &storage.0.join("lib/helper.py"),
                Some(0)
            ),
            Err(ApiError::StorageQuotaExceeded)
        ));
        fs::write(storage.0.join("last.py"), []).unwrap();
        assert!(matches!(
            check_write(&roots, &storage.0, &storage.0.join("folder"), None),
            Err(ApiError::StorageQuotaExceeded)
        ));
        assert!(check_write(&roots, &storage.0, &storage.0.join("last.py"), Some(1)).is_ok());
    }

    #[test]
    fn instance_copy_includes_default_entry_and_cannot_bypass_account_quota() {
        let storage = Storage::new();
        let source = storage.0.join("source");
        let account = storage.0.join("account");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&account).unwrap();
        sized(&account.join("used.py"), MAX_USER_BYTES);
        assert!(matches!(
            check_copy(&[account.clone()], &SpawnProgram::Directory(source.clone())),
            Err(ApiError::StorageQuotaExceeded)
        ));
        fs::remove_file(account.join("used.py")).unwrap();
        assert!(check_copy(&[account], &SpawnProgram::Directory(source)).is_ok());
    }

    #[tokio::test]
    async fn concurrent_http_uploads_share_account_quota_and_deletion_releases_space() {
        use crate::domain::{BiologicalOrganism, OrganismProgramId, WorldDirectory};
        use axum::{
            body::{Body, to_bytes},
            http::{Request, StatusCode},
        };
        use std::sync::Arc;
        use tower::ServiceExt;
        let storage = Storage::new();
        let terrains = Arc::new(crate::domain::create_default_terrain_registry().unwrap());
        let components = Arc::new(crate::domain::create_default_component_registry().unwrap());
        let worlds = Arc::new(WorldDirectory::new(terrains.clone()));
        let mut model = BiologicalOrganism::new(OrganismProgramId::new());
        model.add_component("small_body_1", &components).unwrap();
        let mut paths = Vec::new();
        let instances = storage.0.join("instances");
        for index in 0..2 {
            let world = worlds
                .create_player_world(1, &format!("world-{index}"), "basic")
                .unwrap();
            let organism = crate::domain::organism::OrganismInstance::from_model(
                model.id,
                1,
                "test".into(),
                model.clone(),
                &components,
            )
            .unwrap();
            let root = instances.join(organism.id.to_string());
            fs::create_dir_all(&root).unwrap();
            fs::write(root.join("main.py"), b"x").unwrap();
            paths.push((
                format!("/api/v1/worlds/{}/organisms/{}", world.id, organism.id),
                root,
            ));
            worlds.add_organism(world.id, organism).unwrap();
        }
        sized(&paths[0].1.join("used.py"), MAX_USER_BYTES - 3);
        let state = GameState {
            identity: GameIdentity::Local {
                token: "quota-test".into(),
            },
            worlds,
            terrains,
            components,
            elements: Arc::new(crate::domain::create_default_element_registry().unwrap()),
            ai_runtime: Arc::new(
                crate::game::ai::AiRuntime::new(crate::config::AiConfig {
                    enabled: false,
                    ..Default::default()
                })
                .unwrap(),
            ),
            organism_instance_storage: Arc::new(instances.clone()),
            organism_model_storage_lock: Arc::new(tokio::sync::Mutex::new(())),
        };
        let app = crate::api::game_router(state);
        let request = |method: &str, path: String, body: &'static str| {
            Request::builder()
                .method(method)
                .uri(path)
                .header("Authorization", "Bearer quota-test")
                .body(Body::from(body))
                .unwrap()
        };
        let (first, second) = tokio::join!(
            app.clone().oneshot(request(
                "PUT",
                format!("{}/files?path=new/helper.py", paths[0].0),
                "x"
            )),
            app.clone().oneshot(request(
                "PUT",
                format!("{}/files?path=new/helper.py", paths[1].0),
                "x"
            ))
        );
        let responses = [first.unwrap(), second.unwrap()];
        let mut statuses = responses
            .iter()
            .map(|response| response.status().as_u16())
            .collect::<Vec<_>>();
        statuses.sort();
        assert_eq!(statuses, [204, 413]);
        let rejected = responses
            .iter()
            .position(|response| response.status() == StatusCode::PAYLOAD_TOO_LARGE)
            .unwrap();
        for response in responses {
            if response.status() == StatusCode::PAYLOAD_TOO_LARGE {
                let body = to_bytes(response.into_body(), 4096).await.unwrap();
                assert_eq!(
                    serde_json::from_slice::<serde_json::Value>(&body).unwrap()["error"]["code"],
                    "STORAGE_QUOTA_EXCEEDED"
                );
            }
        }
        assert!(!paths[rejected].1.join("new").exists());
        let overwrite = app
            .clone()
            .oneshot(request(
                "PUT",
                format!("{}/files?path=main.py", paths[0].0),
                "y",
            ))
            .await
            .unwrap();
        assert_eq!(overwrite.status(), StatusCode::NO_CONTENT);
        let spawn = Request::builder().method("POST").uri(format!("{}/organisms", paths[0].0.split("/organisms/").next().unwrap()))
            .header("Authorization", "Bearer quota-test").header("Content-Type", "application/json")
            .body(Body::from(serde_json::json!({"model_id":model.id,"local_model":{"id":model.id,"name":"quota","composition":model,"files":{},"directories":[]}}).to_string())).unwrap();
        assert_eq!(
            app.clone().oneshot(spawn).await.unwrap().status(),
            StatusCode::PAYLOAD_TOO_LARGE
        );
        assert_eq!(fs::read_dir(instances).unwrap().count(), 2);
        let deleted = app
            .clone()
            .oneshot(request(
                "DELETE",
                format!("{}/entries?path=used.py", paths[0].0),
                "",
            ))
            .await
            .unwrap();
        assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
        let upload = app
            .clone()
            .oneshot(request(
                "PUT",
                format!("{}/files?path=new/helper.py", paths[rejected].0),
                "ok",
            ))
            .await
            .unwrap();
        assert_eq!(upload.status(), StatusCode::NO_CONTENT);
        let size = 51 * 1024 * 1024;
        let large = Request::builder()
            .method("PUT")
            .uri(format!("{}/files?path=large.bin", paths[rejected].0))
            .header("Authorization", "Bearer quota-test")
            .body(Body::from(vec![0x5a; size]))
            .unwrap();
        assert_eq!(
            app.oneshot(large).await.unwrap().status(),
            StatusCode::NO_CONTENT
        );
        let source = &paths[rejected].1;
        assert_eq!(
            fs::metadata(source.join("large.bin")).unwrap().len(),
            size as u64
        );
        use crate::domain::organism::program_storage::{clone_program, fingerprint};
        let original = fingerprint(source).unwrap();
        let cloned = storage.0.join("cloned");
        clone_program(source, &cloned).unwrap();
        assert_eq!(fingerprint(&cloned).unwrap(), original);
        let bundle = crate::transfer::ModelBundle::read(
            uuid::Uuid::new_v4(),
            "large".into(),
            serde_json::json!({}),
            source,
        )
        .unwrap();
        let imported = storage.0.join("imported");
        bundle.write_program(&imported).unwrap();
        assert_eq!(fingerprint(&imported).unwrap(), original);
    }
}
