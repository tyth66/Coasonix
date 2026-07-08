use std::path::PathBuf;

use coagent_runtime_core::schema::{SchemaRegistry, parse_json_no_duplicate_keys};
use serde_json::json;

fn schema_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas/coagent-v1.schema.json")
}

fn valid_review_result() -> serde_json::Value {
    json!({
        "review": {
            "verdict": "pass",
            "summary": "No findings.",
            "findings": [],
            "tests_to_run": [],
            "risks": [],
            "assumptions": [],
            "confidence": 0.91
        },
        "metadata": {
            "schema_version": "review_result_v1",
            "task_id": "TASK-schema",
            "request_id": "REQ-schema",
            "status": "ok",
            "operation": "reasonix.review_diff",
            "runtime_decision": "allow"
        }
    })
}
fn valid_review_diff_input() -> serde_json::Value {
    json!({
        "schema_version": "review_diff_input_v1",
        "task_id": "TASK-schema",
        "request_id": "REQ-schema",
        "mode": "review_diff",
        "goal": "Review the current diff.",
        "repo": {
            "root": ".",
            "base_branch": "main",
            "working_branch": "codex/v1"
        },
        "artifacts": {
            "context_path": ".agent/context/review.json",
            "diff_path": ".agent/diffs/current.diff"
        },
        "permission_level": "L1_DIFF_REVIEW",
        "output_schema": "review_result_v1"
    })
}

fn valid_runtime_status() -> serde_json::Value {
    json!({
        "backend": "reasonix",
        "repo_root": "D:/repo",
        "reasonix": {
            "has_session": true,
            "session_created_count": 1,
            "prompt_count": 2,
            "reconnect_count": 0,
            "timeout_count": 0,
            "protocol_error_count": 0,
            "io_error_count": 0,
            "spawn_error_count": 0,
            "tool_call_count": 0,
            "denied_tool_call_count": 0,
            "last_error": null
        }
    })
}

#[test]
fn validates_review_result_v1_from_root_schema_registry() {
    let registry = SchemaRegistry::load_from_path(schema_path()).expect("schema registry loads");

    let result = registry.validate("coagent_review_wrapper_v1", &valid_review_result());

    assert!(
        result.valid,
        "expected valid payload, got {:?}",
        result.errors
    );
}

#[test]
fn validates_review_diff_input_v1_from_root_schema_registry() {
    let registry = SchemaRegistry::load_from_path(schema_path()).expect("schema registry loads");

    let result = registry.validate("review_diff_input_v1", &valid_review_diff_input());

    assert!(
        result.valid,
        "expected valid payload, got {:?}",
        result.errors
    );
}

#[test]
fn validates_runtime_status_v1_from_root_schema_registry() {
    let registry = SchemaRegistry::load_from_path(schema_path()).expect("schema registry loads");

    let result = registry.validate("runtime_status_v1", &valid_runtime_status());

    assert!(
        result.valid,
        "expected valid payload, got {:?}",
        result.errors
    );
}

#[test]
fn validates_runtime_status_v1_without_reasonix_stats() {
    let registry = SchemaRegistry::load_from_path(schema_path()).expect("schema registry loads");
    let payload = json!({
        "backend": "mock",
        "repo_root": "D:/repo",
        "reasonix": null
    });

    let result = registry.validate("runtime_status_v1", &payload);

    assert!(
        result.valid,
        "expected valid payload, got {:?}",
        result.errors
    );
}

#[test]
fn unknown_expected_schema_fails_closed() {
    let registry = SchemaRegistry::load_from_path(schema_path()).expect("schema registry loads");

    let result = registry.validate("unknown_schema_v1", &valid_review_result());

    assert!(!result.valid);
    assert!(
        result
            .errors
            .iter()
            .any(|error| error.message.contains("missing $defs/unknown_schema_v1")),
        "expected missing definition error, got {:?}",
        result.errors
    );
}

#[test]
fn rejects_schema_version_that_does_not_match_expected_schema() {
    let registry = SchemaRegistry::load_from_path(schema_path()).expect("schema registry loads");
    let mut payload = valid_review_result();
    payload["metadata"]["schema_version"] = json!("security_audit_v1");

    let result = registry.validate("coagent_review_wrapper_v1", &payload);

    assert!(!result.valid);
    assert!(
        result
            .errors
            .iter()
            .any(|error| error.path.contains("schema_version")),
        "expected schema_version error, got {:?}",
        result.errors
    );
}

#[test]
fn rejects_payload_valid_for_different_schema() {
    let registry = SchemaRegistry::load_from_path(schema_path()).expect("schema registry loads");

    let result = registry.validate("coagent_review_wrapper_v1", &valid_review_diff_input());

    assert!(!result.valid);
    assert!(
        result
            .errors
            .iter()
            .any(|error| error.message.contains("review")),
        "expected required field error, got {:?}",
        result.errors
    );
}

#[test]
fn rejects_review_diff_input_output_schema_mismatch() {
    let registry = SchemaRegistry::load_from_path(schema_path()).expect("schema registry loads");
    let mut payload = valid_review_diff_input();
    payload["output_schema"] = json!("security_audit_v1");

    let result = registry.validate("review_diff_input_v1", &payload);

    assert!(!result.valid);
    assert!(
        result
            .errors
            .iter()
            .any(|error| error.path.contains("output_schema")),
        "expected output_schema error, got {:?}",
        result.errors
    );
}

#[test]
fn rejects_confidence_outside_allowed_range() {
    let registry = SchemaRegistry::load_from_path(schema_path()).expect("schema registry loads");
    let mut payload = valid_review_result();
    payload["review"]["confidence"] = json!(1.1);

    let result = registry.validate("coagent_review_wrapper_v1", &payload);

    assert!(!result.valid);
    assert!(
        result
            .errors
            .iter()
            .any(|error| error.path.contains("confidence")),
        "expected confidence error, got {:?}",
        result.errors
    );
}

#[test]
fn rejects_unexpected_top_level_field_when_schema_disallows_it() {
    let registry = SchemaRegistry::load_from_path(schema_path()).expect("schema registry loads");
    let mut payload = valid_review_result();
    payload["unexpected"] = json!("not allowed");

    let result = registry.validate("coagent_review_wrapper_v1", &payload);

    assert!(!result.valid);
    assert!(
        result
            .errors
            .iter()
            .any(|error| error.message.contains("unexpected")),
        "expected additionalProperties error, got {:?}",
        result.errors
    );
}

#[test]
fn rejects_duplicate_json_keys_before_schema_validation() {
    let duplicate = r#"{
        "schema_version": "review_result_v1",
        "schema_version": "security_audit_v1",
        "task_id": "TASK-schema",
        "request_id": "REQ-schema",
        "status": "ok",
        "verdict": "pass",
        "summary": "No findings.",
        "confidence": 0.91
    }"#;

    let error = parse_json_no_duplicate_keys(duplicate).expect_err("duplicate key should fail");

    assert!(error.to_string().contains("duplicate key"));
}

#[test]
fn malformed_json_returns_schema_error_without_panic() {
    let error = parse_json_no_duplicate_keys(r#"{ "schema_version": "#)
        .expect_err("malformed JSON should fail");

    assert!(error.to_string().contains("invalid JSON"));
}
