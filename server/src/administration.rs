use crate::{
    api::error::ApiError,
    game_state::{GameIdentity, GameState},
    observability::{LogQuery, LogStore},
    state::AppState,
};
use axum::{
    Json, Router,
    extract::{Path, Query, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

pub struct AdminService {
    pub(crate) relay: crate::relay::RelayService,
    pub pool: PgPool,
    pub logs: Arc<LogStore>,
    token: String,
    directory: PathBuf,
    dist: PathBuf,
    pub(crate) servers: Mutex<Vec<GameServer>>,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct GameServer {
    pub id: String,
    pub name: String,
    pub api_url: String,
    #[serde(default)]
    pub registration_key_hash: Option<String>,
    #[serde(default)]
    pub last_seen: Option<i64>,
    #[serde(default)]
    pub worlds: Vec<crate::game_registry::WorldAnnouncement>,
}

pub(crate) fn load_admin_token(
    directory: &std::path::Path,
    configured: Option<String>,
) -> anyhow::Result<String> {
    fn validate(token: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            (32..=256).contains(&token.len()) && token.bytes().all(|v| v.is_ascii_graphic()),
            "admin token must contain 32..256 visible ASCII characters"
        );
        Ok(())
    }
    if let Some(token) = configured.filter(|value| !value.is_empty()) {
        validate(&token)?;
        return Ok(token);
    }
    std::fs::create_dir_all(directory)?;
    let path = directory.join("admin-token.txt");
    match std::fs::read_to_string(&path) {
        Ok(value) => {
            let token = value.trim_end_matches(['\r', '\n']).to_owned();
            validate(&token)
                .map_err(|_| anyhow::anyhow!("invalid admin token file: {}", path.display()))?;
            return Ok(token);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    use base64::Engine;
    use ring::rand::SecureRandom;
    use std::io::Write;
    let mut bytes = [0u8; 32];
    ring::rand::SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| anyhow::anyhow!("cannot generate admin token"))?;
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(&path) {
        Ok(mut file) => {
            file.write_all(token.as_bytes())?;
            file.sync_all()?;
            Ok(token)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            // Another process created the credential; never overwrite it.
            let value = std::fs::read_to_string(path)?;
            let token = value.trim_end_matches(['\r', '\n']).to_owned();
            validate(&token)?;
            Ok(token)
        }
        Err(error) => Err(error.into()),
    }
}

impl AdminService {
    pub fn new(
        pool: PgPool,
        logs: Arc<LogStore>,
        directory: PathBuf,
        dist: PathBuf,
        token: Option<String>,
    ) -> anyhow::Result<Self> {
        let token = load_admin_token(&directory, token)?;
        std::fs::create_dir_all(&directory)?;
        let mut servers: Vec<GameServer> = match std::fs::read(directory.join("game-servers.json"))
        {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error.into()),
        };
        for server in &mut servers {
            server.last_seen = None;
            server.worlds.clear();
        }
        Ok(Self {
            relay: crate::relay::RelayService::new()?,
            pool,
            logs,
            directory,
            dist,
            token,
            servers: Mutex::new(servers),
        })
    }
    fn authenticate(&self, headers: &HeaderMap) -> Result<(), ApiError> {
        let expected = &self.token;
        let actual = bearer(headers)?;
        use sha2::{Digest, Sha256};
        let a = Sha256::digest(expected.as_bytes());
        let b = Sha256::digest(actual.as_bytes());
        if a.iter().zip(b.iter()).fold(0u8, |v, (a, b)| v | (a ^ b)) != 0 {
            return Err(ApiError::Unauthorized);
        }
        Ok(())
    }
    pub(crate) fn save_servers(&self, servers: &[GameServer]) -> Result<(), ApiError> {
        let temporary = self.directory.join("game-servers.tmp");
        std::fs::write(
            &temporary,
            serde_json::to_vec_pretty(servers).map_err(|_| ApiError::Internal)?,
        )
        .map_err(io_error)?;
        std::fs::rename(temporary, self.directory.join("game-servers.json")).map_err(io_error)
    }
}
fn bearer(headers: &HeaderMap) -> Result<&str, ApiError> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|v| v.len() <= 8192)
        .ok_or(ApiError::Unauthorized)
}
fn db_error(error: sqlx::Error) -> ApiError {
    if let sqlx::Error::Database(ref db) = error {
        if db.is_unique_violation() {
            return ApiError::Conflict("用户名、邮箱或记录已存在".into());
        }
        if db.is_foreign_key_violation() {
            return ApiError::BadRequest("所属用户不存在".into());
        }
    }
    tracing::error!(%error, "admin database operation failed");
    ApiError::Internal
}
fn io_error(error: std::io::Error) -> ApiError {
    tracing::error!(%error, "admin storage operation failed");
    ApiError::Internal
}
fn data(value: Value) -> Json<Value> {
    Json(json!({"data": value}))
}

