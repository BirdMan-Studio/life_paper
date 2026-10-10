use super::error::ApiError;
use crate::{domain::User, state::AppState};
use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    routing::{delete, get, post},
};
use chrono::{Duration, Utc};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const SESSION_TTL_SECONDS: u64 = 7 * 24 * 60 * 60;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/account/me", get(account_me))
        .route("/account", delete(delete_account))
}

#[derive(Deserialize)]
struct RegisterRequest {
    username: String,
    password: String,
    email: String,
}

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct DataResponse<T> {
    data: T,
}

#[derive(Serialize)]
struct RegisteredUser {
    user_id: i64,
    username: String,
}

#[derive(Serialize)]
struct LoginData {
    token: String,
    expires_at: String,
}

#[derive(Serialize)]
struct AccountProfile {
    id: i64,
    username: String,
    email: String,
}

pub(crate) struct AuthenticatedSession {
    token: String,
    pub(crate) user_id: i64,
}

async fn register(
    State(state): State<AppState>,
    Json(request): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<DataResponse<RegisteredUser>>), ApiError> {
    validate_registration(&request)?;

    let password_hash = hash_password(request.password).await?;
    let username = request.username.trim();
    let email = request.email.trim();
    let user = state
        .users
        .create(username, &password_hash, email)
        .await
        .map_err(map_create_user_error)?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse {
            data: RegisteredUser {
                user_id: user.id,
                username: user.username,
            },
        }),
    ))
}

async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<DataResponse<LoginData>>, ApiError> {
    if request.username.trim().is_empty() || request.password.is_empty() {
        return Err(ApiError::BadRequest("用户名和密码不能为空".to_string()));
    }

    let username = request.username.trim();
    let user = state
        .users
        .find_by_username(username)
        .await
        .map_err(|error| {
            tracing::error!(target: "api", %error, "failed to query user during login");
            ApiError::Internal
        })?
        .ok_or(ApiError::InvalidCredentials)?;

    if !verify_password(request.password, user.password_hash.clone()).await? {
        return Err(ApiError::InvalidCredentials);
    }

    let token = create_session(&state, &user).await?;
    let expires_at = (Utc::now() + Duration::seconds(SESSION_TTL_SECONDS as i64)).to_rfc3339();

    Ok(Json(DataResponse {
        data: LoginData { token, expires_at },
    }))
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Result<StatusCode, ApiError> {
    let session = authenticate(&state, &headers).await?;
    delete_session(&state, session.user_id, &session.token).await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn account_me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<DataResponse<AccountProfile>>, ApiError> {
    let session = authenticate(&state, &headers).await?;
    let user = state
        .users
        .find_by_id(session.user_id)
        .await
        .map_err(|error| {
            tracing::error!(target: "api", %error, user_id = session.user_id, "failed to query current account");
            ApiError::Internal
        })?;

    let Some(user) = user else {
        delete_session(&state, session.user_id, &session.token).await?;
        return Err(ApiError::Unauthorized);
    };

    Ok(Json(DataResponse {
        data: AccountProfile {
            id: user.id,
            username: user.username,
            email: user.email,
        },
    }))
}

async fn delete_account(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let session = authenticate(&state, &headers).await?;
    let _storage_guard = state.organism_model_storage_lock.lock().await;
    let model_ids: Vec<_> = state
        .organism_models
        .list_for_user(session.user_id)
        .await
        .map_err(|error| {
            tracing::error!(target: "api", %error, user_id = session.user_id, "failed to list organism models before account deletion");
            ApiError::Internal
        })?
        .into_iter()
        .map(|model| model.id)
        .collect();

    let deleted = state.users.delete(session.user_id).await.map_err(|error| {
        tracing::error!(target: "api", %error, user_id = session.user_id, "failed to delete account");
        ApiError::Internal
    })?;

    if !deleted {
        return Err(ApiError::Unauthorized);
    }

    // Only remove in-memory and on-disk worlds after the account deletion
    // succeeds, so a database failure cannot destroy user data.
    state.worlds.remove_for_user(session.user_id);
    for model_id in model_ids {
        let folder = state.organism_model_storage.join(model_id.to_string());
        if let Err(error) = tokio::fs::remove_dir_all(folder).await
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(target: "api", %error, %model_id, "failed to remove organism model folder after account deletion");
        }
    }
    revoke_all_sessions(&state, session.user_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

fn validate_registration(request: &RegisterRequest) -> Result<(), ApiError> {
    let username = request.username.trim();
    let email = request.email.trim();

    if !(3..=32).contains(&username.chars().count()) {
        return Err(ApiError::BadRequest(
            "用户名长度必须在 3 到 32 个字符之间".to_string(),
        ));
    }

    if request.password.len() < 8 {
        return Err(ApiError::BadRequest(
            "密码长度不能少于 8 个字节".to_string(),
        ));
    }

    if email.len() > 254 || !email.contains('@') {
        return Err(ApiError::BadRequest("邮箱格式不正确".to_string()));
    }

    Ok(())
}

async fn hash_password(password: String) -> Result<String, ApiError> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
    })
    .await
    .map_err(|error| {
        tracing::error!(target: "api", %error, "password hashing task failed");
        ApiError::Internal
    })?
    .map_err(|error| {
        tracing::error!(target: "api", %error, "password hashing failed");
        ApiError::Internal
    })
}

