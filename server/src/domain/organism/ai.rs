use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct AiState {
    pub calls: u64,
    pub memory: Map<String, Value>,
    pub last_run: Option<AiRunReport>,
    pub pending_commands: Vec<crate::domain::organism_component::ComponentCommand>,
    #[serde(default)]
    pub history: Vec<AiRunReport>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AiRunReport {
    pub ok: bool,
    pub logs: String,
    pub error: Option<String>,
    pub duration_ms: u64,
    #[serde(default)]
    pub commands: Vec<crate::domain::organism_component::ComponentCommand>,
    #[serde(default)]
    pub metrics: AiRunMetrics,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct AiRunMetrics {
    pub code_version: String,
    pub snapshot_cache_hit: bool,
    pub code_cache_hit: bool,
    pub worker_reused: bool,
    pub worker_pid: u32,
    pub snapshot_us: u64,
    pub startup_us: u64,
    pub load_us: u64,
    pub execution_us: u64,
    pub query_us: u64,
    pub roundtrip_us: u64,
    pub total_us: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_ai_state_saved_before_component_controls() {
        let state: AiState = serde_json::from_value(serde_json::json!({
            "calls": 1, "memory": {"count": 1},
            "last_run": {"ok": true, "logs": "", "error": null, "duration_ms": 1}
        }))
        .unwrap();
        assert!(state.pending_commands.is_empty());
        assert!(state.last_run.unwrap().commands.is_empty());
    }
}
