use std::path::PathBuf;
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use super::mock::PureReviewResult;

// ── Persistent ACP Session ──

/// A long-lived Reasonix ACP process with an established session.
/// Owned by the CoagentServer, reused across all review_diff calls.
pub struct AcpSession {
    child: Child,
    stdin: tokio::process::ChildStdin,
    reader: BufReader<tokio::process::ChildStdout>,
    session_id: String,
    next_request_id: u64,
}

impl AcpSession {
    pub async fn connect(model: &str, cwd: &PathBuf) -> Result<Self, ReasonixError> {
        let reasonix_cmd =
            std::env::var("COAGENT_REASONIX_PATH").unwrap_or_else(|_| "reasonix".into());

        let mut child = Command::new(&reasonix_cmd)
            .arg("acp")
            .arg("--model")
            .arg(model)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| ReasonixError::Spawn(e.to_string()))?;

        let mut stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        let mut reader = BufReader::new(stdout);

        // ACP initialize
        send_frame(&mut stdin, 1, "initialize", &serde_json::json!({
            "protocolVersion": 1,
            "clientInfo": { "name": "coagent", "version": "0.1.0" }
        })).await?;
        read_line(&mut reader).await?; // ignore init response

        // ACP session/new
        send_frame(&mut stdin, 2, "session/new", &serde_json::json!({
            "cwd": cwd.to_string_lossy()
        })).await?;

        let session_resp = read_line(&mut reader).await?;
        let session: serde_json::Value = serde_json::from_str(&session_resp)
            .map_err(|e| ReasonixError::Protocol(format!("session/new: {e}")))?;
        let session_id = session["result"]["sessionId"]
            .as_str()
            .ok_or_else(|| ReasonixError::Protocol("missing sessionId".into()))?
            .to_string();

        Ok(Self {
            child,
            stdin,
            reader,
            session_id,
            next_request_id: 3,
        })
    }

    /// Send a session/prompt and collect the response.
    pub async fn send_prompt(&mut self, goal: &str, diff_path: &str) -> Result<PureReviewResult, ReasonixError> {
        let timeout_ms: u64 = std::env::var("COAGENT_AGENT_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(120_000);
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        let id = self.next_request_id;
        self.next_request_id += 2;

        let prompt = build_review_prompt(goal, diff_path);
        send_frame(&mut self.stdin, id, "session/prompt", &serde_json::json!({
            "sessionId": self.session_id,
            "prompt": [{ "type": "text", "text": prompt }]
        })).await?;

        let mut collected_text = String::new();
        loop {
            let line = tokio::time::timeout_at(deadline, read_line(&mut self.reader))
                .await
                .map_err(|_| ReasonixError::Timeout("ACP prompt timed out".into()))??;
            if line.is_empty() { continue; }
            let msg: serde_json::Value = serde_json::from_str(&line)
                .map_err(|e| ReasonixError::Protocol(format!("invalid frame: {e}")))?;

            if msg.get("id").and_then(|v| v.as_i64()) == Some(id as i64) {
                if let Some(err) = msg.get("error") {
                    return Err(ReasonixError::Protocol(
                        err.get("message").and_then(|v| v.as_str()).unwrap_or("unknown error").into(),
                    ));
                }
                break;
            }

            if msg.get("method").and_then(|v| v.as_str()) == Some("session/update") {
                if let Some(update) = msg.get("params").and_then(|p| p.get("update")) {
                    if update.get("sessionUpdate").and_then(|v| v.as_str()) == Some("agent_message_chunk") {
                        if let Some(text) = update.get("content").and_then(|c| c.get("text")).and_then(|v| v.as_str()) {
                            collected_text.push_str(text);
                        }
                    }
                }
            }
        }

        let review: PureReviewResult = serde_json::from_str(&collected_text)
            .or_else(|_| extract_json(&collected_text))
            .map_err(|e| ReasonixError::Protocol(format!("parse review: {e}")))?;
        Ok(review)
    }

    pub async fn shutdown(&mut self) {
        let _ = self.child.kill().await;
    }
}

// ── Reasonix Runner ──

use std::sync::Arc;

/// Reasonix backend. Holds a shared, lazily-initialized ACP session.
/// The session is created on first use and reused for subsequent calls.
#[derive(Clone)]
pub struct ReasonixRunner {
    model: String,
    cwd: PathBuf,
    session: Arc<Mutex<Option<AcpSession>>>,
}

impl ReasonixRunner {
    pub fn new(model: impl Into<String>, cwd: PathBuf) -> Self {
        Self {
            model: model.into(),
            cwd,
            session: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn run(
        &self,
        goal: &str,
        diff_path: &str,
    ) -> Result<PureReviewResult, ReasonixError> {
        let mut guard = self.session.lock().await;

        if guard.is_none() {
            let session = AcpSession::connect(&self.model, &self.cwd).await?;
            *guard = Some(session);
        }

        let session = guard.as_mut().unwrap();
        session.send_prompt(goal, diff_path).await
    }

    /// Explicitly close the session and cleanup.
    pub async fn shutdown(&self) {
        let mut guard = self.session.lock().await;
        if let Some(ref mut session) = *guard {
            session.shutdown().await;
        }
        *guard = None;
    }
}

// ── Helpers ──

async fn send_frame(
    stdin: &mut tokio::process::ChildStdin,
    id: u64,
    method: &str,
    params: &serde_json::Value,
) -> Result<(), ReasonixError> {
    let frame = serde_json::json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
    stdin.write_all(format!("{}\n", frame).as_bytes()).await.map_err(|e| ReasonixError::Io(e.to_string()))?;
    stdin.flush().await.map_err(|e| ReasonixError::Io(e.to_string()))
}

async fn read_line(reader: &mut BufReader<tokio::process::ChildStdout>) -> Result<String, ReasonixError> {
    let mut line = String::new();
    reader.read_line(&mut line).await.map_err(|e| ReasonixError::Io(e.to_string()))?;
    Ok(line)
}

#[derive(Debug, thiserror::Error)]
pub enum ReasonixError {
    #[error("spawn: {0}")] Spawn(String),
    #[error("I/O: {0}")] Io(String),
    #[error("protocol: {0}")] Protocol(String),
    #[error("timeout: {0}")] Timeout(String),
}

fn build_review_prompt(goal: &str, diff_path: &str) -> String {
    format!(
        "You are reviewing a code diff.\n\n\
         Review goal: {goal}\n\n\
         Artifacts:\n\
         - diff_path: {diff_path}\n\n\
         Read the diff file, analyze it, then return your review as a single JSON \
         object with this exact schema. Return ONLY the JSON, no other text:\n\
         {{\n  \"verdict\": \"pass\" | \"needs_fix\" | \"risky\" | \"unknown\",\n  \
         \"summary\": \"one-sentence summary\",\n  \
         \"findings\": [],\n  \
         \"tests_to_run\": [],\n  \
         \"risks\": [],\n  \
         \"assumptions\": [],\n  \
         \"confidence\": 0.0-1.0\n}}"
    )
}

fn extract_json(text: &str) -> Result<PureReviewResult, serde_json::Error> {
    if let Some(start) = text.find('{') {
        let slice = &text[start..];
        let mut end = slice.len();
        while end > 0 {
            if let Ok(v) = serde_json::from_str(&slice[..end]) { return Ok(v); }
            end = slice[..end].rfind('}').map(|i| i + 1).unwrap_or(0);
        }
    }
    serde_json::from_str(text)
}



