use std::collections::BTreeMap;

use sqlx::PgPool;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerData {
    pub energy_coins: i64,
    pub gold_coins: i64,
    pub bio_soup: BTreeMap<String, i64>,
}

#[derive(Clone)]
pub struct PlayerDataRepository {
    pool: PgPool,
}

impl PlayerDataRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get(&self, user_id: i64) -> Result<PlayerData, sqlx::Error> {
        let wallet = sqlx::query_as::<_, (i64, i64)>(
            r#"
            WITH inserted AS (
                INSERT INTO player_wallets (user_id)
                VALUES ($1)
                ON CONFLICT (user_id) DO NOTHING
                RETURNING energy_coins, gold_coins
            )
            SELECT energy_coins, gold_coins FROM inserted
            UNION ALL
            SELECT energy_coins, gold_coins FROM player_wallets WHERE user_id = $1
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;

        let soup = sqlx::query_as::<_, (String, i64)>(
            "SELECT element_id, amount FROM player_bio_soup WHERE user_id = $1 AND amount > 0 ORDER BY element_id",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .collect();

        Ok(PlayerData {
            energy_coins: wallet.0,
            gold_coins: wallet.1,
            bio_soup: soup,
        })
    }
}