async fn verify_password(password: String, password_hash: String) -> Result<bool, ApiError> {
    tokio::task::spawn_blocking(move || {
        let Ok(parsed_hash) = PasswordHash::new(&password_hash) else {
            return false;
        };

        Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok()
    })
    .await
    .map_err(|error| {
        tracing::error!(target: "api", %error, "password verification task failed");
        ApiError::Internal
    })
}

async fn create_session(state: &AppState, user: &User) -> Result<String, ApiError> {
    let token: String = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let session_key = session_key(&token);
    let user_sessions_key = user_sessions_key(user.id);
    let mut connection = state.redis_pool.get().await.map_err(|error| {
        tracing::error!(target: "api", %error, "failed to get Redis connection");
        ApiError::Internal
    })?;

    redis::pipe()
        .atomic()
        .cmd("SET")
        .arg(&session_key)
        .arg(user.id)
        .arg("EX")
        .arg(SESSION_TTL_SECONDS)
        .ignore()
        .cmd("SADD")
        .arg(&user_sessions_key)
        .arg(&session_key)
        .ignore()
        .cmd("EXPIRE")
        .arg(&user_sessions_key)
        .arg(SESSION_TTL_SECONDS)
        .ignore()
        .query_async::<()>(&mut connection)
        .await
        .map_err(|error| {
            tracing::error!(target: "api", %error, "failed to create session");
            ApiError::Internal
        })?;

    Ok(token)
}

pub(crate) async fn authenticate(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AuthenticatedSession, ApiError> {
    let authorization = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or(ApiError::Unauthorized)?;
    let token = authorization
        .strip_prefix("Bearer ")
        .filter(|token| !token.is_empty())
        .ok_or(ApiError::Unauthorized)?
        .to_string();

    let mut connection = state.redis_pool.get().await.map_err(|error| {
        tracing::error!(target: "api", %error, "failed to get Redis connection");
        ApiError::Internal
    })?;
    let user_id = connection
        .get::<_, Option<i64>>(session_key(&token))
        .await
        .map_err(|error| {
            tracing::error!(target: "api", %error, "failed to read session");
            ApiError::Internal
        })?
        .ok_or(ApiError::Unauthorized)?;

    Ok(AuthenticatedSession { token, user_id })
}

async fn delete_session(state: &AppState, user_id: i64, token: &str) -> Result<(), ApiError> {
    let mut connection = state.redis_pool.get().await.map_err(|error| {
        tracing::error!(target: "api", %error, "failed to get Redis connection");
        ApiError::Internal
    })?;

    let session_key = session_key(token);
    redis::pipe()
        .atomic()
        .cmd("DEL")
        .arg(&session_key)
        .ignore()
        .cmd("SREM")
        .arg(user_sessions_key(user_id))
        .arg(&session_key)
        .ignore()
        .query_async::<()>(&mut connection)
        .await
        .map_err(|error| {
            tracing::error!(target: "api", %error, "failed to delete session");
            ApiError::Internal
        })
}

async fn revoke_all_sessions(state: &AppState, user_id: i64) -> Result<(), ApiError> {
    const REVOKE_ALL_SCRIPT: &str = r#"
        local session_keys = redis.call('SMEMBERS', KEYS[1])
        for _, session_key in ipairs(session_keys) do
            redis.call('DEL', session_key)
        end
        redis.call('DEL', KEYS[1])
        return #session_keys
    "#;

    let mut connection = state.redis_pool.get().await.map_err(|error| {
        tracing::error!(target: "api", %error, "failed to get Redis connection");
        ApiError::Internal
    })?;

    let revoked_count = redis::cmd("EVAL")
        .arg(REVOKE_ALL_SCRIPT)
        .arg(1)
        .arg(user_sessions_key(user_id))
        .query_async::<usize>(&mut connection)
        .await
        .map_err(|error| {
            tracing::error!(target: "api", %error, user_id, "failed to revoke all sessions");
            ApiError::Internal
        })?;

    tracing::info!(target: "api", user_id, revoked_count, "revoked all user sessions");
    Ok(())
}

fn session_key(token: &str) -> String {
    format!("session:v2:{token}")
}

fn user_sessions_key(user_id: i64) -> String {
    format!("user_sessions:v2:{user_id}")
}

fn map_create_user_error(error: sqlx::Error) -> ApiError {
    if let sqlx::Error::Database(database_error) = &error
        && database_error.is_unique_violation()
    {
        return ApiError::Conflict("用户名或邮箱已被使用".to_string());
    }

    tracing::error!(target: "api", %error, "failed to create user");
    ApiError::Internal
}
