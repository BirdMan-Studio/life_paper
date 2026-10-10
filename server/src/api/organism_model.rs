use super::{auth::authenticate, error::ApiError};
use crate::{
    db::OrganismModelRecord,
    domain::{
        BiologicalOrganism, ComponentCategory, ComponentInstance, ComponentRegistry,
        OrganismProgramId,
    },
    state::AppState,
};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, StatusCode},
    routing::{delete, get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    path::{Component, PathBuf},
};
use uuid::Uuid;

const MAX_COMPONENTS: usize = 256;
const MAX_FILE_SIZE: usize = 10 * 1024 * 1024;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/organism-models", post(create_model).get(list_models))
        .route("/organism-models/{model_id}", delete(delete_model))
        .route("/organism-models/{model_id}/folder", get(list_folder))
        .route("/organism-models/{model_id}/folders", post(create_folder))
        .route(
            "/organism-models/{model_id}/files",
            get(read_file).put(upload_file),
        )
        .route(
            "/organism-models/{model_id}/entries",
            delete(delete_entry).patch(rename_entry),
        )
        .layer(DefaultBodyLimit::max(MAX_FILE_SIZE))
}

#[derive(Serialize)]
struct DataResponse<T> {
    data: T,
}

#[derive(Deserialize)]
struct CreateModelRequest {
    name: String,
    components: Vec<ComponentRequest>,
    #[serde(default)]
    connections: Vec<ConnectionRequest>,
}

#[derive(Deserialize)]
struct ComponentRequest {
    id: Uuid,
    component_id: String,
}

#[derive(Deserialize)]
struct ConnectionRequest {
    first: Uuid,
    second: Uuid,
}

