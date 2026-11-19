use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};
use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::{Layer, layer::Context};

const MAX_FILE: u64 = 10 * 1024 * 1024;
const MAX_RECORDS: usize = 5000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogRecord {
    pub id: u64,
    pub time: String,
    pub level: String,
    pub category: String,
    pub target: String,
    pub fields: Value,
}
struct Inner {
    records: VecDeque<LogRecord>,
    next: u64,
    file: Option<std::fs::File>,
    bytes: u64,
}
pub struct LogStore {
    path: PathBuf,
    inner: Mutex<Inner>,
}
#[derive(Default, Deserialize)]
pub struct LogQuery {
    pub before: Option<u64>,
    pub limit: Option<usize>,
    pub level: Option<String>,
    pub category: Option<String>,
    pub search: Option<String>,
}
impl LogStore {
    pub fn open(directory: PathBuf) -> std::io::Result<Arc<Self>> {
        std::fs::create_dir_all(&directory)?;
        let path = directory.join("events.jsonl");
        let mut records = VecDeque::new();
        for source in [directory.join("events.previous.jsonl"), path.clone()] {
            if let Ok(file) = std::fs::File::open(source) {
                for line in BufReader::new(file).lines().map_while(Result::ok) {
                    if let Ok(record) = serde_json::from_str::<LogRecord>(&line) {
                        records.push_back(record);
                        if records.len() > MAX_RECORDS {
                            records.pop_front();
                        }
                    }
                }
            }
        }
        let next = records.back().map_or(1, |record| record.id + 1);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        let bytes = file.metadata()?.len();
        Ok(Arc::new(Self {
            path,
            inner: Mutex::new(Inner {
                records,
                next,
                file: Some(file),
                bytes,
            }),
        }))
    }
    pub fn push(&self, level: &str, category: &str, target: &str, fields: Value) {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let record = LogRecord {
            id: inner.next,
            time: chrono::Utc::now().to_rfc3339(),
            level: level.into(),
            category: category.into(),
            target: target.into(),
            fields,
        };
        inner.next += 1;
        if inner.bytes >= MAX_FILE {
            inner.file.take();
            let previous = self.path.with_file_name("events.previous.jsonl");
            let rotation = (|| {
                if previous.exists() {
                    std::fs::remove_file(&previous)?;
                }
                std::fs::rename(&self.path, previous)
            })();
            inner.file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
                .ok();
            inner.bytes = inner
                .file
                .as_ref()
                .and_then(|f| f.metadata().ok())
                .map_or(0, |m| m.len());
            if let Err(error) = rotation {
                eprintln!("log rotation failed: {error}");
            }
        }
        if let Ok(mut bytes) = serde_json::to_vec(&record) {
            bytes.push(b'\n');
            if let Some(file) = &mut inner.file {
                if let Err(error) = file.write_all(&bytes) {
                    eprintln!("log write failed: {error}");
                }
            }
            inner.bytes += bytes.len() as u64;
        }
        inner.records.push_back(record);
        if inner.records.len() > MAX_RECORDS {
            inner.records.pop_front();
        }
    }
    pub fn query(&self, query: &LogQuery) -> Value {
        let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let search = query.search.as_deref().unwrap_or("").to_lowercase();
        let records: Vec<_> = inner
            .records
            .iter()
            .rev()
            .filter(|r| query.before.is_none_or(|before| r.id < before))
            .filter(|r| {
                query
                    .level
                    .as_ref()
                    .is_none_or(|v| v.is_empty() || *v == r.level)
            })
            .filter(|r| {
                query
                    .category
                    .as_ref()
                    .is_none_or(|v| v.is_empty() || *v == r.category)
            })
            .filter(|r| {
                search.is_empty()
                    || format!("{} {}", r.target, r.fields)
                        .to_lowercase()
                        .contains(&search)
            })
            .take(query.limit.unwrap_or(100).clamp(1, 500))
            .cloned()
            .collect();
        json!({"records": records, "next_before": records.last().map(|r| r.id), "oldest_id": inner.records.front().map(|r| r.id)})
    }
}

pub struct LogLayer(pub Arc<LogStore>);
struct Fields(serde_json::Map<String, Value>);
impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let name = field.name();
        if ["password", "token", "authorization", "cookie", "public_key"]
            .iter()
            .any(|s| name.to_lowercase().contains(s))
        {
            return;
        }
        let value = format!("{value:?}");
        self.0.insert(
            name.into(),
            Value::String(value.chars().take(4096).collect()),
        );
    }
}
impl<S: Subscriber> Layer<S> for LogLayer {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let mut fields = Fields(serde_json::Map::new());
        event.record(&mut fields);
        self.0.push(
            event.metadata().level().as_str(),
            "runtime",
            event.metadata().target(),
            Value::Object(fields.0),
        );
    }
}

pub async fn network(State(logs): State<Arc<LogStore>>, request: Request, next: Next) -> Response {
    let start = Instant::now();
    let method = request.method().to_string();
    // Query strings, headers and bodies can contain credentials or player code.
    let path: String = request.uri().path().chars().take(512).collect();
    let request_id = uuid::Uuid::new_v4().to_string();
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert("x-request-id", request_id.parse().unwrap());
    logs.push("INFO", "network", "http", json!({"request_id": request_id, "method": method, "path": path, "status": response.status().as_u16(), "duration_ms": start.elapsed().as_millis()}));
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::prelude::*;
    #[test]
    fn logs_persist_filter_and_page() {
        let path = std::env::temp_dir().join(format!("life-logs-{}", uuid::Uuid::new_v4()));
        let logs = LogStore::open(path.clone()).unwrap();
        logs.push("INFO", "network", "http", json!({"status": 401}));
        logs.push("ERROR", "runtime", "game", json!({"message": "AI failed"}));
        assert_eq!(
            logs.query(&LogQuery {
                category: Some("network".into()),
                ..Default::default()
            })["records"][0]["id"],
            1
        );
        assert_eq!(
            logs.query(&LogQuery {
                before: Some(2),
                ..Default::default()
            })["records"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        drop(logs);
        assert_eq!(
            LogStore::open(path.clone())
                .unwrap()
                .query(&LogQuery::default())["records"][0]["id"],
            2
        );
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn logs_rotate_and_redact_credential_fields() {
        let path = std::env::temp_dir().join(format!("life-log-rotation-{}", uuid::Uuid::new_v4()));
        let logs = LogStore::open(path.clone()).unwrap();
        let subscriber = tracing_subscriber::registry().with(LogLayer(logs.clone()));
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(
                token = "secret-value",
                password = "password-value",
                authorization = "bearer-value",
                message = "safe message"
            );
        });
        let encoded = logs.query(&LogQuery::default()).to_string();
        assert!(encoded.contains("safe message"));
        for secret in ["secret-value", "password-value", "bearer-value"] {
            assert!(!encoded.contains(secret));
        }
        logs.inner.lock().unwrap().bytes = MAX_FILE;
        logs.push("INFO", "runtime", "game", json!({"message":"rotated"}));
        assert!(path.join("events.previous.jsonl").is_file());
        drop(logs);
        let restored = LogStore::open(path.clone()).unwrap();
        assert_eq!(
            restored.query(&LogQuery::default())["records"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(restored.query(&LogQuery::default())["records"][0]["id"], 2);
        drop(restored);
        std::fs::remove_dir_all(path).unwrap();
    }
}
