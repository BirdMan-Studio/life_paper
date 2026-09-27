use crate::domain::User;
use sqlx::{FromRow, PgPool};

#[derive(Clone)]
pub struct UserRepository {
    pool: PgPool,
}

#[derive(FromRow)]
struct UserRow {
    id: i64,
    username: String,
    password_hash: String,
    email: String,
}

impl From<UserRow> for User {
    fn from(row: UserRow) -> Self {
        Self::new(row.id, row.username, row.password_hash, row.email)
    }
}

impl UserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(
        &self,
        username: &str,
        password_hash: &str,
        email: &str,
    ) -> Result<User, sqlx::Error> {
        let row = sqlx::query_as::<_, UserRow>(
            r#"
            INSERT INTO users (username, password_hash, email)
            VALUES ($1, $2, $3)
            RETURNING id, username, password_hash, email
            "#,
        )
        .bind(username)
        .bind(password_hash)
        .bind(email)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.into())
    }

    pub async fn find_by_id(&self, id: i64) -> Result<Option<User>, sqlx::Error> {
        let row = sqlx::query_as::<_, UserRow>(
            r#"
            SELECT id, username, password_hash, email
            FROM users
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Into::into))
    }

    pub async fn find_by_username(&self, username: &str) -> Result<Option<User>, sqlx::Error> {
        let row = sqlx::query_as::<_, UserRow>(
            r#"
            SELECT id, username, password_hash, email
            FROM users
            WHERE username = $1
            "#,
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Into::into))
    }

    pub async fn update_username(
        &self,
        id: i64,
        username: &str,
    ) -> Result<Option<User>, sqlx::Error> {
        let row = sqlx::query_as::<_, UserRow>(
            r#"
            UPDATE users
            SET username = $1
            WHERE id = $2
            RETURNING id, username, password_hash, email
            "#,
        )
        .bind(username)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Into::into))
    }

    pub async fn update_email(&self, id: i64, email: &str) -> Result<Option<User>, sqlx::Error> {
        let row = sqlx::query_as::<_, UserRow>(
            r#"
            UPDATE users
            SET email = $1
            WHERE id = $2
            RETURNING id, username, password_hash, email
            "#,
        )
        .bind(email)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Into::into))
    }

    pub async fn update_password_hash(
        &self,
        id: i64,
        password_hash: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            UPDATE users
            SET password_hash = $1
            WHERE id = $2
            "#,
        )
        .bind(password_hash)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    pub async fn delete(&self, id: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() == 1)
    }
}
