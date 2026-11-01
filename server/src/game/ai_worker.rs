use super::ai::{AiContext, AiOutput};
use crate::{
    config::AiConfig,
    domain::organism::{AiRunMetrics, AiRunReport},
};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Instant,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
};

pub(super) struct Worker {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    workspace: WorkerWorkspace,
}

struct WorkerWorkspace(PathBuf);
impl Drop for WorkerWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Worker {
    pub async fn start(config: &AiConfig) -> anyhow::Result<Self> {
        let mut command = Command::new(&config.python);
        command
            .args([
                "-I",
                "-u",
                "-X",
                "utf8",
                "-c",
                include_str!("../../python/worker.py"),
            ])
            .current_dir(std::env::temp_dir())
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        for key in ["SystemRoot", "WINDIR", "TEMP", "TMP"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let workspace = WorkerWorkspace(
            std::env::temp_dir().join(format!("life-paper-ai-worker-{}", uuid::Uuid::new_v4())),
        );
        std::fs::create_dir(&workspace.0)?;
        let mut child = command.spawn()?;
        let mut worker = Self {
            input: child.stdin.take().unwrap(),
            output: BufReader::new(child.stdout.take().unwrap()),
            child,
            workspace,
        };
        tokio::time::timeout(std::time::Duration::from_millis(config.timeout_ms), async {
            worker.send(&json!({"sdk": include_str!("../../python/life_game.py"), "cache_entries": config.cache_entries, "workspace": worker.workspace.0})).await?;
            let (ready, _) = worker.receive().await?;
            anyhow::ensure!(ready["type"] == "ready", "invalid AI worker handshake");
            Ok::<_, anyhow::Error>(())
        }).await??;
        Ok(worker)
    }

    pub fn pid(&self) -> u32 {
        self.child.id().unwrap_or_default()
    }

    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    pub async fn stop(&mut self) {
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
    }

    async fn send(&mut self, message: &Value) -> anyhow::Result<()> {
        let mut bytes = serde_json::to_vec(message)?;
        bytes.push(b'\n');
        self.input.write_all(&bytes).await?;
        self.input.flush().await?;
        Ok(())
    }

    async fn receive(&mut self) -> anyhow::Result<(Value, usize)> {
        let mut line = Vec::new();
        loop {
            let buffer = self.output.fill_buf().await?;
            anyhow::ensure!(!buffer.is_empty(), "AI worker exited without a result");
            let end = buffer.iter().position(|byte| *byte == b'\n');
            let length = end.map_or(buffer.len(), |index| index + 1);
            anyhow::ensure!(
                line.len() + length <= 128 * 1024,
                "AI protocol message too large"
            );
            line.extend_from_slice(&buffer[..length]);
            self.output.consume(length);
            if end.is_some() {
                break;
            }
        }
        Ok((serde_json::from_slice(&line)?, line.len()))
    }

    pub async fn execute(
        &mut self,
        context: &AiContext,
        program: &Path,
        version: &str,
        max_queries: usize,
        metrics: &mut AiRunMetrics,
    ) -> anyhow::Result<(AiOutput, bool)> {
        let call_id = uuid::Uuid::new_v4().to_string();
        self.send(&json!({"call_id": call_id, "context": context.payload, "program": program, "version": version})).await?;
        let mut queries = 0usize;
        let mut commands = Vec::new();
        let mut received = 0;
        loop {
            let (message, size) = self.receive().await?;
            received += size;
            anyhow::ensure!(
                received <= 1024 * 1024,
                "AI protocol output budget exceeded"
            );
            anyhow::ensure!(message["call_id"] == call_id, "stale AI worker message");
            match message["type"].as_str() {
                Some("query") => {
                    let method = message["method"].as_str().unwrap_or("");
                    let cost = if method == "get_tiles" {
                        message["params"]["points"]
                            .as_array()
                            .map_or(1, |points| points.len().max(1))
                    } else {
                        1
                    };
                    queries = queries.saturating_add(cost);
                    anyhow::ensure!(queries <= max_queries, "AI world query budget exceeded");
                    let started = Instant::now();
                    let response = if method == "component_command" {
                        match context.component_command(&message["params"]) {
                            Ok(command) => {
                                // Each actuator has one output per call; the last setting wins.
                                commands.retain(|previous: &crate::domain::organism_component::ComponentCommand| previous.component_id != command.component_id);
                                let response = json!({"data": command});
                                commands.push(command);
                                response
                            }
                            Err(error) => error,
                        }
                    } else if method == "component_vision" {
                        json!({"error": {"code": "NOT_IMPLEMENTED", "message": "vision requires the world lighting and occlusion simulation"}})
                    } else {
                        context.query(method, &message["params"])
                    };
                    metrics.query_us += started.elapsed().as_micros() as u64;
                    self.send(&response).await?;
                }
                Some("result") => {
                    let ok = message["ok"]
                        .as_bool()
                        .ok_or_else(|| anyhow::anyhow!("invalid AI result"))?;
                    let memory = if ok {
                        let memory = message["memory"]
                            .as_object()
                            .ok_or_else(|| anyhow::anyhow!("invalid AI memory"))?
                            .clone();
                        anyhow::ensure!(
                            serde_json::to_vec(&memory)?.len() <= 16384,
                            "AI memory exceeds 16 KiB"
                        );
                        Some(memory)
                    } else {
                        None
                    };
                    metrics.code_cache_hit = message["code_cache_hit"].as_bool().unwrap_or(false);
                    metrics.load_us = message["load_us"].as_u64().unwrap_or_default();
                    metrics.execution_us = message["execution_us"].as_u64().unwrap_or_default();
                    return Ok((
                        AiOutput {
                            report: AiRunReport {
                                ok,
                                logs: message["logs"]
                                    .as_str()
                                    .unwrap_or("")
                                    .chars()
                                    .take(16384)
                                    .collect(),
                                error: if ok {
                                    None
                                } else {
                                    Some(
                                        message["error"]
                                            .as_str()
                                            .unwrap_or("AI execution failed")
                                            .chars()
                                            .take(8192)
                                            .collect(),
                                    )
                                },
                                duration_ms: 0,
                                commands: if ok { commands } else { Vec::new() },
                                metrics: Default::default(),
                            },
                            memory,
                        },
                        message["reusable"].as_bool().unwrap_or(false),
                    ));
                }
                _ => anyhow::bail!("invalid AI protocol message"),
            }
        }
    }
}
