use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Clone, Debug, FromRow)]
pub struct OrganismModelRecord {
    pub id: Uuid,
    pub owner_user_id: i64,
    pub name: String,
    pub composition: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct OrganismModelRepository {
    pool: PgPool,
}

impl OrganismModelRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(
        &self,
        id: Uuid,
        owner_user_id: i64,
        name: &str,
        composition: Value,
    ) -> Result<OrganismModelRecord, sqlx::Error> {
        sqlx::query_as(
            r#"INSERT INTO organism_models (id, owner_user_id, name, composition)
               VALUES ($1, $2, $3, $4)
               RETURNING id, owner_user_id, name, composition, created_at, updated_at"#,
        )
        .bind(id)
        .bind(owner_user_id)
        .bind(name)
        .bind(composition)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn list_for_user(
        &self,
        user_id: i64,
    ) -> Result<Vec<OrganismModelRecord>, sqlx::Error> {
        sqlx::query_as(
            r#"SELECT id, owner_user_id, name, composition, created_at, updated_at
               FROM organism_models WHERE owner_user_id = $1
               ORDER BY created_at DESC, id"#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn find_owned(
        &self,
        id: Uuid,
        user_id: i64,
    ) -> Result<Option<OrganismModelRecord>, sqlx::Error> {
        sqlx::query_as(
            r#"SELECT id, owner_user_id, name, composition, created_at, updated_at
               FROM organism_models WHERE id = $1 AND owner_user_id = $2"#,
        )
        .bind(id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn delete_owned(&self, id: Uuid, user_id: i64) -> Result<bool, sqlx::Error> {
        Ok(
            sqlx::query("DELETE FROM organism_models WHERE id = $1 AND owner_user_id = $2")
                .bind(id)
                .bind(user_id)
                .execute(&self.pool)
                .await?
                .rows_affected()
                == 1,
        )
    }
}