pub fn routes(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/admin/session", get(session))
        .route(
            "/api/v1/admin/records/{kind}",
            get(list_records).post(create_record),
        )
        .route(
            "/api/v1/admin/records/{kind}/{id}",
            axum::routing::put(update_record).delete(delete_record),
        )
        .route("/api/v1/admin/catalog", get(catalog))
        .route("/api/v1/admin/logs", get(logs))
        .route("/api/v1/admin/servers", get(servers).post(save_server))
        .route(
            "/api/v1/admin/servers/{id}",
            axum::routing::put(edit_server).delete(delete_server),
        )
        .route("/api/v1/admin/servers/{id}/ticket", post(ticket))
        .route(
            "/api/v1/admin/servers/{id}/registration-key",
            post(crate::game_registry::rotate_key),
        )
        .route_layer(middleware::from_fn_with_state(state.clone(), authorize))
        .route("/admin", get(|| async { Redirect::permanent("/admin/") }))
        .route("/admin/", get(page))
        .route("/assets/{file}", get(asset))
        .with_state(state)
}
async fn authorize(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    state.admin.authenticate(request.headers())?;
    let method = request.method().to_string();
    let path = request.uri().path().to_owned();
    let response = next.run(request).await;
    if method != "GET" {
        state.admin.logs.push(
            "INFO",
            "audit",
            "admin",
            json!({"method": method, "path": path, "status": response.status().as_u16()}),
        );
    }
    Ok(response)
}
async fn session() -> Json<Value> {
    data(json!({"role": "administrator"}))
}
async fn page(State(state): State<AppState>) -> Response {
    static_file(
        &state.admin.dist.join("admin/index.html"),
        "text/html; charset=utf-8",
    )
    .await
}
async fn asset(State(state): State<AppState>, Path(file): Path<String>) -> Response {
    if !file
        .bytes()
        .all(|v| v.is_ascii_alphanumeric() || b"-_.".contains(&v))
        || file.starts_with('.')
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    let mime = if file.ends_with(".js") {
        "text/javascript"
    } else if file.ends_with(".css") {
        "text/css"
    } else {
        return StatusCode::NOT_FOUND.into_response();
    };
    static_file(&state.admin.dist.join("assets").join(file), mime).await
}
async fn static_file(path: &std::path::Path, mime: &str) -> Response {
    match tokio::fs::read(path).await {
        Ok(bytes) => (
            [
                ("content-type", mime),
                ("cache-control", "no-store"),
                ("x-content-type-options", "nosniff"),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            "Admin UI missing. Run npm run build in client and configure LIFE_PAPER_ADMIN_DIST.",
        )
            .into_response(),
    }
}
async fn logs(State(state): State<AppState>, Query(query): Query<LogQuery>) -> Json<Value> {
    data(state.admin.logs.query(&query))
}
async fn catalog(State(state): State<AppState>) -> Json<Value> {
    data(
        json!({"components": state.components.iter().collect::<Vec<_>>(), "elements": state.elements.iter().collect::<Vec<_>>() }),
    )
}
#[derive(Default, Deserialize)]
struct ListQuery {
    #[serde(default)]
    search: String,
    #[serde(default)]
    offset: i64,
    limit: Option<i64>,
}
async fn list_records(
    State(state): State<AppState>,
    Path(kind): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>, ApiError> {
    let (table, filter, row) = match kind.as_str() {
        "users" => (
            "users u",
            "u.username ILIKE $1 OR u.email ILIKE $1 OR u.id::text = $2",
            "jsonb_build_object('id',u.id::text,'username',u.username,'email',u.email,'energy_coins',COALESCE((SELECT energy_coins::text FROM player_wallets w WHERE w.user_id=u.id),'0'),'gold_coins',COALESCE((SELECT gold_coins::text FROM player_wallets w WHERE w.user_id=u.id),'0'),'bio_soup',COALESCE((SELECT jsonb_object_agg(element_id,amount::text) FROM player_bio_soup s WHERE s.user_id=u.id),'{}'::jsonb),'unlocked_components',COALESCE((SELECT jsonb_agg(component_id ORDER BY component_id) FROM user_unlocked_components c WHERE c.user_id=u.id),'[]'::jsonb))",
        ),
        "models" => (
            "organism_models u",
            "u.name ILIKE $1 OR u.owner_user_id::text = $2 OR u.id::text = $2",
            "jsonb_build_object('id',u.id,'owner_user_id',u.owner_user_id::text,'name',u.name,'composition',u.composition,'created_at',u.created_at,'updated_at',u.updated_at)",
        ),
        _ => return Err(ApiError::NotFound),
    };
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let offset = query.offset.max(0);
    let search = format!("%{}%", query.search.chars().take(200).collect::<String>());
    let total =
        sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {table} WHERE {filter}"))
            .bind(&search)
            .bind(&query.search)
            .fetch_one(&state.admin.pool)
            .await
            .map_err(db_error)?;
    let rows = sqlx::query_scalar::<_, Value>(&format!(
        "SELECT {row} FROM {table} WHERE {filter} ORDER BY u.id LIMIT $3 OFFSET $4"
    ))
    .bind(search)
    .bind(query.search)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.admin.pool)
    .await
    .map_err(db_error)?;
    Ok(data(
        json!({"records": rows, "total": total, "offset": offset, "limit": limit}),
    ))
}

#[derive(Deserialize)]
struct UserInput {
    username: String,
    email: String,
    #[serde(default)]
    password: String,
    #[serde(default = "zero")]
    energy_coins: String,
    #[serde(default = "zero")]
    gold_coins: String,
    #[serde(default)]
    bio_soup: BTreeMap<String, String>,
    #[serde(default)]
    unlocked_components: Vec<String>,
}
fn zero() -> String {
    "0".into()
}
fn amount(value: &str) -> Result<i64, ApiError> {
    value
        .parse::<i64>()
        .ok()
        .filter(|v| *v >= 0)
        .ok_or_else(|| ApiError::BadRequest("数量必须为非负整数".into()))
}
async fn save_user(state: &AppState, id: Option<i64>, body: Value) -> Result<Value, ApiError> {
    let input: UserInput =
        serde_json::from_value(body).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    if input.username.trim().is_empty()
        || input.username.chars().count() > 64
        || !input.email.contains('@')
        || input.email.len() > 254
    {
        return Err(ApiError::BadRequest("请填写有效的用户名和邮箱".into()));
    }
    if (id.is_none() || !input.password.is_empty()) && !(8..=1024).contains(&input.password.len()) {
        return Err(ApiError::BadRequest("密码长度必须为8至1024字节".into()));
    }
    let energy = amount(&input.energy_coins)?;
    let gold = amount(&input.gold_coins)?;
    for (element, value) in &input.bio_soup {
        if state.elements.get(element).is_none() {
            return Err(ApiError::BadRequest(format!("未知物质: {element}")));
        }
        amount(value)?;
    }
    for component in &input.unlocked_components {
        if state.components.get(component).is_none() {
            return Err(ApiError::BadRequest(format!("未知组件: {component}")));
        }
    }
    let hash = if input.password.is_empty() {
        None
    } else {
        Some(crate::api::auth::hash_password(input.password).await?)
    };
    if let Some(id) = id {
        if hash.is_some() {
            crate::api::auth::revoke_all_sessions(state, id).await?;
        }
    }
    let mut tx = state.admin.pool.begin().await.map_err(db_error)?;
    let id = if let Some(id) = id {
        let changed = sqlx::query("UPDATE users SET username=$2,email=$3,password_hash=COALESCE($4,password_hash) WHERE id=$1").bind(id).bind(input.username.trim()).bind(input.email.trim()).bind(hash).execute(&mut *tx).await.map_err(db_error)?;
        if changed.rows_affected() == 0 {
            return Err(ApiError::NotFound);
        }
        id
    } else {
        sqlx::query_scalar::<_, i64>(
            "INSERT INTO users(username,email,password_hash) VALUES($1,$2,$3) RETURNING id",
        )
        .bind(input.username.trim())
        .bind(input.email.trim())
        .bind(hash)
        .fetch_one(&mut *tx)
        .await
        .map_err(db_error)?
    };
    sqlx::query("INSERT INTO player_wallets(user_id,energy_coins,gold_coins) VALUES($1,$2,$3) ON CONFLICT(user_id) DO UPDATE SET energy_coins=$2,gold_coins=$3,updated_at=now()")
        .bind(id).bind(energy).bind(gold).execute(&mut *tx).await.map_err(db_error)?;
    sqlx::query("DELETE FROM player_bio_soup WHERE user_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    for (element, value) in input.bio_soup {
        sqlx::query("INSERT INTO player_bio_soup(user_id,element_id,amount) VALUES($1,$2,$3)")
            .bind(id)
            .bind(element)
            .bind(amount(&value)?)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
    }
    sqlx::query("DELETE FROM user_unlocked_components WHERE user_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    for component in input.unlocked_components {
        sqlx::query("INSERT INTO user_unlocked_components(user_id,component_id) VALUES($1,$2) ON CONFLICT DO NOTHING").bind(id).bind(component).execute(&mut *tx).await.map_err(db_error)?;
    }
    tx.commit().await.map_err(db_error)?;
    Ok(json!({"id": id.to_string()}))
}

#[derive(Deserialize)]
struct ModelInput {
    owner_user_id: String,
    name: String,
    composition: Value,
}
async fn save_model(state: &AppState, id: Option<Uuid>, body: Value) -> Result<Value, ApiError> {
    let input: ModelInput =
        serde_json::from_value(body).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let owner = input
        .owner_user_id
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| ApiError::BadRequest("所属用户ID无效".into()))?;
    if input.name.trim().is_empty() || input.name.chars().count() > 128 {
        return Err(ApiError::BadRequest(
            "模型名称长度必须为1至128个字符".into(),
        ));
    }
    let create = id.is_none();
    let id = id.unwrap_or_else(Uuid::new_v4);
    let mut model: crate::domain::BiologicalOrganism = serde_json::from_value(input.composition)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    model.id = id;
    for (key, part) in &model.components {
        if *key != part.id {
            return Err(ApiError::BadRequest("组件实例ID与索引不一致".into()));
        }
    }
    for connection in std::mem::take(&mut model.connections) {
        model
            .connect(connection.first, connection.second, &state.components)
            .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    }
    if model.components.len() > 256
        || !crate::api::organism_model::has_body_and_is_connected(&model, &state.components)
    {
        return Err(ApiError::BadRequest(
            "模型须包含身体，且所有组件必须连接；最多256个组件".into(),
        ));
    }
    crate::domain::organism_component::normalize_placements(&mut model, &state.components)
        .map_err(ApiError::BadRequest)?;
    crate::domain::organism::OrganismInstance::from_model(
        id,
        owner,
        input.name.clone(),
        model.clone(),
        &state.components,
    )
    .map_err(ApiError::BadRequest)?;
    let composition = serde_json::to_value(model).map_err(|_| ApiError::Internal)?;
    let _guard = state.organism_model_storage_lock.lock().await;
    if create {
        let folder = state.organism_model_storage.join(id.to_string());
        tokio::fs::create_dir(&folder).await.map_err(io_error)?;
        if let Err(error) = state
            .organism_models
            .create(id, owner, input.name.trim(), composition)
            .await
        {
            let _ = tokio::fs::remove_dir_all(folder).await;
            return Err(db_error(error));
        }
    } else {
        let result = sqlx::query("UPDATE organism_models SET owner_user_id=$2,name=$3,composition=$4,updated_at=now() WHERE id=$1")
            .bind(id).bind(owner).bind(input.name.trim()).bind(composition).execute(&state.admin.pool).await.map_err(db_error)?;
        if result.rows_affected() == 0 {
            return Err(ApiError::NotFound);
        }
    }
    Ok(json!({"id": id}))
}
async fn create_record(
    State(state): State<AppState>,
    Path(kind): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    Ok(data(match kind.as_str() {
        "users" => save_user(&state, None, body).await?,
        "models" => save_model(&state, None, body).await?,
        _ => return Err(ApiError::NotFound),
    }))
}
async fn update_record(
    State(state): State<AppState>,
    Path((kind, id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    Ok(data(match kind.as_str() {
        "users" => {
            save_user(
                &state,
                Some(id.parse().map_err(|_| ApiError::NotFound)?),
                body,
            )
            .await?
        }
        "models" => {
            save_model(
                &state,
                Some(id.parse().map_err(|_| ApiError::NotFound)?),
                body,
            )
            .await?
        }
        _ => return Err(ApiError::NotFound),
    }))
}
async fn delete_record(
    State(state): State<AppState>,
    Path((kind, id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let _guard = state.organism_model_storage_lock.lock().await;
    let model_ids = match kind.as_str() {
        "users" => {
            let user_id = id.parse::<i64>().map_err(|_| ApiError::NotFound)?;
            crate::api::auth::revoke_all_sessions(&state, user_id).await?;
            let models = state
                .organism_models
                .list_for_user(user_id)
                .await
                .map_err(db_error)?;
            let instance_folders = crate::api::auth::account_instance_folders(&state, user_id).await?;
            if !state.users.delete(user_id).await.map_err(db_error)? {
                return Err(ApiError::NotFound);
            }
            crate::api::auth::remove_account_worlds(&state, user_id, instance_folders).await?;
            models.into_iter().map(|m| m.id).collect::<Vec<_>>()
        }
        "models" => {
            let id = id.parse::<Uuid>().map_err(|_| ApiError::NotFound)?;
            let result = sqlx::query("DELETE FROM organism_models WHERE id=$1")
                .bind(id)
                .execute(&state.admin.pool)
                .await
                .map_err(db_error)?;
            if result.rows_affected() == 0 {
                return Err(ApiError::NotFound);
            }
            vec![id]
        }
        _ => return Err(ApiError::NotFound),
    };
    for id in model_ids {
        if let Err(error) =
            tokio::fs::remove_dir_all(state.organism_model_storage.join(id.to_string())).await
        {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(%error, %id, "admin model cleanup failed");
            }
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

fn validate_server(server: &GameServer) -> Result<(), ApiError> {
    let uri = server
        .api_url
        .parse::<axum::http::Uri>()
        .map_err(|_| ApiError::BadRequest("游戏服地址无效".into()))?;
    if server.id.is_empty()
        || server.id.len() > 128
        || !server
            .id
            .bytes()
            .all(|v| v.is_ascii_alphanumeric() || b"-_".contains(&v))
        || server.name.trim().is_empty()
        || server.name.chars().count() > 128
        || !matches!(uri.scheme_str(), Some("http" | "https"))
        || uri.host().is_none()
        || uri.authority().is_some_and(|a| a.as_str().contains('@'))
        || uri.query().is_some()
        || uri.path() != "/api/v1"
    {
        return Err(ApiError::BadRequest(
            "填写有效的游戏服ID、名称及 http(s)://主机:端口/api/v1 地址".into(),
        ));
    }
    Ok(())
}
async fn servers(State(state): State<AppState>) -> Json<Value> {
    data(json!(
        state
            .admin
            .servers
            .lock()
            .unwrap()
            .iter()
            .map(crate::game_registry::public_server)
            .collect::<Vec<_>>()
    ))
}
async fn mutate_server(
    state: &AppState,
    id: Option<String>,
    mut server: Option<GameServer>,
) -> Result<Json<Value>, ApiError> {
    if let Some(s) = &server {
        validate_server(s)?;
    }
    if let Some(s) = &mut server {
        s.registration_key_hash = None;
        s.last_seen = None;
        s.worlds.clear();
    }
    let mut guard = state.admin.servers.lock().unwrap();
    let mut updated = guard.clone();
    if let Some(id) = &id {
        let position = updated
            .iter()
            .position(|s| s.id == *id)
            .ok_or(ApiError::NotFound)?;
        if server.as_ref().is_some_and(|s| s.id != *id) {
            return Err(ApiError::BadRequest("游戏服ID不能修改".into()));
        }
        let previous = updated.remove(position);
        if let Some(server) = &mut server {
            server.registration_key_hash = previous.registration_key_hash;
            if server.api_url == previous.api_url {
                server.last_seen = previous.last_seen;
                server.worlds = previous.worlds;
            }
        }
    }
    let mut registration_key = None;
    if let Some(server) = server {
        let mut server = server;
        if id.is_none() {
            let key = crate::game_registry::generate_key()?;
            server.registration_key_hash = Some(crate::game_registry::hash_key(&key));
            server.last_seen = None;
            server.worlds.clear();
            registration_key = Some(key);
        }
        if updated.iter().any(|s| s.id == server.id) {
            return Err(ApiError::Conflict("游戏服ID已存在".into()));
        }
        if updated.len() >= 128 {
            return Err(ApiError::BadRequest("游戏服登记上限为128".into()));
        }
        updated.push(server);
    }
    state.admin.save_servers(&updated)?;
    *guard = updated;
    Ok(data(
        json!({"saved": true, "registration_key": registration_key}),
    ))
}
async fn save_server(
    State(state): State<AppState>,
    Json(server): Json<GameServer>,
) -> Result<Json<Value>, ApiError> {
    mutate_server(&state, None, Some(server)).await
}
async fn edit_server(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(server): Json<GameServer>,
) -> Result<Json<Value>, ApiError> {
    mutate_server(&state, Some(id), Some(server)).await
}
async fn delete_server(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    mutate_server(&state, Some(id), None).await
}
async fn ticket(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let server = state.admin.connected_server(&id)?;
    let token = crate::transfer::sign(&state.signing_key, "game-admin", &id, 1, ())
        .map_err(|_| ApiError::Internal)?;
    Ok(data(
        json!({"token": token, "api_url": server.api_url, "relay_url":crate::relay::relay_path(&server.id),"expires_in": 900}),
    ))
}

#[derive(Clone)]
struct GameAdmin {
    game: GameState,
    logs: Arc<LogStore>,
}
pub fn game_routes(game: GameState, logs: Arc<LogStore>) -> Router {
    let state = GameAdmin { game, logs };
    Router::new()
        .route("/api/v1/management/logs", get(game_logs))
        .route(
            "/api/v1/management/worlds",
            get(game_worlds).post(game_create_world),
        )
        .route(
            "/api/v1/management/worlds/{id}",
            get(game_read_world)
                .patch(game_update_world)
                .delete(game_delete_world),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            game_authorize,
        ))
        .with_state(state)
}
async fn game_authorize(
    State(state): State<GameAdmin>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    match &state.game.identity {
        GameIdentity::Signed {
            server_id,
            public_key,
        } => {
            crate::transfer::verify::<()>(
                bearer(request.headers())?,
                public_key,
                "game-admin",
                server_id,
            )
            .map_err(|_| ApiError::Unauthorized)?;
        }
        _ => return Err(ApiError::Forbidden),
    }
    let method = request.method().to_string();
    let path = request.uri().path().to_owned();
    let response = next.run(request).await;
    if method != "GET" {
        state.logs.push(
            "INFO",
            "audit",
            "game-admin",
            json!({"method":method,"path":path,"status":response.status().as_u16()}),
        );
    }
    Ok(response)
}
async fn game_logs(State(state): State<GameAdmin>, Query(query): Query<LogQuery>) -> Json<Value> {
    data(state.logs.query(&query))
}
async fn game_worlds(State(state): State<GameAdmin>) -> Json<Value> {
    data(json!(state.game.worlds.list_all().iter().map(|w| json!({"id":w.id,"name":w.name,"owner_user_id":w.owner_user_id.to_string(),"level_id":w.level_id,"status":w.status()})).collect::<Vec<_>>()))
}
#[derive(Deserialize)]
struct WorldInput {
    owner_user_id: String,
    name: String,
    level_id: String,
}
async fn game_create_world(
    State(state): State<GameAdmin>,
    Json(input): Json<WorldInput>,
) -> Result<Json<Value>, ApiError> {
    let owner = input
        .owner_user_id
        .parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| ApiError::BadRequest("所属用户ID无效".into()))?;
    let world = tokio::task::spawn_blocking(move || {
        state
            .game
            .worlds
            .create_player_world(owner, input.level_id, input.name)
    })
    .await
    .map_err(|_| ApiError::Internal)?
    .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok(data(json!({"id":world.id})))
}
async fn game_read_world(
    State(state): State<GameAdmin>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let world = state.game.worlds.get(id).ok_or(ApiError::NotFound)?;
    let snapshot =
        tokio::task::spawn_blocking(move || state.game.worlds.read_world(id, world.owner_user_id))
            .await
            .map_err(|_| ApiError::Internal)?
            .map_err(|_| ApiError::Internal)?;
    Ok(data(json!({"organisms": snapshot.organisms})))
}
#[derive(Deserialize)]
struct WorldUpdate {
    status: crate::domain::WorldStatus,
}
async fn game_update_world(
    State(state): State<GameAdmin>,
    Path(id): Path<Uuid>,
    Json(input): Json<WorldUpdate>,
) -> Result<StatusCode, ApiError> {
    if state.game.worlds.get(id).is_none() {
        return Err(ApiError::NotFound);
    }
    tokio::task::spawn_blocking(move || state.game.worlds.set_status(id, input.status))
        .await
        .map_err(|_| ApiError::Internal)?
        .ok_or(ApiError::Internal)?;
    Ok(StatusCode::NO_CONTENT)
}
async fn game_delete_world(
    State(state): State<GameAdmin>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let _guard = state
        .game
        .organism_model_storage_lock
        .clone()
        .lock_owned()
        .await;
    tokio::task::spawn_blocking(move || {
        let world = state.game.worlds.get(id).ok_or(ApiError::NotFound)?;
        let snapshot = state
            .game
            .worlds
            .read_world(id, world.owner_user_id)
            .map_err(|_| ApiError::Internal)?;
        if !state.game.worlds.admin_delete(id).map_err(io_error)? {
            return Err(ApiError::NotFound);
        }
        for organism in snapshot.organisms.values() {
            if let Err(error) = std::fs::remove_dir_all(
                state
                    .game
                    .organism_instance_storage
                    .join(organism.id.to_string()),
            ) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(%error, "deleted world instance cleanup failed");
                }
            }
        }
        Ok::<_, ApiError>(())
    })
    .await
    .map_err(|_| ApiError::Internal)??;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_token_is_generated_persisted_and_overridden_explicitly() {
        let root = std::env::temp_dir().join(format!("life-admin-token-{}", Uuid::new_v4()));
        let token = load_admin_token(&root, None).unwrap();
        assert_eq!(token.len(), 43);
        assert_eq!(
            std::fs::read_to_string(root.join("admin-token.txt")).unwrap(),
            token
        );
        assert_eq!(load_admin_token(&root, None).unwrap(), token);
        assert_eq!(load_admin_token(&root, Some(String::new())).unwrap(), token);
        let explicit = "explicit-admin-token-01234567890123456789";
        assert_eq!(
            load_admin_token(&root, Some(explicit.into())).unwrap(),
            explicit
        );
        assert_eq!(
            std::fs::read_to_string(root.join("admin-token.txt")).unwrap(),
            token
        );
        assert!(load_admin_token(&root, Some("short".into())).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(root.join("admin-token.txt"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        std::fs::write(root.join("admin-token.txt"), "invalid").unwrap();
        assert!(load_admin_token(&root, None).is_err());
        assert_eq!(
            std::fs::read_to_string(root.join("admin-token.txt")).unwrap(),
            "invalid"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL configured by LIFE_PAPER_ADMIN_TEST_CONFIG"]
    async fn postgres_admin_crud_and_asset_validation() -> anyhow::Result<()> {
        let path =
            std::env::var("LIFE_PAPER_ADMIN_TEST_CONFIG").unwrap_or_else(|_| "config.toml".into());
        let config: crate::config::Config = toml::from_str(&std::fs::read_to_string(path)?)?;
        let options = sqlx::postgres::PgConnectOptions::new()
            .host(&config.postgres.host)
            .port(config.postgres.port)
            .username(&config.postgres.user)
            .password(&config.postgres.password)
            .database(&config.postgres.database);
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect_with(options)
            .await?;
        let root = std::env::temp_dir().join(format!("life-admin-db-{}", Uuid::new_v4()));
        std::fs::create_dir_all(root.join("models"))?;
        let terrains = Arc::new(crate::domain::create_default_terrain_registry()?);
        let logs = LogStore::open(root.join("logs"))?;
        let state = AppState {
            admin: Arc::new(AdminService::new(
                pool.clone(),
                logs,
                root.join("admin"),
                root.join("dist"),
                None,
            )?),
            central_only: true,
            signing_key: Arc::new(crate::transfer::tests::key()),
            ai_runtime: Arc::new(crate::game::ai::AiRuntime::new(
                crate::config::AiConfig::default(),
            )?),
            users: crate::db::UserRepository::new(pool.clone()),
            player_data: crate::db::PlayerDataRepository::new(pool.clone()),
            component_unlocks: crate::db::ComponentUnlockRepository::new(pool.clone()),
            organism_models: crate::db::OrganismModelRepository::new(pool.clone()),
            organism_model_storage: Arc::new(root.join("models")),
            organism_instance_storage: Arc::new(root.join("instances")),
            organism_model_storage_lock: Arc::new(tokio::sync::Mutex::new(())),
            redis_pool: crate::db::redis_pool::create_pool(&config.redis)?,
            elements: Arc::new(crate::domain::create_default_element_registry()?),
            components: Arc::new(crate::domain::create_default_component_registry()?),
            worlds: Arc::new(crate::domain::WorldDirectory::new(terrains.clone())),
            terrains,
        };
        let username = format!("admin_test_{}", Uuid::new_v4().simple());
        let mut user = json!({"username":username,"email":format!("{username}@invalid.local"),"password":"test-password-only","energy_coins":"9007199254740995","gold_coins":"10","bio_soup":{"water":"123"},"unlocked_components":["small_body_1"]});
        let created = save_user(&state, None, user.clone())
            .await
            .map_err(|e| anyhow::anyhow!("create user: {e:?}"))?;
        let id = created["id"].as_str().unwrap().parse::<i64>()?;
        let result: anyhow::Result<()> = async {
            user["password"] = json!("");
            user["energy_coins"] = json!("22");
            save_user(&state, Some(id), user.clone())
                .await
                .map_err(|e| anyhow::anyhow!("update user: {e:?}"))?;
            let listed = list_records(
                State(state.clone()),
                Path("users".into()),
                Query(ListQuery {
                    search: username.clone(),
                    ..Default::default()
                }),
            )
            .await
            .map_err(|e| anyhow::anyhow!("list users: {e:?}"))?
            .0;
            assert_eq!(listed["data"]["records"][0]["energy_coins"], "22");
            assert_eq!(listed["data"]["records"][0]["bio_soup"]["water"], "123");
            user["bio_soup"] = json!({"unknown-matter":"5"});
            assert!(matches!(
                save_user(&state, Some(id), user).await,
                Err(ApiError::BadRequest(_))
            ));
            assert_eq!(state.player_data.get(id).await?.energy_coins, 22);
            let mut model =
                crate::domain::BiologicalOrganism::new(crate::domain::OrganismProgramId::new());
            model.add_component("small_body_1", &state.components)?;
            let mut body =
                json!({"name":"Admin model","owner_user_id":id.to_string(),"composition":model});
            let saved = save_model(&state, None, body.clone())
                .await
                .map_err(|e| anyhow::anyhow!("create model: {e:?}"))?;
            let model_id: Uuid = saved["id"].as_str().unwrap().parse()?;
            body["name"] = json!("Updated model");
            save_model(&state, Some(model_id), body)
                .await
                .map_err(|e| anyhow::anyhow!("update model: {e:?}"))?;
            let models = list_records(
                State(state.clone()),
                Path("models".into()),
                Query(ListQuery {
                    search: id.to_string(),
                    ..Default::default()
                }),
            )
            .await
            .map_err(|e| anyhow::anyhow!("list models: {e:?}"))?
            .0;
            assert_eq!(models["data"]["records"][0]["name"], "Updated model");
            delete_record(
                State(state.clone()),
                Path(("models".into(), model_id.to_string())),
            )
            .await
            .map_err(|e| anyhow::anyhow!("delete model: {e:?}"))?;
            assert!(!root.join("models").join(model_id.to_string()).exists());
            assert!(
                state
                    .organism_models
                    .find_owned(model_id, id)
                    .await?
                    .is_none()
            );
            Ok(())
        }
        .await;
        sqlx::query("DELETE FROM users WHERE id=$1")
            .bind(id)
            .execute(&pool)
            .await?;
        drop(state);
        std::fs::remove_dir_all(root)?;
        pool.close().await;
        result
    }
}
