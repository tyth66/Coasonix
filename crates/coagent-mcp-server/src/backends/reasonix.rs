use std::path::PathBuf;
use std::process::Stdio;

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
            .map_err(|e| ReasonixError::Spawn(e.to_string()))?;

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
        let init_resp = read_line(&mut reader).await?;
        parse_response_frame(&init_resp, 1, "initialize")?;

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

        let session_resp = read_line(&mut reader).await?;
        let session = parse_response_frame(&session_resp, 2, "session/new")?;
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
        loop {
            let line = tokio::time::timeout_at(deadline, read_line(&mut self.reader))
                .await
                .map_err(|_| ReasonixError::Timeout("ACP prompt timed out".into()))??;
            if line.is_empty() {
                continue;
            }
            let msg: serde_json::Value = serde_json::from_str(&line)
                .map_err(|e| ReasonixError::Protocol(format!("invalid frame: {e}")))?;

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
                && update.get("sessionUpdate").and_then(|v| v.as_str())
                    == Some("agent_message_chunk")
                && let Some(text) = update
                    .get("content")
                    .and_then(|c| c.get("text"))
                    .and_then(|v| v.as_str())
            {
                collected_text.push_str(text);
            }
        }

        let review: PureReviewResult = serde_json::from_str(&collected_text)
            .or_else(|_| extract_json(&collected_text))
            .map_err(|e| ReasonixError::Protocol(format!("parse review: {e}")))?;
        Ok(review)
    }
}

// ── Reasonix Runner ──

use std::sync::Arc;

/// Reasonix-specific ACP runner. Holds a shared, lazily-initialized Reasonix
/// session; it does not yet honor arbitrary AgentProfile command/args.
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
        context: &crate::backends::context::ContextProjection,
    ) -> Result<PureReviewResult, ReasonixError> {
        let mut guard = self.session.lock().await;

        if guard.is_none() {
            let session = AcpSession::connect(&self.model, &self.cwd).await?;
            *guard = Some(session);
        }

        let first = {
            let session = guard.as_mut().unwrap();
            session.send_prompt(goal, diff_path, context).await
        };

        match first {
            Ok(result) => Ok(result),
            Err(error) if error.is_recoverable() => {
                eprintln!(
                    "[coagent] Reasonix session failed ({}), reconnecting and retrying",
                    error
                );
                *guard = None;
                let mut session =
                    AcpSession::connect(&self.model, &self.cwd)
                        .await
                        .map_err(|connect_err| {
                            ReasonixError::Protocol(format!(
                                "session recovery failed after {}: {}",
                                error, connect_err
                            ))
                        })?;
                let retry = session.send_prompt(goal, diff_path, context).await;
                *guard = Some(session);
                retry
            }
            Err(error) => Err(error),
        }
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
    /// Protocol and Io errors are recoverable via session restart.
    fn is_recoverable(&self) -> bool {
        matches!(self, Self::Io(_) | Self::Protocol(_))
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

fn parse_response_frame(
    line: &str,
    expected_id: u64,
    context: &str,
) -> Result<serde_json::Value, ReasonixError> {
    let frame: serde_json::Value = serde_json::from_str(line)
        .map_err(|e| ReasonixError::Protocol(format!("{context}: invalid frame: {e}")))?;
    if frame.get("id").and_then(|v| v.as_u64()) != Some(expected_id) {
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
    Ok(frame)
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

function Write-Frame($frame) {
    [Console]::Out.WriteLine(($frame | ConvertTo-Json -Compress -Depth 20))
    [Console]::Out.Flush()
}

while ($null -ne ($line = [Console]::In.ReadLine())) {
    $msg = $line | ConvertFrom-Json

    if ($msg.method -eq "initialize") {
        if ($case -eq "initialize_error") {
            Write-Frame @{ jsonrpc = "2.0"; id = $msg.id; error = @{ code = -32000; message = "initialize rejected" } }
            exit 0
        }
        Write-Frame @{ jsonrpc = "2.0"; id = $msg.id; result = @{ ok = $true } }
    }
    elseif ($msg.method -eq "session/new") {
        if ($case -eq "session_new_error") {
            Write-Frame @{ jsonrpc = "2.0"; id = $msg.id; error = @{ code = -32001; message = "session rejected" } }
            exit 0
        }
        Write-Frame @{ jsonrpc = "2.0"; id = $msg.id; result = @{ sessionId = "fake-session" } }
    }
    elseif ($msg.method -eq "session/prompt") {
        if ($case -eq "prompt_eof") {
            exit 0
        }

        if ($case -eq "invalid_review") {
            $chunks = @("not valid json")
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
        Write-Frame @{ jsonrpc = "2.0"; id = $msg.id; result = @{ stopReason = "end_turn" } }
    }
}
"#
    }

    #[cfg(not(windows))]
    fn fake_reasonix_shell() -> &'static str {
        r#"#!/usr/bin/env sh
set -eu

case_name="${COAGENT_FAKE_REASONIX_CASE:-success}"

while IFS= read -r line; do
  id="$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')"
  method="$(printf '%s' "$line" | sed -n 's/.*"method":"\([^"]*\)".*/\1/p')"

  if [ "$method" = "initialize" ]; then
    if [ "$case_name" = "initialize_error" ]; then
      printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32000,"message":"initialize rejected"}}\n' "$id"
      exit 0
    fi
    printf '{"jsonrpc":"2.0","id":%s,"result":{"ok":true}}\n' "$id"
  elif [ "$method" = "session/new" ]; then
    if [ "$case_name" = "session_new_error" ]; then
      printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32001,"message":"session rejected"}}\n' "$id"
      exit 0
    fi
    printf '{"jsonrpc":"2.0","id":%s,"result":{"sessionId":"fake-session"}}\n' "$id"
  elif [ "$method" = "session/prompt" ]; then
    if [ "$case_name" = "prompt_eof" ]; then
      exit 0
    fi
    if [ "$case_name" = "invalid_review" ]; then
      printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"fake-session","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"not valid json"}}}}\n'
    else
      printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"fake-session","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"{\"verdict\":\"needs_fix\",\"summary\":\"Fake ACP review.\","}}}}\n'
      printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"fake-session","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"\"findings\":[],\"tests_to_run\":[\"cargo test -p coagent-mcp-server\"],"}}}}\n'
      printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"fake-session","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"\"risks\":[],\"assumptions\":[],\"confidence\":0.73}"}}}}\n'
    fi
    printf '{"jsonrpc":"2.0","id":%s,"result":{"stopReason":"end_turn"}}\n' "$id"
  fi
done
"#
    }
}
