CREATE TABLE IF NOT EXISTS player_wallets (
    user_id BIGINT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    energy_coins BIGINT NOT NULL DEFAULT 0 CHECK (energy_coins >= 0),
    gold_coins BIGINT NOT NULL DEFAULT 0 CHECK (gold_coins >= 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS player_bio_soup (
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    element_id TEXT NOT NULL,
    amount BIGINT NOT NULL DEFAULT 0 CHECK (amount >= 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, element_id)
);

CREATE INDEX IF NOT EXISTS player_bio_soup_user_idx
    ON player_bio_soup (user_id);

CREATE TABLE IF NOT EXISTS user_achievements (
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    achievement_id TEXT NOT NULL,
    achieved_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, achievement_id)
);
