use super::{client::ApiClient, error::ApiError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct RegisteredUser {
    pub user_id: i64,
    pub username: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoginData {
    pub token: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AccountProfile {
    pub id: i64,
    pub username: String,
    pub email: String,
}

#[derive(Deserialize)]
struct DataResponse<T> {
    data: T,
}

#[derive(Serialize)]
struct RegisterRequest<'a> {
    username: &'a str,
    password: &'a str,
    email: &'a str,
}

#[derive(Serialize)]
struct LoginRequest<'a> {
    username: &'a str,
    password: &'a str,
}

impl ApiClient {
    pub async fn register(
        &self,
        username: &str,
        password: &str,
        email: &str,
    ) -> Result<RegisteredUser, ApiError> {
        let response = self
            .http
            .post(self.endpoint("auth/register"))
            .json(&RegisterRequest {
                username,
                password,
                email,
            })
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(ApiError::from_response(response).await);
        }

        response
            .json::<DataResponse<RegisteredUser>>()
            .await
            .map(|response| response.data)
            .map_err(|error| ApiError::InvalidResponse(error.to_string()))
    }

    pub async fn login(&self, username: &str, password: &str) -> Result<LoginData, ApiError> {
        let response = self
            .http
            .post(self.endpoint("auth/login"))
            .json(&LoginRequest { username, password })
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(ApiError::from_response(response).await);
        }

        response
            .json::<DataResponse<LoginData>>()
            .await
            .map(|response| response.data)
            .map_err(|error| ApiError::InvalidResponse(error.to_string()))
    }

    pub async fn logout(&self, token: &str) -> Result<(), ApiError> {
        self.authenticated_empty_request(reqwest::Method::POST, "auth/logout", token)
            .await
    }

    pub async fn account_me(&self, token: &str) -> Result<AccountProfile, ApiError> {
        let response = self
            .http
            .get(self.endpoint("account/me"))
            .bearer_auth(token)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(ApiError::from_response(response).await);
        }

        response
            .json::<DataResponse<AccountProfile>>()
            .await
            .map(|response| response.data)
            .map_err(|error| ApiError::InvalidResponse(error.to_string()))
    }

    pub async fn delete_account(&self, token: &str) -> Result<(), ApiError> {
        self.authenticated_empty_request(reqwest::Method::DELETE, "account", token)
            .await
    }

    async fn authenticated_empty_request(
        &self,
        method: reqwest::Method,
        path: &str,
        token: &str,
    ) -> Result<(), ApiError> {
        let response = self
            .http
            .request(method, self.endpoint(path))
            .bearer_auth(token)
            .send()
            .await?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(ApiError::from_response(response).await)
        }
    }
}
