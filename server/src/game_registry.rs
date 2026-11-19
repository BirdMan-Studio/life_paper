use crate::{
    administration::{AdminService, GameServer},
    api::error::ApiError,
    state::AppState,
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::HeaderMap,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const LEASE_SECONDS: i64 = 90;
#[derive(Clone, Deserialize, Serialize)]
pub struct WorldAnnouncement {
    pub id: Uuid,
    pub owner_user_id: i64,
    pub name: String,
    #[serde(default)]
    pub level_id: String,
    #[serde(default)]
    pub tutorial_completed: bool,
}
#[derive(Serialize, Deserialize)]
pub struct Registration {
    pub server_id: String,
    pub api_url: String,
    pub worlds: Vec<WorldAnnouncement>,
}
pub fn hash_key(key: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(key.as_bytes()))
}
pub fn generate_key() -> Result<String, ApiError> {
    use base64::Engine;
    use ring::rand::SecureRandom;
    let mut bytes = [0u8; 32];
    ring::rand::SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| ApiError::Internal)?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}
pub fn online(server: &GameServer) -> bool {
    server.registration_key_hash.is_some()
        && server.last_seen.is_some_and(|time| {
            chrono::Utc::now()
                .timestamp()
                .checked_sub(time)
                .is_some_and(|age| (0..LEASE_SECONDS).contains(&age))
        })
}
pub fn public_server(server: &GameServer) -> Value {
    json!({"id":server.id,"name":server.name,"api_url":server.api_url,"direct_url":server.api_url,"relay_url":crate::relay::relay_path(&server.id),"online":online(server),"last_seen":server.last_seen,"key_configured":server.registration_key_hash.is_some()})
}
impl AdminService {
    pub fn connected_server(&self, id: &str) -> Result<GameServer, ApiError> {
        let server = self
            .servers
            .lock()
            .unwrap()
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .ok_or(ApiError::NotFound)?;
        if !online(&server) {
            return Err(ApiError::Conflict(
                "游戏服尚未通过密钥注册或心跳已过期".into(),
            ));
        }
        Ok(server)
    }
    fn register_server(&self, key: &str, request: Registration) -> Result<GameServer, ApiError> {
        if request.worlds.len() > 1024
            || request
                .worlds
                .iter()
                .any(|w| w.owner_user_id <= 0 || w.name.chars().count() > 128)
        {
            return Err(ApiError::BadRequest("世界目录无效".into()));
        }
        let mut servers = self.servers.lock().unwrap();
        let server = servers
            .iter_mut()
            .find(|s| s.id == request.server_id)
            .ok_or(ApiError::Unauthorized)?;
        let expected = server
            .registration_key_hash
            .as_deref()
            .ok_or(ApiError::Unauthorized)?;
        let actual = hash_key(key);
        if expected.len() != actual.len()
            || expected
                .bytes()
                .zip(actual.bytes())
                .fold(0u8, |v, (a, b)| v | (a ^ b))
                != 0
        {
            return Err(ApiError::Unauthorized);
        }
        if server.api_url != request.api_url {
            return Err(ApiError::BadRequest("上报地址与中心登记地址不一致".into()));
        }
        server.last_seen = Some(chrono::Utc::now().timestamp());
        server.worlds = request.worlds;
        Ok(server.clone())
    }
}
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/game-servers/register", post(register))
        .route("/game-servers", get(list))
        .route("/game-servers/{id}/connection", get(connection))
        .route("/game-worlds/{id}/connection", get(world_connection))
}
async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<Registration>,
) -> Result<Json<Value>, ApiError> {
    let key = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|v| v.len() <= 256)
        .ok_or(ApiError::Unauthorized)?;
    let server = state.admin.register_server(key, request)?;
    let receipt = crate::transfer::sign(&state.signing_key, "game-registration", &server.id, 1, ())
        .map_err(|_| ApiError::Internal)?;
    Ok(Json(
        json!({"data":{"receipt":receipt,"lease_seconds":LEASE_SECONDS,"server_id":server.id}}),
    ))
}
pub async fn rotate_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let key = generate_key()?;
    let mut guard = state.admin.servers.lock().unwrap();
    let mut updated = guard.clone();
    let server = updated
        .iter_mut()
        .find(|s| s.id == id)
        .ok_or(ApiError::NotFound)?;
    server.registration_key_hash = Some(hash_key(&key));
    server.last_seen = None;
    server.worlds.clear();
    state.admin.save_servers(&updated)?;
    *guard = updated;
    Ok(Json(json!({"data":{"registration_key":key}})))
}
async fn list(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<Value>, ApiError> {
    let session = crate::api::auth::authenticate(&state, &headers).await?;
    Ok(Json(
        json!({"deployment_mode":if state.central_only { "central" } else { "combined" },"data":state.admin.servers.lock().unwrap().iter().filter(|s| online(s)).map(|s| {
        let mut value = public_server(s);
        value["worlds"] = json!(s.worlds.iter().filter(|w| w.owner_user_id == session.user_id).collect::<Vec<_>>()); value
    }).collect::<Vec<_>>() }),
    ))
}
fn connection_data(
    state: &AppState,
    server: &GameServer,
    user_id: i64,
) -> Result<Json<Value>, ApiError> {
    let token = crate::transfer::sign(&state.signing_key, "game-entry", &server.id, user_id, ())
        .map_err(|_| ApiError::Internal)?;
    Ok(Json(
        json!({"data":{"game_server_id":server.id,"api_url":server.api_url,"direct_url":server.api_url,"relay_url":crate::relay::relay_path(&server.id),"token":token,"expires_in":900}}),
    ))
}
async fn connection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let session = crate::api::auth::authenticate(&state, &headers).await?;
    connection_data(&state, &state.admin.connected_server(&id)?, session.user_id)
}
async fn world_connection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let session = crate::api::auth::authenticate(&state, &headers).await?;
    let servers = state.admin.servers.lock().unwrap();
    let matches: Vec<_> = servers
        .iter()
        .filter(|s| {
            online(s)
                && s.worlds
                    .iter()
                    .any(|w| w.id == id && w.owner_user_id == session.user_id)
        })
        .collect();
    if matches.len() > 1 {
        return Err(ApiError::Conflict("世界ID在多个游戏服重复".into()));
    }
    connection_data(
        &state,
        matches.first().ok_or(ApiError::NotFound)?,
        session.user_id,
    )
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistrationConfig {
    pub central_api_url: String,
    pub public_api_url: String,
    pub verification_key: String,
}
impl RegistrationConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        for address in [&self.central_api_url, &self.public_api_url] {
            let url = reqwest::Url::parse(address)
                .map_err(|_| anyhow::anyhow!("invalid registration API URL"))?;
            anyhow::ensure!(
                matches!(url.scheme(), "http" | "https")
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.query().is_none()
                    && url.fragment().is_none()
                    && url.path() == "/api/v1",
                "registration URLs must be http(s)://host:port/api/v1 without credentials or query strings"
            );
        }
        anyhow::ensure!(
            (32..=256).contains(&self.verification_key.len())
                && self.verification_key.bytes().all(|v| v.is_ascii_graphic()),
            "invalid game server verification key"
        );
        Ok(())
    }
}
pub fn registration_client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(10))
        .build()?)
}
pub async fn announce(
    client: &reqwest::Client,
    config: &RegistrationConfig,
    server_id: &str,
    public_key: &[u8],
    worlds: &crate::domain::WorldDirectory,
) -> anyhow::Result<()> {
    let request = Registration {
        server_id: server_id.into(),
        api_url: config.public_api_url.clone(),
        worlds: worlds
            .list_all()
            .iter()
            .map(|w| WorldAnnouncement {
                id: w.id,
                owner_user_id: w.owner_user_id,
                name: w.name.clone(),
                level_id: w.level_id.clone(),
                tutorial_completed: w.is_tutorial() && worlds.read_world(w.id, w.owner_user_id).ok()
                    .and_then(|world| world.tutorial.as_ref().map(|tutorial| tutorial.completed)).unwrap_or(false),
            })
            .collect(),
    };
    let mut response = client
        .post(format!("{}/game-servers/register", config.central_api_url))
        .bearer_auth(&config.verification_key)
        .json(&request)
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("central registration connection failed"))?;
    anyhow::ensure!(
        response.status().is_success(),
        "central rejected game server registration ({})",
        response.status().as_u16()
    );
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| anyhow::anyhow!("cannot read registration response"))?
    {
        anyhow::ensure!(
            bytes.len() + chunk.len() <= 16384,
            "registration response too large"
        );
        bytes.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("invalid registration response"))?;
    let receipt = value["data"]["receipt"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing registration receipt"))?;
    crate::transfer::verify::<()>(receipt, public_key, "game-registration", server_id)
        .map_err(|_| anyhow::anyhow!("untrusted central registration receipt"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn registration_requires_key_and_address_and_expires_after_reboot() {
        let root = std::env::temp_dir().join(format!("life-registration-{}", Uuid::new_v4()));
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
            .unwrap();
        let logs = crate::observability::LogStore::open(root.join("logs")).unwrap();
        let service = AdminService::new(
            pool.clone(),
            logs.clone(),
            root.join("admin"),
            root.join("dist"),
            None,
        )
        .unwrap();
        let key = generate_key().unwrap();
        service.servers.lock().unwrap().push(GameServer {
            id: "game-1".into(),
            name: "test".into(),
            api_url: "http://127.0.0.1:8081/api/v1".into(),
            registration_key_hash: Some(hash_key(&key)),
            last_seen: None,
            worlds: vec![],
        });
        service
            .save_servers(&service.servers.lock().unwrap())
            .unwrap();
        let world_id = Uuid::new_v4();
        let payload = || Registration {
            server_id: "game-1".into(),
            api_url: "http://127.0.0.1:8081/api/v1".into(),
            worlds: vec![WorldAnnouncement {
                id: world_id,
                owner_user_id: 7,
                name: "private world".into(),
                level_id: "basic".into(),
                tutorial_completed: false,
            }],
        };
        assert!(service.connected_server("game-1").is_err());
        assert!(matches!(
            service.register_server("wrong-key", payload()),
            Err(ApiError::Unauthorized)
        ));
        let mut request = payload();
        request.api_url = "http://attacker.test/api/v1".into();
        assert!(matches!(
            service.register_server(&key, request),
            Err(ApiError::BadRequest(_))
        ));
        let mut request = payload();
        request.server_id = "game-2".into();
        assert!(matches!(
            service.register_server(&key, request),
            Err(ApiError::Unauthorized)
        ));
        service.register_server(&key, payload()).unwrap();
        let server = service.connected_server("game-1").unwrap();
        assert_eq!(server.worlds[0].id, world_id);
        assert!(
            public_server(&server)
                .get("registration_key_hash")
                .is_none()
        );
        service.servers.lock().unwrap()[0].last_seen =
            Some(chrono::Utc::now().timestamp() - LEASE_SECONDS - 1);
        assert!(service.connected_server("game-1").is_err());
        service.register_server(&key, payload()).unwrap();
        service
            .save_servers(&service.servers.lock().unwrap())
            .unwrap();
        drop(service);
        let service = AdminService::new(
            pool,
            logs.clone(),
            root.join("admin"),
            root.join("dist"),
            None,
        )
        .unwrap();
        assert!(service.connected_server("game-1").is_err());
        assert!(service.servers.lock().unwrap()[0].worlds.is_empty());
        service.register_server(&key, payload()).unwrap();
        assert!(service.connected_server("game-1").is_ok());
        drop(service);
        drop(logs);
        std::fs::remove_dir_all(root).unwrap();
    }
}
