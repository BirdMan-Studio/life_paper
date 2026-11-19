use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD as BASE64};
use ring::signature::{ED25519, Ed25519KeyPair, KeyPair, UnparsedPublicKey};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{collections::BTreeMap, path::Path};
use uuid::Uuid;

pub const MAX_BUNDLE_BYTES: usize =
    crate::domain::organism::program_storage::MAX_ACCOUNT_STORAGE_BYTES as usize;
// File bytes and then the signed JSON are both base64 encoded; reserve metadata space too.
const MAX_SIGNED_BYTES: usize = MAX_BUNDLE_BYTES * 2;
pub const MAX_TRANSFER_REQUEST_BYTES: usize = MAX_SIGNED_BYTES + 8 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Claims<T> {
    pub version: u32,
    pub purpose: String,
    pub audience: String,
    pub user_id: i64,
    pub issued_at: i64,
    pub expires_at: i64,
    pub data: T,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ModelBundle {
    pub id: Uuid,
    pub name: String,
    pub composition: serde_json::Value,
    /// Base64 encoded bytes preserve arbitrary model resource files.
    pub files: BTreeMap<String, String>,
    #[serde(default)]
    pub directories: Vec<String>,
}

pub fn public_key(key: &Ed25519KeyPair) -> String {
    BASE64.encode(key.public_key().as_ref())
}

pub fn load_signing_key(path: &Path) -> anyhow::Result<Ed25519KeyPair> {
    if !path.exists() {
        use std::io::Write;
        let key = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new())
            .map_err(|_| anyhow::anyhow!("cannot generate signing key"))?;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(path) {
            Ok(mut file) => {
                file.write_all(key.as_ref())?;
                file.sync_all()?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ed25519KeyPair::from_pkcs8(&std::fs::read(path)?)
        .map_err(|_| anyhow::anyhow!("invalid signing key"))
}

pub fn sign<T: Serialize>(
    key: &Ed25519KeyPair,
    purpose: &str,
    audience: &str,
    user_id: i64,
    data: T,
) -> anyhow::Result<String> {
    let now = chrono::Utc::now().timestamp();
    let claims = Claims {
        version: 1,
        purpose: purpose.into(),
        audience: audience.into(),
        user_id,
        issued_at: now,
        expires_at: now + 900,
        data,
    };
    let payload = BASE64.encode(serde_json::to_vec(&claims)?);
    let signature = BASE64.encode(key.sign(payload.as_bytes()).as_ref());
    Ok(format!("{payload}.{signature}"))
}

pub fn decode_public_key(value: &str) -> anyhow::Result<Vec<u8>> {
    let bytes = BASE64.decode(value)?;
    anyhow::ensure!(bytes.len() == 32, "Ed25519 public key must be 32 bytes");
    Ok(bytes)
}

pub fn verify<T: DeserializeOwned>(
    token: &str,
    key: &[u8],
    purpose: &str,
    audience: &str,
) -> anyhow::Result<Claims<T>> {
    anyhow::ensure!(token.len() <= MAX_SIGNED_BYTES, "signed payload too large");
    let (payload, signature) = token
        .split_once('.')
        .ok_or_else(|| anyhow::anyhow!("invalid signed payload"))?;
    UnparsedPublicKey::new(&ED25519, key)
        .verify(payload.as_bytes(), &BASE64.decode(signature)?)
        .map_err(|_| anyhow::anyhow!("invalid signature"))?;
    let claims: Claims<T> = serde_json::from_slice(&BASE64.decode(payload)?)?;
    let now = chrono::Utc::now().timestamp();
    anyhow::ensure!(
        claims.version == 1
            && claims.purpose == purpose
            && claims.audience == audience
            && claims.user_id > 0
            && claims.issued_at <= now + 30
            && claims.expires_at > now
            && claims.expires_at > claims.issued_at
            && claims
                .expires_at
                .checked_sub(claims.issued_at)
                .is_some_and(|ttl| ttl <= 900),
        "invalid or expired claims"
    );
    Ok(claims)
}

impl ModelBundle {
    pub fn read(
        id: Uuid,
        name: String,
        composition: serde_json::Value,
        root: &Path,
    ) -> anyhow::Result<Self> {
        use crate::domain::organism::program_storage::{checked_path, fingerprint};
        fingerprint(root)?;
        let mut files = BTreeMap::new();
        let mut directories = Vec::new();
        let mut pending = vec![String::new()];
        let mut total = 0usize;
        while let Some(path) = pending.pop() {
            for entry in std::fs::read_dir(root.join(&path))? {
                let entry = entry?;
                let relative = if path.is_empty() {
                    entry
                        .file_name()
                        .to_str()
                        .ok_or_else(|| anyhow::anyhow!("non-UTF8 path"))?
                        .to_owned()
                } else {
                    format!(
                        "{path}/{}",
                        entry
                            .file_name()
                            .to_str()
                            .ok_or_else(|| anyhow::anyhow!("non-UTF8 path"))?
                    )
                };
                let target = checked_path(root, &relative, false)?;
                if target.is_dir() {
                    directories.push(relative.clone());
                    pending.push(relative);
                } else {
                    let bytes = std::fs::read(target)?;
                    total += bytes.len();
                    anyhow::ensure!(
                        total <= MAX_BUNDLE_BYTES && files.len() < 1024,
                        "model package too large"
                    );
                    files.insert(relative, BASE64.encode(bytes));
                }
            }
        }
        Ok(Self {
            id,
            name,
            composition,
            files,
            directories,
        })
    }

    pub fn write_program(&self, target: &Path) -> anyhow::Result<()> {
        use crate::domain::organism::program_storage::{checked_path, ensure_entry};
        anyhow::ensure!(
            self.files.len().saturating_add(self.directories.len()) <= 1024,
            "too many model entries"
        );
        std::fs::create_dir(target)?;
        let result = (|| {
            let mut total = 0usize;
            for path in &self.directories {
                std::fs::create_dir_all(checked_path(target, path, false)?)?;
            }
            for (path, value) in &self.files {
                let file = checked_path(target, path, false)?;
                let bytes = BASE64.decode(value)?;
                total = total
                    .checked_add(bytes.len())
                    .ok_or_else(|| anyhow::anyhow!("package size overflow"))?;
                anyhow::ensure!(total <= MAX_BUNDLE_BYTES, "model package too large");
                if let Some(parent) = file.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(file, bytes)?;
            }
            ensure_entry(target)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_dir_all(target);
        }
        result
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn transfer_limits_cover_nested_base64_for_full_account_capacity() {
        let encoded_files = MAX_BUNDLE_BYTES.div_ceil(3) * 4;
        let signed_json = (encoded_files + 4 * 1024 * 1024).div_ceil(3) * 4 + 128;
        assert!(signed_json <= MAX_SIGNED_BYTES);
        assert!(signed_json + 1024 * 1024 <= MAX_TRANSFER_REQUEST_BYTES);
    }

    pub(crate) fn key() -> Ed25519KeyPair {
        let document = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
        Ed25519KeyPair::from_pkcs8(document.as_ref()).unwrap()
    }

    #[test]
    fn signed_claims_bind_user_audience_purpose_and_expiry_and_reject_tampering() {
        let key = key();
        let public = key.public_key().as_ref();
        let token = sign(&key, "game-entry", "game-1", 7, ()).unwrap();
        assert_eq!(
            verify::<()>(&token, public, "game-entry", "game-1")
                .unwrap()
                .user_id,
            7
        );
        assert!(verify::<()>(&token, public, "game-entry", "game-2").is_err());
        assert!(verify::<()>(&token, public, "model-bundle", "game-1").is_err());
        assert!(
            verify::<()>(
                &token,
                super::tests::key().public_key().as_ref(),
                "game-entry",
                "game-1"
            )
            .is_err()
        );
        let (payload, signature) = token.split_once('.').unwrap();
        let mut claims: serde_json::Value =
            serde_json::from_slice(&BASE64.decode(payload).unwrap()).unwrap();
        claims["user_id"] = 9.into();
        let tampered = format!(
            "{}.{}",
            BASE64.encode(serde_json::to_vec(&claims).unwrap()),
            signature
        );
        assert!(verify::<()>(&tampered, public, "game-entry", "game-1").is_err());
        for (issued, expires) in [
            (0, 1),
            (
                chrono::Utc::now().timestamp() + 120,
                chrono::Utc::now().timestamp() + 900,
            ),
        ] {
            claims["issued_at"] = issued.into();
            claims["expires_at"] = expires.into();
            let payload = BASE64.encode(serde_json::to_vec(&claims).unwrap());
            let token = format!(
                "{}.{}",
                payload,
                BASE64.encode(key.sign(payload.as_bytes()).as_ref())
            );
            assert!(verify::<()>(&token, public, "game-entry", "game-1").is_err());
        }
    }

    #[test]
    fn key_persists_and_model_packages_preserve_resources_and_reject_paths_atomically() {
        let temp = std::env::temp_dir().join(format!("life-transfer-{}", Uuid::new_v4()));
        std::fs::create_dir_all(temp.join("model/lib")).unwrap();
        std::fs::create_dir_all(temp.join("model/empty")).unwrap();
        let first = load_signing_key(&temp.join("key.pk8")).unwrap();
        assert_eq!(
            public_key(&first),
            public_key(&load_signing_key(&temp.join("key.pk8")).unwrap())
        );
        assert_eq!(
            decode_public_key(&public_key(&first)).unwrap(),
            first.public_key().as_ref()
        );
        std::fs::write(temp.join("model/main.py"), b"import lib.brain\n").unwrap();
        std::fs::write(temp.join("model/lib/brain.py"), b"value = 1\n").unwrap();
        std::fs::write(temp.join("model/asset.bin"), [0, 128, 255]).unwrap();
        let mut bundle = ModelBundle::read(
            Uuid::new_v4(),
            "test".into(),
            serde_json::json!({}),
            &temp.join("model"),
        )
        .unwrap();
        bundle.write_program(&temp.join("instance")).unwrap();
        assert!(temp.join("instance/empty").is_dir());
        assert_eq!(
            std::fs::read(temp.join("instance/asset.bin")).unwrap(),
            [0, 128, 255]
        );
        assert_eq!(
            std::fs::read(temp.join("instance/lib/brain.py")).unwrap(),
            b"value = 1\n"
        );
        for path in [
            "../escaped.py",
            "C:/escape.py",
            "main.py:stream",
            "/absolute.py",
            "alias./main.py",
        ] {
            bundle.files.insert(path.into(), BASE64.encode(b"invalid"));
            assert!(bundle.write_program(&temp.join("invalid")).is_err());
            assert!(!temp.join("invalid").exists());
            bundle.files.remove(path);
        }
        assert!(!temp.join("escaped.py").exists());
        std::fs::remove_dir_all(temp).unwrap();
    }
}
