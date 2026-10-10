use std::collections::BTreeSet;

use crate::domain::ComponentUnlockMethod;
use sqlx::PgPool;

#[derive(Clone)]
pub struct ComponentUnlockRepository {
    pool: PgPool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnlockResult {
    Unlocked,
    AlreadyUnlocked,
    InsufficientFunds,
}

impl ComponentUnlockRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn list_for_user(&self, user_id: i64) -> Result<BTreeSet<String>, sqlx::Error> {
        let ids = sqlx::query_scalar::<_, String>(
            "SELECT component_id FROM user_unlocked_components WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(ids.into_iter().collect())
    }

    pub async fn unlock_with_methods(
        &self,
        user_id: i64,
        component_id: &str,
        methods: &[ComponentUnlockMethod],
    ) -> Result<UnlockResult, sqlx::Error> {
        let mut transaction = self.pool.begin().await?;

        // Repeated unlock requests are idempotent. Check this before looking at
        // the wallet so an already-unlocked component never depends on balance.
        let already_unlocked = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM user_unlocked_components WHERE user_id = $1 AND component_id = $2)",
        )
        .bind(user_id)
        .bind(component_id)
        .fetch_one(&mut *transaction)
        .await?;
        if already_unlocked {
            transaction.rollback().await?;
            return Ok(UnlockResult::AlreadyUnlocked);
        }

        sqlx::query(
            "INSERT INTO player_wallets (user_id) VALUES ($1) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .execute(&mut *transaction)
        .await?;

        let (energy, gold) = sqlx::query_as::<_, (i64, i64)>(
            "SELECT energy_coins, gold_coins FROM player_wallets WHERE user_id = $1 FOR UPDATE",
        )
        .bind(user_id)
        .fetch_one(&mut *transaction)
        .await?;

        // Another request may have completed the unlock while this request
        // waited for the wallet row. Re-check after acquiring that lock so the
        // loser of the race is idempotent instead of evaluating stale funds.
        let already_unlocked = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM user_unlocked_components WHERE user_id = $1 AND component_id = $2)",
        )
        .bind(user_id)
        .bind(component_id)
        .fetch_one(&mut *transaction)
        .await?;
        if already_unlocked {
            transaction.rollback().await?;
            return Ok(UnlockResult::AlreadyUnlocked);
        }

        let mut charge = None;
        for method in methods {
            match method {
                ComponentUnlockMethod::EnergyCoins { amount }
                    if i64::try_from(*amount)
                        .ok()
                        .is_some_and(|cost| energy >= cost) =>
                {
                    charge = Some(("energy_coins", i64::try_from(*amount).unwrap()));
                    break;
                }
                ComponentUnlockMethod::GoldCoins { amount }
                    if i64::try_from(*amount).ok().is_some_and(|cost| gold >= cost) =>
                {
                    charge = Some(("gold_coins", i64::try_from(*amount).unwrap()));
                    break;
                }
                ComponentUnlockMethod::Achievement { achievement_id } => {
                    let achieved = sqlx::query_scalar::<_, bool>(
                        "SELECT EXISTS(SELECT 1 FROM user_achievements WHERE user_id = $1 AND achievement_id = $2)",
                    )
                    .bind(user_id)
                    .bind(achievement_id)
                    .fetch_one(&mut *transaction)
                    .await?;
                    if achieved {
                        charge = Some(("none", 0));
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some((currency, amount)) = charge else {
            transaction.rollback().await?;
            return Ok(UnlockResult::InsufficientFunds);
        };

        let inserted = sqlx::query(
            "INSERT INTO user_unlocked_components (user_id, component_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(user_id)
        .bind(component_id)
        .execute(&mut *transaction)
        .await?
        .rows_affected() == 1;
        if !inserted {
            transaction.rollback().await?;
            return Ok(UnlockResult::AlreadyUnlocked);
        }
        if currency != "none" {
            sqlx::query(&format!("UPDATE player_wallets SET {currency} = {currency} - $2, updated_at = now() WHERE user_id = $1"))
                .bind(user_id)
                .bind(amount)
                .execute(&mut *transaction)
                .await?;
        }

        transaction.commit().await?;
        Ok(UnlockResult::Unlocked)
    }
}
