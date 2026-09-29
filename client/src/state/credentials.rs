use keyring::{Error, v1::Entry};
use serde::{Deserialize, Serialize};

const SERVICE_NAME: &str = "life-paper-session";

#[derive(Clone, Serialize, Deserialize)]
pub struct StoredSession {
    pub token: String,
    pub expires_at: String,
}

pub struct TokenStore;

impl TokenStore {
    pub fn load(server_url: &str) -> Result<Option<StoredSession>, String> {
        let entry = entry(server_url)?;
        match entry.get_password() {
            Ok(value) => serde_json::from_str(&value)
                .map(Some)
                .map_err(|error| format!("invalid stored session: {error}")),
            Err(Error::NoEntry) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    pub fn save(server_url: &str, session: &StoredSession) -> Result<(), String> {
        let value = serde_json::to_string(session).map_err(|error| error.to_string())?;
        entry(server_url)?
            .set_password(&value)
            .map_err(|error| error.to_string())
    }

    pub fn delete(server_url: &str) -> Result<(), String> {
        match entry(server_url)?.delete_credential() {
            Ok(()) | Err(Error::NoEntry) => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}

fn entry(server_url: &str) -> Result<Entry, String> {
    Entry::new(SERVICE_NAME, server_url).map_err(|error| error.to_string())
}
