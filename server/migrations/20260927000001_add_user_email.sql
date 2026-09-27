ALTER TABLE users
    ADD COLUMN IF NOT EXISTS email TEXT;

-- 为旧数据生成不可投递的占位邮箱，使字段可以安全改为 NOT NULL。
UPDATE users
SET email = CONCAT('legacy-', id, '@invalid.local')
WHERE email IS NULL;

ALTER TABLE users
    ALTER COLUMN email SET NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS users_email_unique_idx
    ON users (email);
