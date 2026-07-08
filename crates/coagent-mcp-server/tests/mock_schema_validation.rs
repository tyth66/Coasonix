use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use coagent_runtime_core::{state::TaskStateValue, storage::RuntimeStore};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

fn temp_repo(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("coagent-mcp-{name}-{unique}"));
    fs::create_dir_all(root.join(".agent/diffs")).expect("create diffs");
    fs::write(
        root.join(".agent/diffs/current.diff"),
        "diff --git a/a b/a\n",
    )
    .expect("write diff");
    root
}

#[tokio::test]
async fn invalid_schema_input_is_rejected_before_mock_backend_success() {
    let server_exe = env!("CARGO_BIN_EXE_coagent-mcp-server");
    let repo = temp_repo("invalid-input");

    let mut child = Command::new(server_exe)
        .env("COAGENT_REPO_ROOT", repo.to_string_lossy().to_string())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("start coagent-mcp-server");

    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let mut reader = BufReader::new(stdout);

    stdin.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-06-18\",\"capabilities\":{},\"clientInfo\":{\"name\":\"test\",\"version\":\"1.0\"}}}\n").await.unwrap();
    stdin.flush().await.unwrap();

    let mut init_line = String::new();
    reader.read_line(&mut init_line).await.unwrap();
    assert!(!init_line.trim().is_empty());

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "coagent.review_diff",
            "arguments": {
                "schema_version": "review_diff_input_v1",
                "task_id": "TASK-invalid-schema",
                "request_id": "REQ-invalid-schema",
                "goal": "Review this diff.",
                "repo": { "root": repo.to_string_lossy() },
                "artifacts": { "diff_path": ".agent/diffs/current.diff" },
                "budget": { "max_steps": 0 },
                "permission_level": "L1_DIFF_REVIEW",
                "output_schema": "review_result_v1"
            }
        }
    });
    stdin
        .write_all(format!("{request}\n").as_bytes())
        .await
        .unwrap();
    stdin.flush().await.unwrap();

    let mut response_line = String::new();
    reader.read_line(&mut response_line).await.unwrap();
    drop(stdin);
    child.kill().await.ok();
    let _ = child.wait().await;

    let response: serde_json::Value = serde_json::from_str(response_line.trim()).unwrap();
    assert_eq!(response["id"], 2);
    let error_message = response["error"]["message"]
        .as_str()
        .expect("schema violation should be a JSON-RPC error");
    assert!(
        error_message.contains("schema_validation_failed"),
        "expected schema validation error, got {error_message}"
    );
    assert!(
        !response_line.contains("Mock runner completed review"),
        "invalid input must not reach mock backend success path"
    );
}

#[tokio::test]
async fn mock_success_records_metadata_schema_audit_and_completes_task() {
    let server_exe = env!("CARGO_BIN_EXE_coagent-mcp-server");
    let repo = temp_repo("mock-success");

    let mut child = Command::new(server_exe)
        .env("COAGENT_REPO_ROOT", repo.to_string_lossy().to_string())
        .env("COAGENT_BACKEND", "mock")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("start coagent-mcp-server");

    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let mut reader = BufReader::new(stdout);

    stdin.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-06-18\",\"capabilities\":{},\"clientInfo\":{\"name\":\"test\",\"version\":\"1.0\"}}}\n").await.unwrap();
    stdin.flush().await.unwrap();

    let mut init_line = String::new();
    reader.read_line(&mut init_line).await.unwrap();
    assert!(!init_line.trim().is_empty());

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "coagent.review_diff",
            "arguments": {
                "schema_version": "review_diff_input_v1",
                "task_id": "TASK-mock-success",
                "request_id": "REQ-mock-success",
                "goal": "Review this diff.",
                "repo": { "root": repo.to_string_lossy() },
                "artifacts": { "diff_path": ".agent/diffs/current.diff" },
                "permission_level": "L1_DIFF_REVIEW",
                "output_schema": "review_result_v1",
                "focus": ["correctness"],
                "constraints": ["return structured JSON"]
            }
        }
    });
    stdin
        .write_all(format!("{request}\n").as_bytes())
        .await
        .unwrap();
    stdin.flush().await.unwrap();

    let mut response_line = String::new();
    reader.read_line(&mut response_line).await.unwrap();
    drop(stdin);
    child.kill().await.ok();
    let _ = child.wait().await;

    let response: serde_json::Value = serde_json::from_str(response_line.trim()).unwrap();
    assert_eq!(response["id"], 2);
    assert_eq!(response["result"]["isError"].as_bool(), Some(false));

    let text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("tool result text");
    let review: serde_json::Value = serde_json::from_str(text).unwrap();
    assert_eq!(review["metadata"]["task_id"], "TASK-mock-success");
    assert_eq!(review["metadata"]["request_id"], "REQ-mock-success");

    let store = RuntimeStore::initialize(&repo).expect("open runtime store");
    assert_eq!(
        store
            .schema_validation_count("TASK-mock-success", "REQ-mock-success")
            .expect("schema validation count"),
        3
    );
    assert_eq!(
        store
            .schema_validation_expected_schemas("TASK-mock-success", "REQ-mock-success")
            .expect("schema validation schemas"),
        vec![
            "review_diff_input_v1",
            "pure_review_result_v1",
            "coagent_review_wrapper_v1"
        ]
    );
    assert_eq!(
        store
            .load_task_state("TASK-mock-success")
            .expect("task state")
            .value(),
        TaskStateValue::Completed
    );
}
