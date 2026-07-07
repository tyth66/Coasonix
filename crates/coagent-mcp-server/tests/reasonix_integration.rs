// Integration test: full review_diff round-trip with Reasonix backend.
// Run: cargo test -p coagent-mcp-server --test reasonix_integration -- --ignored --nocapture

use std::time::Instant;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

#[tokio::test]
#[ignore = "requires Reasonix CLI and DeepSeek API key"]
async fn reasonix_real_review_diff() {
    // Load API key from Reasonix .env
    if std::env::var("DEEPSEEK_API_KEY").is_err() {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let env_file = std::path::PathBuf::from(&appdata)
                .join("reasonix")
                .join(".env");
            if let Ok(content) = std::fs::read_to_string(&env_file) {
                for line in content.lines() {
                    if let Some((k, v)) = line.trim().split_once('=') {
                        if k == "DEEPSEEK_API_KEY" && !v.is_empty() {
                            unsafe {
                                std::env::set_var("DEEPSEEK_API_KEY", v);
                            }
                        }
                    }
                }
            }
        }
    }

    let server_exe = env!("CARGO_BIN_EXE_coagent-mcp-server");
    let reasonix_path = std::env::var("COAGENT_REASONIX_PATH").unwrap_or_else(|_| {
        let appdata = std::env::var("APPDATA").unwrap_or_default();
        let candidate = std::path::PathBuf::from(&appdata)
            .join("npm")
            .join("reasonix.cmd");
        if candidate.exists() {
            candidate.to_string_lossy().to_string()
        } else {
            "reasonix".into()
        }
    });

    let test_repo = std::env::temp_dir().join("coagent-integration-repo");
    let _ = std::fs::create_dir_all(test_repo.join(".agent").join("diffs"));
    std::fs::write(
        test_repo.join(".agent").join("diffs").join("test.diff"),
        r#"diff --git a/src/main.rs b/src/main.rs
--- a/src/main.rs
+++ b/src/main.rs
@@ -10,6 +10,10 @@
     let config = Config::from_env()?;
+    if config.repo_root.to_string_lossy().is_empty() {
+        eprintln!("Warning: empty repo root");
+    }
@@ -30,3 +34,10 @@
+    // WARNING: replacing ? with .expect() could cause panic
     let kernel = RuntimeKernel::initialize(RuntimeConfig {
         repo_root: config.repo_root.clone(),
-    })?;
+    }).expect("kernel init failed");
"#,
    )
    .unwrap();

    let task_id = format!("TASK-int-{}", std::process::id());

    let mut child = Command::new(server_exe)
        .env("COAGENT_REPO_ROOT", test_repo.to_string_lossy().to_string())
        .env("COAGENT_BACKEND", "reasonix")
        .env("COAGENT_REASONIX_MODEL", "deepseek-v4-flash")
        .env("COAGENT_REASONIX_PATH", &reasonix_path)
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("COAGENT_AGENT_TIMEOUT_MS", "180000")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("Failed to start coagent-mcp-server");

    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let _stderr = child.stderr.take();
    let mut reader = BufReader::new(stdout);

    // Send initialize
    stdin.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-06-18\",\"capabilities\":{},\"clientInfo\":{\"name\":\"test\",\"version\":\"1.0\"}}}\n").await.unwrap();
    stdin.flush().await.unwrap();

    let mut init_line = String::new();
    reader.read_line(&mut init_line).await.unwrap();
    println!("Init: {}", init_line);

    // Send review_diff (keep stdin open, don't close it)
    let diff_path = test_repo.join(".agent").join("diffs").join("test.diff");
    let request = serde_json::json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {
            "name": "reasonix.review_diff",
            "arguments": {
                "schema_version": "review_diff_input_v1",
                "task_id": task_id,
                "request_id": format!("REQ-{}", task_id),
                "goal": "Review this diff. The key change replaces ? with .expect() which could cause a panic.",
                "repo": { "root": test_repo.to_string_lossy() },
                "artifacts": { "diff_path": diff_path.to_string_lossy().to_string() },
                "permission_level": "L1_DIFF_REVIEW",
                "output_schema": "review_result_v1",
                "focus": ["correctness", "error handling"]
            }
        }
    });
    stdin
        .write_all(format!("{}\n", request).as_bytes())
        .await
        .unwrap();
    stdin.flush().await.unwrap();

    let start = Instant::now();
    let mut review_response = String::new();

    // Read until we get the tools/call response (id=2)
    let deadline = Instant::now() + std::time::Duration::from_secs(180);
    let mut line_buf = String::new();
    while Instant::now() < deadline {
        line_buf.clear();
        let read_result = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            reader.read_line(&mut line_buf),
        )
        .await;
        match read_result {
            Ok(Ok(n)) if n > 0 => {
                let trimmed = line_buf.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let elapsed = start.elapsed().as_secs();
                println!(
                    "[{}s] {}",
                    elapsed,
                    if trimmed.len() > 250 {
                        &trimmed[..250]
                    } else {
                        trimmed
                    }
                );

                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(trimmed) {
                    if parsed.get("id").and_then(|v| v.as_i64()) == Some(2) {
                        review_response = trimmed.to_string();
                        break;
                    }
                }
            }
            Ok(Ok(0)) => break, // EOF
            _ => continue,      // timeout or error, retry
        }
    }

    let elapsed = start.elapsed();

    // Now close stdin to signal shutdown
    drop(stdin);
    child.kill().await.ok();
    let _ = child.wait().await;

    let stderr_text = String::new();
    // stderr is read on best-effort basis
    if !stderr_text.is_empty() {
        eprintln!("=== STDERR ===\n{}", stderr_text);
    }

    assert!(
        !review_response.is_empty(),
        "No review response after {:.1}s",
        elapsed.as_secs_f64()
    );

    let result: serde_json::Value = serde_json::from_str(&review_response).unwrap();
    let is_error = result["result"]["isError"].as_bool().unwrap_or(true);
    let text = result["result"]["content"][0]["text"].as_str().unwrap();

    assert!(!is_error, "Review failed: {}", text);

    let review: serde_json::Value = serde_json::from_str(text).unwrap();

    println!("\n=== REVIEW RESULT ===");
    println!("Verdict: {}", review["review"]["verdict"]);
    println!("Summary: {}", review["review"]["summary"]);
    println!("Confidence: {}", review["review"]["confidence"]);
    if let Some(findings) = review["review"]["findings"].as_array() {
        println!("Findings: {} issues", findings.len());
        for (i, f) in findings.iter().enumerate() {
            println!(
                "  {}. [{}] {}",
                i + 1,
                f.get("severity").and_then(|v| v.as_str()).unwrap_or("?"),
                f.get("issue").and_then(|v| v.as_str()).unwrap_or("?")
            );
        }
    }

    assert_eq!(review["metadata"]["task_id"], task_id);
    assert_eq!(review["metadata"]["status"], "ok");
    assert!(review["review"]["confidence"].as_f64().unwrap() > 0.0);
    assert!(
        review["review"]["findings"].as_array().unwrap().len() > 0,
        "Expected at least one finding"
    );

    println!("\nPASSED in {:.1}s", elapsed.as_secs_f64());
}
