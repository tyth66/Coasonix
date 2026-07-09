use std::io::ErrorKind;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Mutex as StdMutex;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use coagent_runtime_core::sandbox::SandboxConfig;

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

impl Drop for AcpSession {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

impl AcpSession {
    pub async fn connect(model: &str, cwd: &PathBuf) -> Result<Self, ReasonixError> {
        let reasonix_cmd =
            std::env::var("COAGENT_REASONIX_PATH").unwrap_or_else(|_| "reasonix".into());
        let sandbox = reasonix_sandbox();

        let mut child = Command::new(&reasonix_cmd)
            .arg("acp")
            .arg("--model")
            .arg(model)
            .current_dir(cwd)
            .env_clear()
            .envs(sandbox.filtered_env())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                if e.kind() == ErrorKind::NotFound {
                    ReasonixError::Spawn(format!(
                        "Reasonix CLI not found at '{reasonix_cmd}'. Set COAGENT_REASONIX_PATH or add reasonix to PATH. Original error: {e}"
                    ))
                } else {
                    ReasonixError::Spawn(format!(
                        "failed to start Reasonix CLI '{reasonix_cmd}'. Check COAGENT_REASONIX_PATH, PATH, and the sandbox environment. Original error: {e}"
                    ))
                }
            })?;

        let mut stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        let mut reader = BufReader::new(stdout);

        // ACP initialize
        send_frame(
            &mut stdin,
            1,
            "initialize",
            &serde_json::json!({
                "protocolVersion": 1,
                "clientInfo": { "name": "coagent", "version": "0.1.0" }
            }),
        )
        .await?;
        read_response_frame(&mut reader, 1, "Reasonix ACP initialize failed").await?;

        // ACP session/new
        send_frame(
            &mut stdin,
            2,
            "session/new",
            &serde_json::json!({
                "cwd": cwd.to_string_lossy()
            }),
        )
        .await?;