#[derive(Serialize)]
struct ModelResponse {
    id: Uuid,
    name: String,
    owner_user_id: i64,
    composition: Value,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<OrganismModelRecord> for ModelResponse {
    fn from(record: OrganismModelRecord) -> Self {
        Self {
            id: record.id,
            name: record.name,
            owner_user_id: record.owner_user_id,
            composition: record.composition,
            created_at: record.created_at,
            updated_at: record.updated_at,
        }
    }
}

async fn create_model(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateModelRequest>,
) -> Result<(StatusCode, Json<DataResponse<ModelResponse>>), ApiError> {
    let session = authenticate(&state, &headers).await?;
    let _storage_guard = state.organism_model_storage_lock.lock().await;
    let name = request.name.trim();
    if name.is_empty() || name.chars().count() > 128 {
        return Err(ApiError::BadRequest(
            "模型名称长度必须在 1 到 128 个字符之间".to_string(),
        ));
    }
    if request.components.is_empty() || request.components.len() > MAX_COMPONENTS {
        return Err(ApiError::BadRequest(format!(
            "组件数量必须在 1 到 {MAX_COMPONENTS} 之间"
        )));
    }
    let first_definition = state
        .components
        .get(&request.components[0].component_id)
        .ok_or_else(|| {
            ApiError::BadRequest(format!(
                "未知的生物组件: {}",
                request.components[0].component_id
            ))
        })?;
    if first_definition.category != ComponentCategory::Body {
        return Err(ApiError::BadRequest(
            "首个生物组件必须是身体结构".to_string(),
        ));
    }

    let unlocked = state
        .component_unlocks
        .list_for_user(session.user_id)
        .await
        .map_err(db_error)?;
    let model_id = Uuid::new_v4();
    let mut organism = BiologicalOrganism::new(OrganismProgramId::from_uuid(model_id));
    organism.id = model_id;
    for item in request.components {
        let definition = state.components.get(&item.component_id).ok_or_else(|| {
            ApiError::BadRequest(format!("未知的生物组件: {}", item.component_id))
        })?;
        if !definition.unlock.default_unlocked && !unlocked.contains(definition.id.as_str()) {
            return Err(ApiError::Forbidden);
        }
        if organism
            .components
            .insert(
                item.id,
                ComponentInstance {
                    id: item.id,
                    component: definition.id.clone(),
                },
            )
            .is_some()
        {
            return Err(ApiError::BadRequest("组件实例 ID 不能重复".to_string()));
        }
    }
    for connection in request.connections {
        organism
            .connect(connection.first, connection.second, &state.components)
            .map_err(|error| ApiError::BadRequest(error.to_string()))?;
    }
    if !has_body_and_is_connected(&organism, &state.components) {
        return Err(ApiError::BadRequest(
            "生物模型必须包含身体，且所有组件必须处于同一连接图中".to_string(),
        ));
    }

    let composition = serde_json::to_value(&organism).map_err(|error| {
        tracing::error!(target: "api", %error, "failed to serialize organism model");
        ApiError::Internal
    })?;
    let folder = state.organism_model_storage.join(model_id.to_string());
    tokio::fs::create_dir(&folder).await.map_err(fs_error)?;
    let record = match state
        .organism_models
        .create(model_id, session.user_id, name, composition)
        .await
    {
        Ok(record) => record,
        Err(error) => {
            let _ = tokio::fs::remove_dir_all(&folder).await;
            return Err(db_error(error));
        }
    };

    Ok((
        StatusCode::CREATED,
        Json(DataResponse {
            data: record.into(),
        }),
    ))
}

fn has_body_and_is_connected(organism: &BiologicalOrganism, registry: &ComponentRegistry) -> bool {
    let Some(body_id) = organism.components.iter().find_map(|(id, instance)| {
        registry
            .get(instance.component.as_str())
            .is_some_and(|definition| definition.category == ComponentCategory::Body)
            .then_some(*id)
    }) else {
        return false;
    };

    let mut visited = BTreeSet::from([body_id]);
    let mut pending = vec![body_id];
    while let Some(current) = pending.pop() {
        for connection in &organism.connections {
            let next = if connection.first == current {
                Some(connection.second)
            } else if connection.second == current {
                Some(connection.first)
            } else {
                None
            };
            if let Some(next) = next
                && visited.insert(next)
            {
                pending.push(next);
            }
        }
    }
    visited.len() == organism.components.len()
}

async fn list_models(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<DataResponse<Vec<ModelResponse>>>, ApiError> {
    let session = authenticate(&state, &headers).await?;
    let models = state
        .organism_models
        .list_for_user(session.user_id)
        .await
        .map_err(db_error)?
        .into_iter()
        .map(Into::into)
        .collect();
    Ok(Json(DataResponse { data: models }))
}

#[derive(Deserialize)]
struct EntryPathQuery {
    #[serde(default)]
    path: String,
}

#[derive(Deserialize)]
struct CreateFolderRequest {
    path: String,
}

#[derive(Deserialize)]
struct RenameEntryRequest {
    path: String,
    name: String,
}

#[derive(Serialize)]
struct FolderEntry {
    name: String,
    path: String,
    kind: &'static str,
    size: Option<u64>,
}

async fn list_folder(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(model_id): Path<Uuid>,
    Query(query): Query<EntryPathQuery>,
) -> Result<Json<DataResponse<Vec<FolderEntry>>>, ApiError> {
    let session = authenticate(&state, &headers).await?;
    ensure_owned(&state, model_id, session.user_id).await?;
    let (_, target) = model_path(&state, model_id, &query.path, true)?;
    let mut reader = tokio::fs::read_dir(&target)
        .await
        .map_err(map_entry_error)?;
    let mut entries = Vec::new();
    while let Some(entry) = reader.next_entry().await.map_err(fs_error)? {
        let metadata = entry.metadata().await.map_err(fs_error)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = if query.path.is_empty() {
            name.clone()
        } else {
            format!("{}/{}", query.path.trim_end_matches('/'), name)
        };
        entries.push(FolderEntry {
            name,
            path: path.replace('\\', "/"),
            kind: if metadata.is_dir() { "folder" } else { "file" },
            size: metadata.is_file().then_some(metadata.len()),
        });
    }
    entries.sort_by(|a, b| a.kind.cmp(b.kind).then(a.name.cmp(&b.name)));
    Ok(Json(DataResponse { data: entries }))
}

async fn create_folder(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(model_id): Path<Uuid>,
    Json(request): Json<CreateFolderRequest>,
) -> Result<StatusCode, ApiError> {
    let session = authenticate(&state, &headers).await?;
    let _storage_guard = state.organism_model_storage_lock.lock().await;
    ensure_owned(&state, model_id, session.user_id).await?;
    let (_, target) = model_path(&state, model_id, &request.path, false)?;
    tokio::fs::create_dir_all(target).await.map_err(fs_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn upload_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(model_id): Path<Uuid>,
    Query(query): Query<EntryPathQuery>,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    let session = authenticate(&state, &headers).await?;
    let _storage_guard = state.organism_model_storage_lock.lock().await;
    ensure_owned(&state, model_id, session.user_id).await?;
    let (_, target) = model_path(&state, model_id, &query.path, false)?;
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(fs_error)?;
    }
    tokio::fs::write(target, body).await.map_err(fs_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn read_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(model_id): Path<Uuid>,
    Query(query): Query<EntryPathQuery>,
) -> Result<Bytes, ApiError> {
    let session = authenticate(&state, &headers).await?;
    ensure_owned(&state, model_id, session.user_id).await?;
    let (_, target) = model_path(&state, model_id, &query.path, false)?;
    let metadata = tokio::fs::metadata(&target)
        .await
        .map_err(map_entry_error)?;
    if !metadata.is_file() {
        return Err(ApiError::BadRequest("指定路径不是文件".to_string()));
    }
    tokio::fs::read(target)
        .await
        .map(Bytes::from)
        .map_err(map_entry_error)
}

async fn delete_entry(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(model_id): Path<Uuid>,
    Query(query): Query<EntryPathQuery>,
) -> Result<StatusCode, ApiError> {
    let session = authenticate(&state, &headers).await?;
    let _storage_guard = state.organism_model_storage_lock.lock().await;
    ensure_owned(&state, model_id, session.user_id).await?;
    let (_, target) = model_path(&state, model_id, &query.path, false)?;
    let metadata = tokio::fs::metadata(&target)
        .await
        .map_err(map_entry_error)?;
    if metadata.is_dir() {
        tokio::fs::remove_dir_all(target).await.map_err(fs_error)?;
    } else {
        tokio::fs::remove_file(target).await.map_err(fs_error)?;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn rename_entry(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(model_id): Path<Uuid>,
    Json(request): Json<RenameEntryRequest>,
) -> Result<StatusCode, ApiError> {
    let session = authenticate(&state, &headers).await?;
    let _storage_guard = state.organism_model_storage_lock.lock().await;
    ensure_owned(&state, model_id, session.user_id).await?;
    let (_, source) = model_path(&state, model_id, &request.path, false)?;
    let name = request.name.trim();
    if !is_safe_entry_name(name) {
        return Err(ApiError::BadRequest(
            "名称必须是单个安全路径片段".to_string(),
        ));
    }
    let target = source
        .parent()
        .ok_or_else(|| ApiError::BadRequest("无效的重命名路径".to_string()))?
        .join(name);
    if target == source {
        return Ok(StatusCode::NO_CONTENT);
    }
    match tokio::fs::metadata(&target).await {
        Ok(_) => return Err(ApiError::BadRequest("目标名称已存在".to_string())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(fs_error(error)),
    }
    tokio::fs::rename(source, target)
        .await
        .map_err(map_entry_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_model(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(model_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let session = authenticate(&state, &headers).await?;
    let _storage_guard = state.organism_model_storage_lock.lock().await;
    ensure_owned(&state, model_id, session.user_id).await?;
    let folder = state.organism_model_storage.join(model_id.to_string());
    let tombstone = state
        .organism_model_storage
        .join(format!(".{model_id}.deleting"));
    let moved = match tokio::fs::rename(&folder, &tombstone).await {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(fs_error(error)),
    };
    match state
        .organism_models
        .delete_owned(model_id, session.user_id)
        .await
    {
        Ok(true) => {
            if moved {
                if let Err(error) = tokio::fs::remove_dir_all(tombstone).await {
                    tracing::warn!(target: "api", %error, %model_id, "failed to remove deleted organism model folder");
                }
            }
            Ok(StatusCode::NO_CONTENT)
        }
        Ok(false) => {
            if moved {
                let _ = tokio::fs::rename(tombstone, folder).await;
            }
            Err(ApiError::NotFound)
        }
        Err(error) => {
            if moved {
                let _ = tokio::fs::rename(tombstone, folder).await;
            }
            Err(db_error(error))
        }
    }
}

async fn ensure_owned(state: &AppState, model_id: Uuid, user_id: i64) -> Result<(), ApiError> {
    state
        .organism_models
        .find_owned(model_id, user_id)
        .await
        .map_err(db_error)?
        .map(|_| ())
        .ok_or(ApiError::NotFound)
}

fn model_path(
    state: &AppState,
    model_id: Uuid,
    path: &str,
    allow_root: bool,
) -> Result<(PathBuf, PathBuf), ApiError> {
    let relative = safe_relative_path(path, allow_root)?;
    let root = state.organism_model_storage.join(model_id.to_string());
    Ok((root.clone(), root.join(relative)))
}

fn safe_relative_path(path: &str, allow_root: bool) -> Result<PathBuf, ApiError> {
    let relative = PathBuf::from(path.replace('\\', "/"));
    if (!allow_root && relative.as_os_str().is_empty())
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(ApiError::BadRequest(
            "路径必须是模型目录内的安全相对路径".to_string(),
        ));
    }
    Ok(relative)
}

fn is_safe_entry_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains('/') && !name.contains('\\')
}

fn db_error(error: sqlx::Error) -> ApiError {
    tracing::error!(target: "api", %error, "organism model database operation failed");
    ApiError::Internal
}

fn fs_error(error: std::io::Error) -> ApiError {
    tracing::error!(target: "api", %error, "organism model file operation failed");
    ApiError::Internal
}

fn map_entry_error(error: std::io::Error) -> ApiError {
    if error.kind() == std::io::ErrorKind::NotFound {
        ApiError::NotFound
    } else {
        fs_error(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_nested_model_paths() {
        assert_eq!(
            safe_relative_path("src/brain/main.py", false).unwrap(),
            PathBuf::from("src/brain/main.py")
        );
        assert!(safe_relative_path("", true).unwrap().as_os_str().is_empty());
    }

    #[test]
    fn rejects_paths_outside_model_root() {
        assert!(safe_relative_path("../secret", false).is_err());
        assert!(safe_relative_path("/absolute/path", false).is_err());
        assert!(safe_relative_path("", false).is_err());
        assert!(safe_relative_path("src/../../secret", false).is_err());
    }

    #[test]
    fn validates_rename_entry_names() {
        assert!(is_safe_entry_name("brain.rs"));
        assert!(is_safe_entry_name("感知器"));
        assert!(!is_safe_entry_name(""));
        assert!(!is_safe_entry_name(".."));
        assert!(!is_safe_entry_name("src/main.rs"));
        assert!(!is_safe_entry_name("src\\main.rs"));
    }

    #[test]
    fn requires_every_component_to_be_connected_to_one_body() {
        let registry = crate::domain::create_default_component_registry().unwrap();
        let mut organism = BiologicalOrganism::new(OrganismProgramId::new());
        let body = organism.add_component("small_body_1", &registry).unwrap();
        let sensor = organism
            .add_component("photosensor_organ_1", &registry)
            .unwrap();

        assert!(!has_body_and_is_connected(&organism, &registry));
        organism.connect(body, sensor, &registry).unwrap();
        assert!(has_body_and_is_connected(&organism, &registry));
    }

    #[test]
    fn allows_multiple_bodies_connected_through_a_compatible_component() {
        let mut registry = crate::domain::create_default_component_registry().unwrap();
        registry
            .register(
                "body_bridge_test",
                "身体连接结构",
                ComponentCategory::InternalOrgan,
                crate::domain::ComponentSlots::new().with(ComponentCategory::Body, 2),
                None,
            )
            .unwrap();
        let mut organism = BiologicalOrganism::new(OrganismProgramId::new());
        let first_body = organism.add_component("small_body_1", &registry).unwrap();
        let bridge = organism
            .add_component("body_bridge_test", &registry)
            .unwrap();
        let second_body = organism.add_component("small_body_1", &registry).unwrap();

        organism.connect(first_body, bridge, &registry).unwrap();
        organism.connect(bridge, second_body, &registry).unwrap();

        assert!(has_body_and_is_connected(&organism, &registry));
    }
}
