use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{Value, json};

struct WorkerProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl WorkerProcess {
    fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_coasonix-runtime-worker"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn worker");
        let stdin = child.stdin.take().expect("worker stdin");
        let stdout = BufReader::new(child.stdout.take().expect("worker stdout"));
        Self {
            child,
            stdin,
            stdout,
        }
    }

    fn request(&mut self, frame: Value) -> Value {
        self.send_raw(&format!("{frame}\n"));
        self.read_frame()
    }

    fn send_raw(&mut self, frame: &str) {
        self.stdin
            .write_all(frame.as_bytes())
            .expect("write worker frame");
        self.stdin.flush().expect("flush worker frame");
    }

    fn read_frame(&mut self) -> Value {
        let mut line = String::new();
        self.stdout
            .read_line(&mut line)
            .expect("read worker response");
        assert!(!line.is_empty(), "worker produced no stdout frame");
        serde_json::from_str(&line).expect("stdout frame is JSON")
    }

    fn shutdown(mut self) -> (Value, String) {
        let response = self.request(json!({
            "jsonrpc": "2.0",
            "id": "REQ-shutdown",
            "method": "runtime.shutdown",
            "params": {}
        }));
        drop(self.stdin);
        let output = self.child.wait_with_output().expect("wait worker");
        assert!(
            output.status.success(),
            "worker exit status: {}",
            output.status
        );
        (
            response,
            String::from_utf8(output.stderr).expect("stderr utf8"),
        )
    }
}

fn temp_repo(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("coasonix-worker-{name}-{unique}"));
    fs::create_dir_all(root.join(".agent/diffs")).expect("create diffs");
    fs::create_dir_all(root.join(".agent/results")).expect("create results");
    root
}

fn initialize(worker: &mut WorkerProcess, repo: &PathBuf) -> Value {
    worker.request(json!({
        "jsonrpc": "2.0",
        "id": "REQ-init",
        "method": "runtime.initialize",
        "params": {
            "repo_root": repo,
            "reasonix_executable": "reasonix"
        }
    }))
}

#[test]
fn valid_initialize_succeeds_after_migrations() {
    let repo = temp_repo("initialize");
    let mut worker = WorkerProcess::spawn();

    let response = initialize(&mut worker, &repo);

    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], "REQ-init");
    assert_eq!(response["result"]["initialized"], true);
    assert!(repo.join(".agent/coasonix.sqlite").exists());

    let (_shutdown, stderr) = worker.shutdown();
    assert!(
        stderr.is_empty(),
        "stderr should stay empty unless logging: {stderr}"
    );
}

#[test]
fn unknown_method_is_rejected() {
    let mut worker = WorkerProcess::spawn();

    let response = worker.request(json!({
        "jsonrpc": "2.0",
        "id": "REQ-unknown",
        "method": "runtime.post_v1",
        "params": {}
    }));

    assert_eq!(response["error"]["code"], -32601);
    assert_eq!(response["id"], "REQ-unknown");
    let _ = worker.shutdown();
}

#[test]
fn notification_is_rejected() {
    let mut worker = WorkerProcess::spawn();

    let response = worker.request(json!({
        "jsonrpc": "2.0",
        "method": "runtime.shutdown",
        "params": {}
    }));

    assert_eq!(response["error"]["code"], -32600);
    assert!(response["id"].is_null());
    let _ = worker.shutdown();
}

#[test]
fn malformed_json_is_rejected_with_parse_error() {
    let mut worker = WorkerProcess::spawn();

    worker.send_raw("{not-json}\n");
    let response = worker.read_frame();

    assert_eq!(response["error"]["code"], -32700);
    assert!(response["id"].is_null());
    let _ = worker.shutdown();
}

#[test]
fn invalid_params_are_rejected() {
    let mut worker = WorkerProcess::spawn();

    let response = worker.request(json!({
        "jsonrpc": "2.0",
        "id": "REQ-invalid",
        "method": "runtime.initialize",
        "params": {
            "repo_root": 42,
            "reasonix_executable": "reasonix"
        }
    }));

    assert_eq!(response["error"]["code"], -32602);
    assert_eq!(response["id"], "REQ-invalid");
    let _ = worker.shutdown();
}