        let session =
            read_response_frame(&mut reader, 2, "Reasonix ACP session creation failed").await?;
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
    pub async fn send_prompt(
        &mut self,
        goal: &str,
        diff_path: &str,
        context: &crate::backends::context::ContextProjection,
        stats: &Arc<StdMutex<ReasonixRunnerStats>>,
    ) -> Result<PureReviewResult, ReasonixError> {
        let timeout_ms: u64 = std::env::var("COAGENT_AGENT_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(120_000);
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        let id = self.next_request_id;
        self.next_request_id += 2;

        let prompt = build_review_prompt(goal, diff_path, context);
        send_frame(
            &mut self.stdin,
            id,
            "session/prompt",
            &serde_json::json!({
                "sessionId": self.session_id,
                "prompt": [{ "type": "text", "text": prompt }]
            }),
        )
        .await?;

        let mut collected_text = String::new();
        let mut observed_tool_calls = 0u64;
        loop {
            let line = tokio::time::timeout_at(deadline, read_line(&mut self.reader))
                .await
                .map_err(|_| ReasonixError::Timeout("ACP prompt timed out".into()))??;
            if line.is_empty() {
                continue;
            }
            let msg: serde_json::Value = serde_json::from_str(&line)
                .map_err(|e| ReasonixError::Protocol(format!("invalid frame: {e}")))?;

            // Handle Reasonix outbound requests (e.g. session/request_permission)
            if msg.get("id").is_some()
                && msg.get("method").is_some()
                && msg.get("method").and_then(|v| v.as_str()) != Some("session/update")
                && msg.get("id").and_then(|v| v.as_i64()) != Some(id as i64)
            {
                let m = msg.get("method").and_then(|v| v.as_str()).unwrap_or("?");
                match m {
                    "session/request_permission" => {
                        let outcome = handle_permission_request(&msg, stats);
                        self.send_permission_response(
                            msg.get("id").and_then(|v| v.as_i64()).unwrap_or(0) as u64,
                            &outcome,
                        )
                        .await?;
                    }
                    _ => {
                        eprintln!("[coagent] ignoring unrecognized outbound request: {m}");
                    }
                }
                continue;
            }

            if msg.get("id").and_then(|v| v.as_i64()) == Some(id as i64) {
                if let Some(err) = msg.get("error") {
                    return Err(ReasonixError::Protocol(
                        err.get("message")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown error")
                            .into(),
                    ));
                }
                break;
            }

            if msg.get("method").and_then(|v| v.as_str()) == Some("session/update")
                && let Some(update) = msg.get("params").and_then(|p| p.get("update"))
            {
                match update.get("sessionUpdate").and_then(|v| v.as_str()) {
                    Some("agent_message_chunk") => {
                        if let Some(text) = update
                            .get("content")
                            .and_then(|c| c.get("text"))
                            .and_then(|v| v.as_str())
                        {
                            collected_text.push_str(text);
                        }
                    }
                    Some("tool_call") => {
                        if let Some(review) = try_parse_review(&collected_text) {
                            return Ok(review);
                        }
                        observed_tool_calls += 1;
                        if observed_tool_calls <= ReasonixRunner::MAX_OBSERVED_TOOL_CALLS_PER_PROMPT
                        {
                            let title = update.get("title").and_then(|v| v.as_str()).unwrap_or("?");
                            let kind = update.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
                            let tool_call_id = update
                                .get("toolCallId")
                                .and_then(|v| v.as_str())
                                .unwrap_or("?");
                            eprintln!(
                                "[coagent] tool_call #{}/{}: title={}, kind={}, id={}",
                                observed_tool_calls,
                                ReasonixRunner::MAX_OBSERVED_TOOL_CALLS_PER_PROMPT,
                                title,
                                kind,
                                tool_call_id
                            );
                        }
                        {
                            let mut stats = stats.lock().expect("stats in send_prompt");
                            stats.tool_call_count += 1;
                        }
                        if observed_tool_calls >= ReasonixRunner::MAX_OBSERVED_TOOL_CALLS_PER_PROMPT
                        {
                            return Err(ReasonixError::Protocol(format!(
                                "max observed tool calls ({}) exceeded: {} tool_call events before review JSON",
                                ReasonixRunner::MAX_OBSERVED_TOOL_CALLS_PER_PROMPT,
                                observed_tool_calls
                            )));
                        }
                    }
                    Some("tool_call_update") => {
                        let status = update.get("status").and_then(|v| v.as_str());
                        let mut stats = stats.lock().expect("stats in send_prompt");
                        stats.tool_call_update_count += 1;
                        match status {
                            Some("completed") => stats.completed_tool_call_count += 1,
                            Some("failed") => stats.failed_tool_call_count += 1,
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }

        parse_review_text(&collected_text)
    }

    async fn send_permission_response(
        &mut self,
        request_id: u64,
        outcome: &serde_json::Value,
    ) -> Result<(), ReasonixError> {
        let frame = serde_json::json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "result": outcome
        });
        self.stdin
            .write_all(format!("{}\n", frame).as_bytes())
            .await
            .map_err(|e| ReasonixError::Io(e.to_string()))?;
        self.stdin
            .flush()
            .await
            .map_err(|e| ReasonixError::Io(e.to_string()))
    }
}

// ── Reasonix Runner ──

use std::sync::Arc;

/// Reasonix-specific ACP runner. Holds a shared, lazily-initialized Reasonix
/// session; it does not yet honor arbitrary AgentProfile command/args.
/// The session is created on first use and reused for subsequent calls.
/// Runs are intentionally serialized by the session mutex for the full
/// send_prompt await, preserving ACP frame ordering and Reasonix context.
#[derive(Clone)]
pub struct ReasonixRunner {
    model: String,
    cwd: PathBuf,
    session: Arc<Mutex<Option<AcpSession>>>,
    stats: Arc<StdMutex<ReasonixRunnerStats>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct ReasonixRunnerStats {
    pub has_session: bool,
    pub session_created_count: u64,
    pub prompt_count: u64,
    pub reconnect_count: u64,
    pub timeout_count: u64,
    pub protocol_error_count: u64,
    pub io_error_count: u64,
    pub spawn_error_count: u64,
    pub tool_call_count: u64,
    pub tool_call_update_count: u64,
    pub completed_tool_call_count: u64,
    pub failed_tool_call_count: u64,
    pub permission_request_count: u64,
    pub auto_allowed_permission_count: u64,
    pub auto_rejected_permission_count: u64,
    pub last_error: Option<String>,
}

// -- Permission Request Handling --

fn handle_permission_request(
    msg: &serde_json::Value,
    stats: &Arc<StdMutex<ReasonixRunnerStats>>,
) -> serde_json::Value {
    let tool_call = msg.get("params").and_then(|p| p.get("toolCall"));
    let title = tool_call
        .and_then(|t| t.get("title"))
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let kind = tool_call
        .and_then(|t| t.get("kind"))
        .and_then(|v| v.as_str())
        .unwrap_or("?");

    let mut stats = stats.lock().expect("stats");
    stats.permission_request_count += 1;

    let allowed = is_read_only_tool(title, kind);
    let outcome = if allowed {
        stats.auto_allowed_permission_count += 1;
        serde_json::json!({ "outcome": "selected", "optionId": "allow_once" })
    } else {
        stats.auto_rejected_permission_count += 1;
        serde_json::json!({ "outcome": "selected", "optionId": "reject_once" })
    };

    eprintln!(
        "[coagent] permission_request: title={}, kind={}, allowed={}",
        title, kind, allowed
    );

    outcome
}

fn is_read_only_tool(title: &str, _kind: &str) -> bool {
    matches!(title, "read_file" | "grep" | "glob" | "ls" | "list_files")
}

impl ReasonixRunner {
    /// Maximum observed pre-review tool_call events before failing the prompt.
    const MAX_OBSERVED_TOOL_CALLS_PER_PROMPT: u64 = 5;

    pub fn new(model: impl Into<String>, cwd: PathBuf) -> Self {
        Self {
            model: model.into(),
            cwd,
            session: Arc::new(Mutex::new(None)),
            stats: Arc::new(StdMutex::new(ReasonixRunnerStats::default())),
        }
    }

    pub fn stats(&self) -> ReasonixRunnerStats {
        self.stats.lock().expect("reasonix stats mutex").clone()
    }

    pub async fn run(
        &self,
        goal: &str,
        diff_path: &str,
        context: &crate::backends::context::ContextProjection,
    ) -> Result<PureReviewResult, ReasonixError> {
        let mut guard = self.session.lock().await;

        if guard.is_none() {
            let session = match AcpSession::connect(&self.model, &self.cwd).await {
                Ok(session) => session,
                Err(error) => {
                    self.record_error(&error);
                    return Err(error);
                }
            };
            self.record_session_created();
            *guard = Some(session);
        }

        let first = {
            let session = guard.as_mut().unwrap();
            self.record_prompt();
            session
                .send_prompt(goal, diff_path, context, &self.stats)
                .await
        };

        match first {
            Ok(result) => Ok(result),
            Err(error) if error.should_drop_session() => {
                self.record_error(&error);
                eprintln!(
                    "[coagent] Reasonix session failed ({}), dropping session",
                    error
                );
                *guard = None;
                self.set_has_session(false);
                if !error.is_retryable() {
                    return Err(error);
                }
                self.record_reconnect();
                let mut session = match AcpSession::connect(&self.model, &self.cwd).await {
                    Ok(session) => session,
                    Err(connect_err) => {
                        self.record_error(&connect_err);
                        return Err(ReasonixError::Protocol(format!(
                            "session recovery failed after {}: {}",
                            error, connect_err
                        )));
                    }
                };
                self.record_session_created();
                self.record_prompt();
                let retry = session
                    .send_prompt(goal, diff_path, context, &self.stats)
                    .await;
                if let Err(retry_error) = &retry {
                    self.record_error(retry_error);
                }
                if retry
                    .as_ref()
                    .err()
                    .is_some_and(ReasonixError::should_drop_session)
                {
                    *guard = None;
                    self.set_has_session(false);
                } else {
                    *guard = Some(session);
                    self.set_has_session(true);
                }
                retry
            }
            Err(error) => {
                self.record_error(&error);
                Err(error)
            }
        }
    }

    fn record_session_created(&self) {
        let mut stats = self.stats.lock().expect("reasonix stats mutex");
        stats.has_session = true;
        stats.session_created_count += 1;
    }

    fn set_has_session(&self, has_session: bool) {
        self.stats.lock().expect("reasonix stats mutex").has_session = has_session;
    }

    fn record_prompt(&self) {
        self.stats
            .lock()
            .expect("reasonix stats mutex")
            .prompt_count += 1;
    }

    fn record_reconnect(&self) {
        self.stats
            .lock()
            .expect("reasonix stats mutex")
            .reconnect_count += 1;
    }

    fn record_error(&self, error: &ReasonixError) {
        let mut stats = self.stats.lock().expect("reasonix stats mutex");
        match error {
            ReasonixError::Spawn(_) => stats.spawn_error_count += 1,
            ReasonixError::Io(_) => stats.io_error_count += 1,
            ReasonixError::Protocol(_) => stats.protocol_error_count += 1,
            ReasonixError::Timeout(_) => stats.timeout_count += 1,
        }
        stats.last_error = Some(error.to_string());
    }
}

// ── Helpers ──

async fn send_frame(
    stdin: &mut tokio::process::ChildStdin,
    id: u64,
    method: &str,
    params: &serde_json::Value,
) -> Result<(), ReasonixError> {
    let frame =
        serde_json::json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
    stdin
        .write_all(format!("{}\n", frame).as_bytes())
        .await
        .map_err(|e| ReasonixError::Io(e.to_string()))?;
    stdin
        .flush()
        .await
        .map_err(|e| ReasonixError::Io(e.to_string()))
}

async fn read_line(
    reader: &mut BufReader<tokio::process::ChildStdout>,
) -> Result<String, ReasonixError> {
    let mut line = String::new();
    let bytes_read = reader
        .read_line(&mut line)
        .await
        .map_err(|e| ReasonixError::Io(e.to_string()))?;
    if bytes_read == 0 {
        return Err(ReasonixError::Protocol("ACP process closed stdout".into()));
    }
    Ok(line)
}

#[derive(Debug, thiserror::Error)]
pub enum ReasonixError {
    #[error("spawn: {0}")]
    Spawn(String),
    #[error("I/O: {0}")]
    Io(String),
    #[error("protocol: {0}")]
    Protocol(String),
    #[error("timeout: {0}")]
    Timeout(String),
}

impl ReasonixError {
    /// Errors that poison the current ACP stream and require dropping the session.
    fn should_drop_session(&self) -> bool {
        matches!(self, Self::Io(_) | Self::Protocol(_) | Self::Timeout(_))
    }

    /// Errors that are safe to retry once after reconnecting.
    fn is_retryable(&self) -> bool {
        matches!(self, Self::Io(_) | Self::Protocol(_))
            && !self.to_string().contains("max observed tool calls")
    }
}

fn reasonix_sandbox() -> SandboxConfig {
    SandboxConfig::new().with_env_allowlist(vec![
        "PATH".into(),
        "PATHEXT".into(),
        "SYSTEMROOT".into(),
        "WINDIR".into(),
        "COMSPEC".into(),
        "APPDATA".into(),
        "HOME".into(),
        "USERPROFILE".into(),
        "DEEPSEEK_API_KEY".into(),
        "COAGENT_FAKE_REASONIX_CASE".into(),
    ])
}

/// Prompt template for Reasonix backend, generated from coagent-v1.schema.json.
/// This ensures the model output strictly matches the PureReviewResult schema.
pub const REASONIX_REVIEW_PROMPT_TEMPLATE: &str = r#"
You are reviewing a code diff.

GOAL: {goal}
DIFF PATH: {diff_path}
{context_section}
Read the available files above, analyze the diff, then return your review as a single JSON object.
Do not call tools, tasks, commands, or external agents.
Return ONLY the JSON. No markdown, no explanation, no surrounding text.

{
  "verdict": "PICK_ONE: pass | needs_fix | risky | unknown | not_applicable",
  "summary": "concise one-sentence summary",
  "findings": [
    {
      "id": "F-001",
      "severity": "PICK_ONE: blocker | major | minor | note",
      "category": "security | correctness | performance | style | maintainability | testing | documentation | architecture",
      "file": "relative/path/to/file.ext",
      "line": 42,
      "issue": "concise description of the problem",
      "evidence": "specific code or reasoning",
      "recommendation": "suggested fix",
      "confidence": 0.95
    }
  ],
  "tests_to_run": ["cargo test -p ..."],
  "risks": ["potential risk if merged as-is"],
  "assumptions": ["assumption made during review"],
  "confidence": 0.85
}

RULES:
1. verdict MUST be one of: pass, needs_fix, risky, unknown, not_applicable
2. severity MUST be one of: blocker, major, minor, note
3. Every finding REQUIRES: severity, category, issue, confidence
4. confidence is 0.0-1.0 (0 = completely uncertain, 1 = completely certain)
5. If no issues found: "findings": []
6. Return ONLY the JSON object
7. Do not call tools, tasks, commands, or external agents
"#;

fn build_review_prompt(
    goal: &str,
    diff_path: &str,
    context: &crate::backends::context::ContextProjection,
) -> String {
    REASONIX_REVIEW_PROMPT_TEMPLATE
        .replace("{goal}", goal)
        .replace("{diff_path}", diff_path)
        .replace("{context_section}", &context.render_context_section())
}

async fn read_response_frame(
    reader: &mut BufReader<tokio::process::ChildStdout>,
    expected_id: u64,
    context: &str,
) -> Result<serde_json::Value, ReasonixError> {
    loop {
        let line = read_line(reader).await?;
        let frame: serde_json::Value = serde_json::from_str(&line)
            .map_err(|e| ReasonixError::Protocol(format!("{context}: invalid frame: {e}")))?;
        let Some(id) = frame.get("id").and_then(|v| v.as_u64()) else {
            if frame.get("method").is_some() {
                continue;
            }
            return Err(ReasonixError::Protocol(format!(
                "{context}: response frame missing id"
            )));
        };
        if id != expected_id {
            return Err(ReasonixError::Protocol(format!(
                "{context}: unexpected response id"
            )));
        }
        if let Some(error) = frame.get("error") {
            return Err(ReasonixError::Protocol(format!(
                "{context}: {}",
                error
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown error")
            )));
        }
        return Ok(frame);
    }
}

fn extract_json(text: &str) -> Result<PureReviewResult, serde_json::Error> {
    if let Some(start) = text.find('{') {
        let slice = &text[start..];
        let mut end = slice.len();
        while end > 0 {
            if let Ok(v) = serde_json::from_str(&slice[..end]) {
                return Ok(v);
            }
            end = slice[..end].rfind('}').map(|i| i + 1).unwrap_or(0);
        }
    }
    serde_json::from_str(text)
}

fn parse_review_text(text: &str) -> Result<PureReviewResult, ReasonixError> {
    serde_json::from_str(text)
        .or_else(|_| extract_json(text))
        .map_err(|e| ReasonixError::Protocol(format!("parse review: {e}")))
}

fn try_parse_review(text: &str) -> Option<PureReviewResult> {
    serde_json::from_str(text)
        .or_else(|_| extract_json(text))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::context::ContextProjection;

    use std::fs;
    use std::sync::LazyLock;

    static ENV_LOCK: LazyLock<tokio::sync::Mutex<()>> =
        LazyLock::new(|| tokio::sync::Mutex::new(()));

    #[tokio::test]
    async fn reasonix_contract_collects_acp_chunks_into_review_result() {
        let _guard = ENV_LOCK.lock().await;
        let fake = FakeReasonix::new("success");
        let _env = TestEnv::set(&[
            ("COAGENT_REASONIX_PATH", fake.executable_path()),
            ("COAGENT_FAKE_REASONIX_CASE", "success".into()),
            ("COAGENT_AGENT_TIMEOUT_MS", "1000".into()),
        ]);

        let runner = ReasonixRunner::new("fake-model", fake.dir.clone());

        let result = runner
            .run(
                "review the diff",
                "changes.diff",
                &ContextProjection::default(),
            )
            .await
            .unwrap();

        assert_eq!(result.verdict, "needs_fix");
        assert_eq!(result.summary, "Fake ACP review.");
        assert_eq!(
            result.tests_to_run,
            vec!["cargo test -p coagent-mcp-server"]
        );
        assert_eq!(result.confidence, 0.73);
    }

    #[tokio::test]
    async fn reasonix_runner_reuses_one_session_for_multiple_prompts() {
        let _guard = ENV_LOCK.lock().await;
        let fake = FakeReasonix::new("persistent-reuse");
        let _env = TestEnv::set(&[
            ("COAGENT_REASONIX_PATH", fake.executable_path()),
            ("COAGENT_FAKE_REASONIX_CASE", "success".into()),
            ("COAGENT_AGENT_TIMEOUT_MS", "1000".into()),
        ]);

        let runner = ReasonixRunner::new("fake-model", fake.dir.clone());
        let context = ContextProjection::default();

        runner
            .run("first review", "changes.diff", &context)
            .await
            .unwrap();
        runner
            .run("second review", "changes.diff", &context)
            .await
            .unwrap();

        assert_eq!(fake.count("spawn"), 1);
        assert_eq!(fake.count("initialize"), 1);
        assert_eq!(fake.count("session_new"), 1);
        assert_eq!(fake.count("prompt"), 2);

        let stats = runner.stats();
        assert!(stats.has_session);
        assert_eq!(stats.session_created_count, 1);
        assert_eq!(stats.prompt_count, 2);
        assert_eq!(stats.reconnect_count, 0);
        assert_eq!(stats.timeout_count, 0);
        assert_eq!(stats.protocol_error_count, 0);
        assert_eq!(stats.io_error_count, 0);
        assert_eq!(stats.spawn_error_count, 0);
        assert_eq!(stats.last_error, None);
    }

    #[tokio::test]
    async fn reasonix_runner_reconnects_and_retries_after_protocol_eof() {
        let _guard = ENV_LOCK.lock().await;
        let fake = FakeReasonix::new("prompt-eof-once");
        let _env = TestEnv::set(&[
            ("COAGENT_REASONIX_PATH", fake.executable_path()),
            ("COAGENT_FAKE_REASONIX_CASE", "prompt_eof_once".into()),
            ("COAGENT_AGENT_TIMEOUT_MS", "1000".into()),
        ]);

        let runner = ReasonixRunner::new("fake-model", fake.dir.clone());

        let result = runner
            .run(
                "review after recoverable eof",
                "changes.diff",
                &ContextProjection::default(),
            )
            .await
            .unwrap();

        assert_eq!(result.verdict, "needs_fix");
        assert_eq!(fake.count("spawn"), 2);
        assert_eq!(fake.count("initialize"), 2);
        assert_eq!(fake.count("session_new"), 2);
        assert_eq!(fake.count("prompt"), 2);

        let stats = runner.stats();
        assert!(stats.has_session);
        assert_eq!(stats.session_created_count, 2);
        assert_eq!(stats.prompt_count, 2);
        assert_eq!(stats.reconnect_count, 1);
        assert_eq!(stats.timeout_count, 0);
        assert_eq!(stats.protocol_error_count, 1);
        assert_eq!(stats.io_error_count, 0);
        assert_eq!(stats.spawn_error_count, 0);
        assert!(
            stats
                .last_error
                .as_deref()
                .is_some_and(|error| error.contains("ACP process closed stdout"))
        );
    }

    #[tokio::test]
    async fn reasonix_runner_drops_timed_out_session_without_retry() {
        let _guard = ENV_LOCK.lock().await;
        let fake = FakeReasonix::new("prompt-timeout-once");
        let _env = TestEnv::set(&[
            ("COAGENT_REASONIX_PATH", fake.executable_path()),
            ("COAGENT_FAKE_REASONIX_CASE", "prompt_timeout_once".into()),
            ("COAGENT_AGENT_TIMEOUT_MS", "100".into()),
        ]);

        let runner = ReasonixRunner::new("fake-model", fake.dir.clone());
        let context = ContextProjection::default();

        let error = runner
            .run("first prompt times out", "changes.diff", &context)
            .await
            .unwrap_err();
        assert!(matches!(error, ReasonixError::Timeout(_)));
        assert_eq!(fake.count("spawn"), 1);
        assert_eq!(fake.count("prompt"), 1);

        let stats = runner.stats();
        assert!(!stats.has_session);
        assert_eq!(stats.session_created_count, 1);
        assert_eq!(stats.prompt_count, 1);
        assert_eq!(stats.reconnect_count, 0);
        assert_eq!(stats.timeout_count, 1);
        assert_eq!(stats.protocol_error_count, 0);
        assert_eq!(stats.io_error_count, 0);
        assert_eq!(stats.spawn_error_count, 0);
        assert!(
            stats
                .last_error
                .as_deref()
                .is_some_and(|error| error.contains("ACP prompt timed out"))
        );

        let result = runner
            .run("second prompt uses fresh session", "changes.diff", &context)
            .await
            .unwrap();

        assert_eq!(result.verdict, "needs_fix");
        assert_eq!(fake.count("spawn"), 2);
        assert_eq!(fake.count("initialize"), 2);
        assert_eq!(fake.count("session_new"), 2);
        assert_eq!(fake.count("prompt"), 2);

        let stats = runner.stats();
        assert!(stats.has_session);
        assert_eq!(stats.session_created_count, 2);
        assert_eq!(stats.prompt_count, 2);
        assert_eq!(stats.timeout_count, 1);
    }

    #[tokio::test]
    async fn reasonix_prompt_includes_full_review_context() {
        let _guard = ENV_LOCK.lock().await;
        let fake = FakeReasonix::new("prompt-capture");
        let _env = TestEnv::set(&[
            ("COAGENT_REASONIX_PATH", fake.executable_path()),
            ("COAGENT_FAKE_REASONIX_CASE", "success".into()),
            ("COAGENT_AGENT_TIMEOUT_MS", "1000".into()),
        ]);

        let runner = ReasonixRunner::new("fake-model", fake.dir.clone());
        let context = ContextProjection {
            goal: "review full context".into(),
            diff_path: ".agent/diffs/current.diff".into(),
            context_path: Some(".agent/context/review.md".into()),
            test_log_path: Some(".agent/logs/test.log".into()),
            build_log_path: Some(".agent/logs/build.log".into()),
            focus: vec!["correctness".into(), "policy".into()],
            constraints: vec!["avoid new dependencies".into()],
            base_branch: Some("main".into()),
            working_branch: Some("feature/single-runner".into()),
        };

        runner
            .run(&context.goal, &context.diff_path, &context)
            .await
            .unwrap();

        let prompt_frame = fake.prompt_frame(1);
        for expected in [
            "GOAL: review full context",
            "DIFF PATH: .agent/diffs/current.diff",
            ".agent/context/review.md",
            ".agent/logs/test.log",
            ".agent/logs/build.log",
            "BASE BRANCH: main",
            "WORKING BRANCH: feature/single-runner",
            "FOCUS AREAS",
            "correctness",
            "policy",
            "CONSTRAINTS",
            "avoid new dependencies",
            "Do not call tools, tasks, commands, or external agents.",
        ] {
            assert!(
                prompt_frame.contains(expected),
                "prompt frame missing {expected}: {prompt_frame}"
            );
        }
    }

    #[tokio::test]
    async fn reasonix_returns_collected_review_when_tool_call_follows_valid_json() {
        let _guard = ENV_LOCK.lock().await;
        let fake = FakeReasonix::new("tool-call-after-review");
        let _env = TestEnv::set(&[
            ("COAGENT_REASONIX_PATH", fake.executable_path()),
            (
                "COAGENT_FAKE_REASONIX_CASE",
                "tool_call_after_review".into(),
            ),
            ("COAGENT_AGENT_TIMEOUT_MS", "1000".into()),
        ]);

        let runner = ReasonixRunner::new("fake-model", fake.dir.clone());

        let result = runner
            .run(
                "review with trailing unsupported tool call",
                "changes.diff",
                &ContextProjection::default(),
            )
            .await
            .unwrap();

        assert_eq!(result.verdict, "needs_fix");
        assert_eq!(result.summary, "Fake ACP review.");
        assert_eq!(runner.stats().prompt_count, 1);
    }

    #[tokio::test]
    async fn reasonix_observes_tool_call_before_review_without_sending_tool_result() {
        let _guard = ENV_LOCK.lock().await;
        let fake = FakeReasonix::new("tool-call-before-review");
        let _env = TestEnv::set(&[
            ("COAGENT_REASONIX_PATH", fake.executable_path()),
            (
                "COAGENT_FAKE_REASONIX_CASE",
                "tool_call_before_review".into(),
            ),
            ("COAGENT_AGENT_TIMEOUT_MS", "1000".into()),
        ]);

        let runner = ReasonixRunner::new("fake-model", fake.dir.clone());

        let result = runner
            .run(
                "review after denying tool call",
                "changes.diff",
                &ContextProjection::default(),
            )
            .await
            .unwrap();

        assert_eq!(result.verdict, "needs_fix");
        assert_eq!(
            fake.count("session_tool_result"),
            0,
            "Reasonix tool_call is an internal event notification, not a host-tool request"
        );
        let stats = runner.stats();
        assert_eq!(stats.tool_call_count, 1);
        assert_eq!(stats.tool_call_update_count, 1);
        assert_eq!(stats.completed_tool_call_count, 1);
        assert_eq!(stats.failed_tool_call_count, 0);
        assert_eq!(stats.prompt_count, 1);
    }

    #[tokio::test]
    async fn reasonix_rejects_prompt_after_max_tool_calls_exceeded() {
        let _guard = ENV_LOCK.lock().await;
        let fake = FakeReasonix::new("tool-call-loop");
        let _env = TestEnv::set(&[
            ("COAGENT_REASONIX_PATH", fake.executable_path()),
            ("COAGENT_FAKE_REASONIX_CASE", "tool_call_loop_6".into()),
            ("COAGENT_AGENT_TIMEOUT_MS", "1000".into()),
        ]);

        let runner = ReasonixRunner::new("fake-model", fake.dir.clone());

        let error = runner
            .run(
                "review with tool call loop",
                "changes.diff",
                &ContextProjection::default(),
            )
            .await
            .unwrap_err();

        assert!(matches!(error, ReasonixError::Protocol(_)));
        assert!(error.to_string().contains("max observed tool calls"));
        let stats = runner.stats();
        assert_eq!(
            stats.tool_call_count,
            ReasonixRunner::MAX_OBSERVED_TOOL_CALLS_PER_PROMPT
        );
        assert_eq!(stats.tool_call_update_count, 0);
    }

    #[tokio::test]
    async fn reasonix_spawn_error_mentions_path_configuration() {
        let _guard = ENV_LOCK.lock().await;
        let fake = FakeReasonix::new("missing-executable");
        let missing = fake.dir.join("missing-reasonix");
        let _env = TestEnv::set(&[
            (
                "COAGENT_REASONIX_PATH",
                missing.to_string_lossy().into_owned(),
            ),
            ("COAGENT_AGENT_TIMEOUT_MS", "1000".into()),
        ]);

        let runner = ReasonixRunner::new("fake-model", fake.dir.clone());

        let error = runner
            .run(
                "review with missing cli",
                "changes.diff",
                &ContextProjection::default(),
            )
            .await
            .unwrap_err();

        let message = error.to_string();
        assert!(matches!(error, ReasonixError::Spawn(_)));
        assert!(message.contains("Reasonix CLI not found"));
        assert!(message.contains("COAGENT_REASONIX_PATH"));
        assert!(message.contains("PATH"));

        let stats = runner.stats();
        assert!(!stats.has_session);
        assert_eq!(stats.session_created_count, 0);
        assert_eq!(stats.prompt_count, 0);
        assert_eq!(stats.spawn_error_count, 1);
        assert_eq!(stats.timeout_count, 0);
        assert!(
            stats
                .last_error
                .as_deref()
                .is_some_and(|error| error.contains("Reasonix CLI not found"))
        );
    }

    #[tokio::test]
    async fn reasonix_contract_surfaces_initialize_error_message() {
        let _guard = ENV_LOCK.lock().await;
        let fake = FakeReasonix::new("initialize-error");
        let _env = TestEnv::set(&[
            ("COAGENT_REASONIX_PATH", fake.executable_path()),
            ("COAGENT_FAKE_REASONIX_CASE", "initialize_error".into()),
            ("COAGENT_AGENT_TIMEOUT_MS", "1000".into()),
        ]);

        let runner = ReasonixRunner::new("fake-model", fake.dir.clone());

        let error = runner
            .run(
                "review the diff",
                "changes.diff",
                &ContextProjection::default(),
            )
            .await
            .unwrap_err();

        assert!(matches!(error, ReasonixError::Protocol(_)));
        assert!(error.to_string().contains("initialize rejected"));
    }

    #[tokio::test]
    async fn reasonix_contract_surfaces_session_new_error_message() {
        let _guard = ENV_LOCK.lock().await;
        let fake = FakeReasonix::new("session-new-error");
        let _env = TestEnv::set(&[
            ("COAGENT_REASONIX_PATH", fake.executable_path()),
            ("COAGENT_FAKE_REASONIX_CASE", "session_new_error".into()),
            ("COAGENT_AGENT_TIMEOUT_MS", "1000".into()),
        ]);

        let runner = ReasonixRunner::new("fake-model", fake.dir.clone());

        let error = runner
            .run(
                "review the diff",
                "changes.diff",
                &ContextProjection::default(),
            )
            .await
            .unwrap_err();

        assert!(matches!(error, ReasonixError::Protocol(_)));
        assert!(error.to_string().contains("session rejected"));
    }

    #[tokio::test]
    async fn reasonix_connect_ignores_session_update_before_session_new_response() {
        let _guard = ENV_LOCK.lock().await;
        let fake = FakeReasonix::new("session-update-before-response");
        let _env = TestEnv::set(&[
            ("COAGENT_REASONIX_PATH", fake.executable_path()),
            (
                "COAGENT_FAKE_REASONIX_CASE",
                "session_new_update_before_response".into(),
            ),
            ("COAGENT_AGENT_TIMEOUT_MS", "1000".into()),
        ]);

        let runner = ReasonixRunner::new("fake-model", fake.dir.clone());

        let result = runner
            .run(
                "review despite session update notification",
                "changes.diff",
                &ContextProjection::default(),
            )
            .await
            .unwrap();

        assert_eq!(result.verdict, "needs_fix");
        let stats = runner.stats();
        assert!(stats.has_session);
        assert_eq!(stats.session_created_count, 1);
    }

    #[tokio::test]
    async fn reasonix_contract_treats_prompt_process_exit_as_protocol_eof() {
        let _guard = ENV_LOCK.lock().await;
        let fake = FakeReasonix::new("prompt-eof");
        let _env = TestEnv::set(&[
            ("COAGENT_REASONIX_PATH", fake.executable_path()),
            ("COAGENT_FAKE_REASONIX_CASE", "prompt_eof".into()),
            ("COAGENT_AGENT_TIMEOUT_MS", "100".into()),
        ]);

        let runner = ReasonixRunner::new("fake-model", fake.dir.clone());

        let error = runner
            .run(
                "review the diff",
                "changes.diff",
                &ContextProjection::default(),
            )
            .await
            .unwrap_err();

        assert!(matches!(error, ReasonixError::Protocol(_)));
        assert!(error.to_string().contains("ACP process closed stdout"));
    }

    #[tokio::test]
    async fn reasonix_contract_rejects_unparseable_review_json() {
        let _guard = ENV_LOCK.lock().await;
        let fake = FakeReasonix::new("invalid-review");
        let _env = TestEnv::set(&[
            ("COAGENT_REASONIX_PATH", fake.executable_path()),
            ("COAGENT_FAKE_REASONIX_CASE", "invalid_review".into()),
            ("COAGENT_AGENT_TIMEOUT_MS", "1000".into()),
        ]);

        let runner = ReasonixRunner::new("fake-model", fake.dir.clone());

        let error = runner
            .run(
                "review the diff",
                "changes.diff",
                &ContextProjection::default(),
            )
            .await
            .unwrap_err();

        assert!(matches!(error, ReasonixError::Protocol(_)));
        assert!(error.to_string().contains("parse review"));
    }

    struct FakeReasonix {
        dir: PathBuf,
        executable: PathBuf,
    }

    impl FakeReasonix {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "coagent-reasonix-contract-{name}-{}",
                uuid::Uuid::new_v4()
            ));
            fs::create_dir_all(&dir).unwrap();

            #[cfg(windows)]
            let executable = create_windows_fake_reasonix(&dir);
            #[cfg(not(windows))]
            let executable = create_unix_fake_reasonix(&dir);

            Self { dir, executable }
        }

        fn executable_path(&self) -> String {
            self.executable.to_string_lossy().into_owned()
        }

        fn count(&self, name: &str) -> u64 {
            fs::read_to_string(self.dir.join(format!("{name}_count.txt")))
                .ok()
                .and_then(|value| value.trim().parse().ok())
                .unwrap_or(0)
        }

        fn prompt_frame(&self, index: u64) -> String {
            fs::read_to_string(self.dir.join(format!("prompt_{index}.json"))).unwrap_or_default()
        }
    }

    impl Drop for FakeReasonix {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    struct TestEnv {
        previous: Vec<(&'static str, Option<String>)>,
    }

    impl TestEnv {
        fn set(values: &[(&'static str, String)]) -> Self {
            let previous = values
                .iter()
                .map(|(key, _)| (*key, std::env::var(key).ok()))
                .collect::<Vec<_>>();
            for (key, value) in values {
                unsafe { std::env::set_var(key, value) };
            }
            Self { previous }
        }
    }

    impl Drop for TestEnv {
        fn drop(&mut self) {
            for (key, value) in &self.previous {
                if let Some(value) = value {
                    unsafe { std::env::set_var(key, value) };
                } else {
                    unsafe { std::env::remove_var(key) };
                }
            }
        }
    }

    #[cfg(windows)]
    fn create_windows_fake_reasonix(dir: &std::path::Path) -> PathBuf {
        let cmd_path = dir.join("fake-reasonix.cmd");
        let ps1_path = dir.join("fake-reasonix.ps1");
        fs::write(
            &cmd_path,
            format!(
                "@echo off\r\npowershell -NoProfile -ExecutionPolicy Bypass -File \"{}\" %*\r\n",
                ps1_path.display()
            ),
        )
        .unwrap();
        fs::write(&ps1_path, fake_reasonix_powershell()).unwrap();
        cmd_path
    }

    #[cfg(not(windows))]
    fn create_unix_fake_reasonix(dir: &std::path::Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let script_path = dir.join("fake-reasonix");
        fs::write(&script_path, fake_reasonix_shell()).unwrap();
        let mut permissions = fs::metadata(&script_path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions).unwrap();
        script_path
    }

    #[cfg(windows)]
    fn fake_reasonix_powershell() -> &'static str {
        r#"
$case = $env:COAGENT_FAKE_REASONIX_CASE

function Increment-Count($name) {
    $path = Join-Path (Get-Location) "$($name)_count.txt"
    $value = 0
    if (Test-Path $path) {
        $text = Get-Content -Raw $path
        if ($text) {
            $value = [int]$text.Trim()
        }
    }
    Set-Content -NoNewline -Path $path -Value ($value + 1)
}

function Read-Count($name) {
    $path = Join-Path (Get-Location) "$($name)_count.txt"
    if (Test-Path $path) {
        $text = Get-Content -Raw $path
        if ($text) {
            return [int]$text.Trim()
        }
    }
    return 0
}

function Write-Frame($frame) {
    [Console]::Out.WriteLine(($frame | ConvertTo-Json -Compress -Depth 20))
    [Console]::Out.Flush()
}

Increment-Count "spawn"

while ($null -ne ($line = [Console]::In.ReadLine())) {
    $msg = $line | ConvertFrom-Json

    if ($msg.method -eq "initialize") {
        Increment-Count "initialize"
        if ($case -eq "initialize_error") {
            Write-Frame @{ jsonrpc = "2.0"; id = $msg.id; error = @{ code = -32000; message = "initialize rejected" } }
            exit 0
        }
        Write-Frame @{ jsonrpc = "2.0"; id = $msg.id; result = @{ ok = $true } }
    }
    elseif ($msg.method -eq "session/new") {
        Increment-Count "session_new"
        if ($case -eq "session_new_error") {
            Write-Frame @{ jsonrpc = "2.0"; id = $msg.id; error = @{ code = -32001; message = "session rejected" } }
            exit 0
        }
        if ($case -eq "session_new_update_before_response") {
            Write-Frame @{ jsonrpc = "2.0"; method = "session/update"; params = @{ sessionId = "fake-session"; update = @{ sessionUpdate = "available_commands_update"; availableCommands = @() } } }
        }
        Write-Frame @{ jsonrpc = "2.0"; id = $msg.id; result = @{ sessionId = "fake-session" } }
    }
    elseif ($msg.method -eq "session/prompt") {
        Increment-Count "prompt"
        $promptCount = Read-Count "prompt"
        Set-Content -Path (Join-Path (Get-Location) "prompt_$($promptCount).json") -Value $line
        if ($case -eq "prompt_eof") {
            exit 0
        }
        if (($case -eq "prompt_eof_once") -and ($promptCount -eq 1)) {
            exit 0
        }
        if (($case -eq "prompt_timeout_once") -and ($promptCount -eq 1)) {
            Start-Sleep -Milliseconds 1000
            exit 0
        }

        if ($case -eq "invalid_review") {
            $chunks = @("not valid json")
        } elseif ($case -eq "tool_call_before_review" -or $case -eq "tool_call_loop_6") {
            $chunks = @("let me think...")
        }
        else {
            $chunks = @(
                '{"verdict":"needs_fix","summary":"Fake ACP review.",',
                '"findings":[],"tests_to_run":["cargo test -p coagent-mcp-server"],',
                '"risks":[],"assumptions":[],"confidence":0.73}'
            )
        }

        foreach ($chunk in $chunks) {
            Write-Frame @{
                jsonrpc = "2.0"
                method = "session/update"
                params = @{
                    sessionId = "fake-session"
                    update = @{
                        sessionUpdate = "agent_message_chunk"
                        content = @{ type = "text"; text = $chunk }
                    }
                }
            }
        }

        if ($case -eq "tool_call_after_review") {
            Write-Frame @{
                jsonrpc = "2.0"
                method = "session/update"
                params = @{
                    sessionId = "fake-session"
                    update = @{
                        sessionUpdate = "tool_call"
                        toolCallId = "call_fake"
                        title = "task"
                        kind = "other"
                        status = "pending"
                        rawInput = @{ description = "unneeded after valid JSON" }
                    }
                }
            }
            Start-Sleep -Milliseconds 1000
            exit 0
        }

        if ($case -eq "tool_call_before_review") {
            $chunks = @("let me think...")
            Write-Frame @{
                jsonrpc = "2.0"
                method = "session/update"
                params = @{
                    sessionId = "fake-session"
                    update = @{
                        sessionUpdate = "agent_message_chunk"
                        content = @{ type = "text"; text = "let me think..." }
                    }
                }
            }
            Write-Frame @{
                jsonrpc = "2.0"
                method = "session/update"
                params = @{
                    sessionId = "fake-session"
                    update = @{
                        sessionUpdate = "tool_call"
                        toolCallId = "call_before_review"
                        title = "task"
                        kind = "other"
                        status = "pending"
                        rawInput = @{ description = "need a tool before reviewing" }
                    }
                }
            }
            Start-Sleep -Milliseconds 100
            Write-Frame @{
                jsonrpc = "2.0"
                method = "session/update"
                params = @{
                    sessionId = "fake-session"
                    update = @{
                        sessionUpdate = "tool_call_update"
                        toolCallId = "call_before_review"
                        status = "completed"
                        content = @(@{ type = "text"; text = "internal tool completed" })
                    }
                }
            }
            # Reasonix emits its own tool_call_update, then continues with review JSON chunks.
            foreach ($chunk in @(
                '{"verdict":"needs_fix","summary":"Fake ACP review.",';
                '"findings":[],"tests_to_run":["cargo test -p coagent-mcp-server"],';
                '"risks":[],"assumptions":[],"confidence":0.73}'
            )) {
                Write-Frame @{
                    jsonrpc = "2.0"
                    method = "session/update"
                    params = @{
                        sessionId = "fake-session"
                        update = @{
                            sessionUpdate = "agent_message_chunk"
                            content = @{ type = "text"; text = $chunk }
                        }
                    }
                }
            }
        }

        if ($case -eq "tool_call_loop_6") {
            for ($j = 1; $j -le 6; $j++) {
                Write-Frame @{
                    jsonrpc = "2.0"
                    method = "session/update"
                    params = @{
                        sessionId = "fake-session"
                        update = @{
                            sessionUpdate = "tool_call"
                            toolCallId = "call_loop_$j"
                            title = "task"
                            kind = "other"
                            status = "pending"
                            rawInput = @{ description = "loop iteration $j" }
                        }
                    }
                }
            }
            exit 0
        }

        Write-Frame @{ jsonrpc = "2.0"; id = $msg.id; result = @{ stopReason = "end_turn" } }
    }
    elseif ($msg.method -eq "session/tool/result") {
        Increment-Count "session_tool_result"
    }
}
"#
    }

    #[cfg(not(windows))]
    fn fake_reasonix_shell() -> &'static str {
        r#"#!/usr/bin/env sh
set -eu

case_name="${COAGENT_FAKE_REASONIX_CASE:-success}"

increment_count() {
  name="$1"
  path="${name}_count.txt"
  value=0
  if [ -f "$path" ]; then
    value="$(cat "$path")"
  fi
  printf '%s' "$((value + 1))" > "$path"
}

read_count() {
  name="$1"
  path="${name}_count.txt"
  if [ -f "$path" ]; then
    cat "$path"
  else
    printf '0'
  fi
}

increment_count spawn

while IFS= read -r line; do
  id="$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')"
  method="$(printf '%s' "$line" | sed -n 's/.*"method":"\([^"]*\)".*/\1/p')"

  if [ "$method" = "initialize" ]; then
    increment_count initialize
    if [ "$case_name" = "initialize_error" ]; then
      printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32000,"message":"initialize rejected"}}\n' "$id"
      exit 0
    fi
    printf '{"jsonrpc":"2.0","id":%s,"result":{"ok":true}}\n' "$id"
  elif [ "$method" = "session/new" ]; then
    increment_count session_new
    if [ "$case_name" = "session_new_error" ]; then
      printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32001,"message":"session rejected"}}\n' "$id"
      exit 0
    fi
    if [ "$case_name" = "session_new_update_before_response" ]; then
      printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"fake-session","update":{"sessionUpdate":"available_commands_update","availableCommands":[]}}}\n'
    fi
    printf '{"jsonrpc":"2.0","id":%s,"result":{"sessionId":"fake-session"}}\n' "$id"
  elif [ "$method" = "session/prompt" ]; then
    increment_count prompt
    prompt_count="$(read_count prompt)"
    printf '%s' "$line" > "prompt_${prompt_count}.json"
    if [ "$case_name" = "prompt_eof" ]; then
      exit 0
    fi
    if [ "$case_name" = "prompt_eof_once" ] && [ "$prompt_count" = "1" ]; then
      exit 0
    fi
    if [ "$case_name" = "prompt_timeout_once" ] && [ "$prompt_count" = "1" ]; then
      sleep 1
      exit 0
    fi
    if [ "$case_name" = "invalid_review" ]; then
      printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"fake-session","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"not valid json"}}}}\n'
    else
      printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"fake-session","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"{\"verdict\":\"needs_fix\",\"summary\":\"Fake ACP review.\","}}}}\n'
      printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"fake-session","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"\"findings\":[],\"tests_to_run\":[\"cargo test -p coagent-mcp-server\"],"}}}}\n'
      printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"fake-session","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"\"risks\":[],\"assumptions\":[],\"confidence\":0.73}"}}}}\n'
    fi
    if [ "$case_name" = "tool_call_after_review" ]; then
      printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"fake-session","update":{"sessionUpdate":"tool_call","toolCallId":"call_fake","title":"task","kind":"other","status":"pending","rawInput":{"description":"unneeded after valid JSON"}}}}\n'
      sleep 1
      exit 0
    fi
    if [ "$case_name" = "tool_call_before_review" ]; then
      printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"fake-session","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"let me think..."}}}}\n'
	      printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"fake-session","update":{"sessionUpdate":"tool_call","toolCallId":"call_before_review","title":"task","kind":"other","status":"pending","rawInput":{"description":"need a tool before reviewing"}}}}\n'
	      sleep 0.1
	      printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"fake-session","update":{"sessionUpdate":"tool_call_update","toolCallId":"call_before_review","status":"completed","content":[{"type":"text","text":"internal tool completed"}]}}}\n'
	      printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"fake-session","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"{\"verdict\":\"needs_fix\",\"summary\":\"Fake ACP review.\","}}}}\n'
      printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"fake-session","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"\"findings\":[],\"tests_to_run\":[\"cargo test -p coagent-mcp-server\"],"}}}}\n'
      printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"fake-session","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"\"risks\":[],\"assumptions\":[],\"confidence\":0.73}"}}}}\n'
    fi
    if [ "$case_name" = "tool_call_loop_6" ]; then
      printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"fake-session","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"let me think..."}}}}\n'
      for i in 1 2 3 4 5 6; do
        printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"fake-session","update":{"sessionUpdate":"tool_call","toolCallId":"call_loop_%s","title":"task","kind":"other","status":"pending","rawInput":{"description":"loop iteration %s"}}}}\n' "$i" "$i"
      done
      exit 0
    fi
    printf '{"jsonrpc":"2.0","id":%s,"result":{"stopReason":"end_turn"}}\n' "$id"
  elif [ "$method" = "session/tool/result" ]; then
    increment_count session_tool_result
  fi
done
"#
    }
}
