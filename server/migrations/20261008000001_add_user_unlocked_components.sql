CREATE TABLE IF NOT EXISTS user_unlocked_components (
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    component_id TEXT NOT NULL,
    unlocked_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, component_id)
);

CREATE INDEX IF NOT EXISTS user_unlocked_components_user_idx
    ON user_unlocked_components (user_id);