#[test]
fn evaluate_operation_returns_runtime_decision_v1() {
    let repo = temp_repo("evaluate");
    let mut worker = WorkerProcess::spawn();
    assert!(initialize(&mut worker, &repo).get("error").is_none());

    let response = worker.request(json!({
        "jsonrpc": "2.0",
        "id": "REQ-evaluate",
        "method": "runtime.evaluate_operation",
        "params": {
            "task_id": "TASK-worker-evaluate",
            "operation": "reasonix.review_diff",
            "permission_level": "L1_DIFF_REVIEW",
            "resources": {
                "read_paths": [".agent/diffs/current.diff"],
                "write_paths": [".agent/results/review.json"],
                "network": false,
                "command": ["reasonix", "review-diff"]
            }
        }
    }));

    assert_eq!(response["id"], "REQ-evaluate");
    assert_eq!(response["result"]["schema_version"], "runtime_decision_v1");
    assert_eq!(response["result"]["request_id"], "REQ-evaluate");
    assert_eq!(response["result"]["decision"], "allow");
    assert_eq!(response["result"]["engine_results"]["policy"], "allow");

    let _ = worker.shutdown();
}

#[test]
fn policy_denial_still_returns_runtime_decision_result() {
    let repo = temp_repo("policy-denial");
    let mut worker = WorkerProcess::spawn();
    assert!(initialize(&mut worker, &repo).get("error").is_none());

    let response = worker.request(json!({
        "jsonrpc": "2.0",
        "id": "REQ-policy-deny",
        "method": "runtime.evaluate_operation",
        "params": {
            "task_id": "TASK-worker-policy-deny",
            "operation": "reasonix.review_diff",
            "permission_level": "L1_DIFF_REVIEW",
            "resources": {
                "read_paths": [".agent/diffs/current.diff"],
                "write_paths": [".agent/results/review.json"],
                "network": true,
                "command": ["reasonix", "review-diff"]
            }
        }
    }));

    assert!(response.get("error").is_none());
    assert_eq!(response["result"]["schema_version"], "runtime_decision_v1");
    assert_eq!(response["result"]["decision"], "deny");
    assert_eq!(response["result"]["engine_results"]["policy"], "deny");

    let _ = worker.shutdown();
}

#[test]
fn write_audit_returns_audit_record_after_initialize() {
    let repo = temp_repo("write-audit");
    let mut worker = WorkerProcess::spawn();
    assert!(initialize(&mut worker, &repo).get("error").is_none());

    let response = worker.request(json!({
        "jsonrpc": "2.0",
        "id": "REQ-write-audit",
        "method": "runtime.write_audit",
        "params": {
            "task_id": "TASK-worker-audit",
            "event_type": "manual_note",
            "summary": "worker-owned audit write",
            "payload_json": "{}"
        }
    }));

    assert_eq!(response["id"], "REQ-write-audit");
    assert_eq!(response["result"]["id"], 1);
    assert_eq!(response["result"]["task_sequence"], 1);

    let _ = worker.shutdown();
}

#[test]
fn stdout_contains_json_rpc_frames_only() {
    let mut worker = WorkerProcess::spawn();

    worker.send_raw("{bad-json}\n");
    let response = worker.read_frame();

    assert_eq!(response["jsonrpc"], "2.0");
    assert!(response.get("result").is_some() || response.get("error").is_some());
    let _ = worker.shutdown();
}

#[test]
fn worker_shutdown_is_explicit() {
    let mut worker = WorkerProcess::spawn();

    let response = worker.request(json!({
        "jsonrpc": "2.0",
        "id": "REQ-shutdown",
        "method": "runtime.shutdown",
        "params": {}
    }));
    drop(worker.stdin);
    let status = worker.child.wait().expect("wait worker");

    assert_eq!(response["result"]["shutdown"], true);
    assert!(status.success(), "worker should exit successfully");
}
