use super::{client::ApiClient, error::ApiError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize)]
pub struct OrganismComponent {
    pub id: String,
    pub name: String,
    pub category: String,
    pub slots: ComponentSlots,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ComponentSlots {
    #[serde(default)]
    pub counts: BTreeMap<String, u32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CreateModelComponent {
    pub id: String,
    pub component_id: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct CreateModelConnection {
    pub first: String,
    pub second: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct OrganismModel {
    pub id: String,
    pub name: String,
    pub composition: OrganismComposition,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct OrganismComposition {
    pub components: BTreeMap<String, ModelComponent>,
    #[serde(default)]
    pub connections: Vec<ModelConnection>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ModelComponent {
    pub id: String,
    pub component: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ModelConnection {
    pub first: String,
    pub second: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct FolderEntry {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub size: Option<u64>,
}

#[derive(Deserialize)]
struct DataResponse<T> {
    data: T,
}

impl ApiClient {
    pub async fn organism_components(&self) -> Result<Vec<OrganismComponent>, ApiError> {
        let response = self
            .http
            .get(self.endpoint("organism-components"))
            .send()
            .await?;
        decode(response).await
    }

    pub async fn unlocked_organism_components(
        &self,
        token: &str,
    ) -> Result<Vec<OrganismComponent>, ApiError> {
        let response = self
            .http
            .get(self.endpoint("organism-components/unlocked"))
            .bearer_auth(token)
            .send()
            .await?;
        decode(response).await
    }

    pub async fn create_organism_model(
        &self,
        token: &str,
        name: &str,
        components: Vec<CreateModelComponent>,
        connections: Vec<CreateModelConnection>,
    ) -> Result<OrganismModel, ApiError> {
        #[derive(Serialize)]
        struct Request<'a> {
            name: &'a str,
            components: Vec<CreateModelComponent>,
            connections: Vec<CreateModelConnection>,
        }

        let response = self
            .http
            .post(self.endpoint("organism-models"))
            .bearer_auth(token)
            .json(&Request {
                name,
                components,
                connections,
            })
            .send()
            .await?;
        decode(response).await
    }

    pub async fn organism_models(&self, token: &str) -> Result<Vec<OrganismModel>, ApiError> {
        let response = self
            .http
            .get(self.endpoint("organism-models"))
            .bearer_auth(token)
            .send()
            .await?;
        decode(response).await
    }

    pub async fn organism_model_folder(
        &self,
        token: &str,
        model_id: &str,
        path: &str,
    ) -> Result<Vec<FolderEntry>, ApiError> {
        let response = self
            .http
            .get(self.endpoint(&format!("organism-models/{model_id}/folder")))
            .bearer_auth(token)
            .query(&[("path", path)])
            .send()
            .await?;
        decode(response).await
    }

    pub async fn create_organism_model_folder(
        &self,
        token: &str,
        model_id: &str,
        path: &str,
    ) -> Result<(), ApiError> {
        #[derive(Serialize)]
        struct Request<'a> {
            path: &'a str,
        }
        let response = self
            .http
            .post(self.endpoint(&format!("organism-models/{model_id}/folders")))
            .bearer_auth(token)
            .json(&Request { path })
            .send()
            .await?;
        decode_empty(response).await
    }

    pub async fn organism_model_file(
        &self,
        token: &str,
        model_id: &str,
        path: &str,
    ) -> Result<Vec<u8>, ApiError> {
        let response = self
            .http
            .get(self.endpoint(&format!("organism-models/{model_id}/files")))
            .bearer_auth(token)
            .query(&[("path", path)])
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(ApiError::from_response(response).await);
        }
        response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(ApiError::from)
    }

    pub async fn write_organism_model_file(
        &self,
        token: &str,
        model_id: &str,
        path: &str,
        content: String,
    ) -> Result<(), ApiError> {
        self.upload_organism_model_file(token, model_id, path, content.into_bytes())
            .await
    }

    pub async fn upload_organism_model_file(
        &self,
        token: &str,
        model_id: &str,
        path: &str,
        content: Vec<u8>,
    ) -> Result<(), ApiError> {
        let response = self
            .http
            .put(self.endpoint(&format!("organism-models/{model_id}/files")))
            .bearer_auth(token)
            .query(&[("path", path)])
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .body(content)
            .send()
            .await?;
        decode_empty(response).await
    }

    pub async fn delete_organism_model_entry(
        &self,
        token: &str,
        model_id: &str,
        path: &str,
    ) -> Result<(), ApiError> {
        let response = self
            .http
            .delete(self.endpoint(&format!("organism-models/{model_id}/entries")))
            .bearer_auth(token)
            .query(&[("path", path)])
            .send()
            .await?;
        decode_empty(response).await
    }

    pub async fn rename_organism_model_entry(
        &self,
        token: &str,
        model_id: &str,
        path: &str,
        name: &str,
    ) -> Result<(), ApiError> {
        #[derive(Serialize)]
        struct Request<'a> {
            path: &'a str,
            name: &'a str,
        }
        let response = self
            .http
            .patch(self.endpoint(&format!("organism-models/{model_id}/entries")))
            .bearer_auth(token)
            .json(&Request { path, name })
            .send()
            .await?;
        decode_empty(response).await
    }

    pub async fn delete_organism_model(&self, token: &str, model_id: &str) -> Result<(), ApiError> {
        let response = self
            .http
            .delete(self.endpoint(&format!("organism-models/{model_id}")))
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

async fn decode<T: for<'de> Deserialize<'de>>(response: reqwest::Response) -> Result<T, ApiError> {
    if !response.status().is_success() {
        return Err(ApiError::from_response(response).await);
    }
    response
        .json::<DataResponse<T>>()
        .await
        .map(|response| response.data)
        .map_err(|error| ApiError::InvalidResponse(error.to_string()))
}

async fn decode_empty(response: reqwest::Response) -> Result<(), ApiError> {
    if response.status().is_success() {
        Ok(())
    } else {
        Err(ApiError::from_response(response).await)
    }
}
