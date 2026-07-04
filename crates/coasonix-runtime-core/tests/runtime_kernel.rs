use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use coasonix_runtime_core::{
    kernel::{
        AuditEvent, RuntimeConfig, RuntimeDecisionValue, RuntimeKernel, SchemaValidationRequest,
        engine_results,
    },
    policy::{CommandInvocation, PermissionLevel, ResourceSet, RuntimeOperationRequest},
    state::{TaskState, TaskStateValue},
    storage::RuntimeStore,
};
use serde_json::json;

fn schema_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas/coasonix-v1.schema.json")
}

fn temp_repo(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("coasonix-kernel-{name}-{unique}"));
    fs::create_dir_all(root.join(".agent/diffs")).expect("create diffs");
    fs::create_dir_all(root.join(".agent/results")).expect("create results");
    root
}

fn config(repo_root: PathBuf) -> RuntimeConfig {
    RuntimeConfig {
        repo_root,
        schema_path: schema_path(),
        reasonix_executable: "reasonix".to_string(),
    }
}

fn allowed_request(task_id: &str, request_id: &str) -> RuntimeOperationRequest {
    RuntimeOperationRequest {
        task_id: task_id.to_string(),
        request_id: Some(request_id.to_string()),
        operation: "reasonix.review_diff".to_string(),
        permission_level: PermissionLevel::L1DiffReview,
        resources: ResourceSet {
            read_paths: vec![".agent/diffs/current.diff".to_string()],
            write_paths: vec![".agent/results/review.json".to_string()],
            network: false,
            command: Some(CommandInvocation::Argv(vec![
                "reasonix".to_string(),
                "review-diff".to_string(),
            ])),
        },
    }
}

#[test]
fn allow_decision_contains_schema_state_policy_results_and_validates() {
    let repo = temp_repo("allow");
    let mut kernel = RuntimeKernel::initialize(config(repo)).expect("initialize kernel");

    let decision = kernel.evaluate_operation(allowed_request("TASK-kernel", "REQ-kernel"));

    assert_eq!(decision.decision, RuntimeDecisionValue::Allow);
    assert_eq!(decision.engine_results.schema, RuntimeDecisionValue::Allow);
    assert_eq!(decision.engine_results.state, RuntimeDecisionValue::Allow);
    assert_eq!(decision.engine_results.policy, RuntimeDecisionValue::Allow);
    assert!(decision.reasons.is_empty());
    assert!(decision.audit_event_id.is_some());

    let validation = kernel.validate_schema(SchemaValidationRequest {
        task_id: "TASK-kernel".to_string(),
        request_id: Some("REQ-kernel".to_string()),
        expected_schema: "runtime_decision_v1".to_string(),
        payload: decision.to_payload(),
    });
    assert!(
        validation.valid,
        "runtime_decision_v1 errors: {:?}",
        validation.errors
    );
}

#[test]
fn policy_denial_beats_state_allow_and_is_persisted() {
    let repo = temp_repo("policy-deny");
    let mut kernel = RuntimeKernel::initialize(config(repo.clone())).expect("initialize kernel");
    let mut request = allowed_request("TASK-policy-deny", "REQ-policy-deny");
    request.resources.network = true;

    let decision = kernel.evaluate_operation(request);

    assert_eq!(decision.decision, RuntimeDecisionValue::Deny);
    assert_eq!(decision.engine_results.state, RuntimeDecisionValue::Allow);
    assert_eq!(decision.engine_results.policy, RuntimeDecisionValue::Deny);
    assert!(
        decision
            .reasons
            .iter()
            .any(|reason| reason.contains("network"))
    );

    let store = RuntimeStore::initialize(repo).expect("reopen store");
    assert_eq!(
        store
            .runtime_decision_count("TASK-policy-deny", RuntimeDecisionValue::Deny.into())
            .expect("count persisted deny"),
        1
    );
}

#[test]
fn unknown_operation_is_denied_by_schema_gate() {
    let repo = temp_repo("unknown-operation");
    let mut kernel = RuntimeKernel::initialize(config(repo)).expect("initialize kernel");
    let mut request = allowed_request("TASK-unknown-operation", "REQ-unknown-operation");
    request.operation = "reasonix.unknown".to_string();

    let decision = kernel.evaluate_operation(request);

    assert_eq!(decision.decision, RuntimeDecisionValue::Deny);
    assert_eq!(decision.engine_results.schema, RuntimeDecisionValue::Deny);
    assert!(
        decision
            .reasons
            .iter()
            .any(|reason| reason.contains("schema"))
    );
}

#[test]
fn state_denial_beats_policy_allow() {
    let repo = temp_repo("state-deny");
    let store = RuntimeStore::initialize(&repo).expect("initialize store");
    let terminal = TaskState::restore("TASK-state-deny", TaskStateValue::Completed, 0);
    store
        .upsert_task_state(&terminal)
        .expect("persist terminal state");

    let mut kernel = RuntimeKernel::initialize(config(repo)).expect("initialize kernel");
    let decision = kernel.evaluate_operation(allowed_request("TASK-state-deny", "REQ-state-deny"));

    assert_eq!(decision.decision, RuntimeDecisionValue::Deny);
    assert_eq!(decision.engine_results.state, RuntimeDecisionValue::Deny);
    assert_eq!(decision.engine_results.policy, RuntimeDecisionValue::Allow);
    assert!(
        decision
            .reasons
            .iter()
            .any(|reason| reason.contains("terminal"))
    );
}

#[test]
fn audit_event_id_is_attached_to_persisted_runtime_decision() {
    let repo = temp_repo("audit-id");
    let mut kernel = RuntimeKernel::initialize(config(repo.clone())).expect("initialize kernel");

    let decision = kernel.evaluate_operation(allowed_request("TASK-audit-id", "REQ-audit-id"));
    let audit_event_id = decision.audit_event_id.expect("audit id on decision");

    let store = RuntimeStore::initialize(repo).expect("reopen store");
    assert_eq!(
        store
            .runtime_decision_audit_event_id("TASK-audit-id", "REQ-audit-id")
            .expect("load persisted audit event id"),
        Some(audit_event_id)
    );
}

#[test]
fn write_audit_is_centralized_through_runtime_kernel() {
    let repo = temp_repo("write-audit");
    let mut kernel = RuntimeKernel::initialize(config(repo)).expect("initialize kernel");

    let result = kernel
        .write_audit(AuditEvent {
            task_id: "TASK-audit".to_string(),
            event_type: "manual_note".to_string(),
            summary: "kernel-owned audit write".to_string(),
            payload_json: "{}".to_string(),
        })
        .expect("write audit through kernel");

    assert_eq!(result.task_sequence, 1);
}

#[test]
fn decision_merge_precedence_matches_blueprint() {
    assert_eq!(
        RuntimeKernel::merge_decisions(engine_results(
            RuntimeDecisionValue::Allow,
            RuntimeDecisionValue::RequireApproval,
            RuntimeDecisionValue::Allow,
        )),
        RuntimeDecisionValue::RequireApproval
    );
    assert_eq!(
        RuntimeKernel::merge_decisions(engine_results(
            RuntimeDecisionValue::Allow,
            RuntimeDecisionValue::Allow,
            RuntimeDecisionValue::RetryableError,
        )),
        RuntimeDecisionValue::RetryableError
    );
    assert_eq!(
        RuntimeKernel::merge_decisions(engine_results(
            RuntimeDecisionValue::Allow,
            RuntimeDecisionValue::FatalError,
            RuntimeDecisionValue::RetryableError,
        )),
        RuntimeDecisionValue::FatalError
    );
    assert_eq!(
        RuntimeKernel::merge_decisions(engine_results(
            RuntimeDecisionValue::FatalError,
            RuntimeDecisionValue::Allow,
            RuntimeDecisionValue::Deny,
        )),
        RuntimeDecisionValue::Deny
    );
}

#[test]
fn validate_schema_routes_through_kernel_registry() {
    let repo = temp_repo("schema");
    let kernel = RuntimeKernel::initialize(config(repo.clone())).expect("initialize kernel");

    let validation = kernel.validate_schema(SchemaValidationRequest {
        task_id: "TASK-schema".to_string(),
        request_id: Some("REQ-schema".to_string()),
        expected_schema: "review_result_v1".to_string(),
        payload: json!({
            "schema_version": "review_result_v1",
            "task_id": "TASK-schema",
            "request_id": "REQ-schema",
            "status": "ok",
            "verdict": "pass",
            "summary": "No findings.",
            "confidence": 0.9
        }),
    });

    assert!(validation.valid, "schema errors: {:?}", validation.errors);

    let store = RuntimeStore::initialize(repo).expect("reopen store");
    assert_eq!(
        store
            .schema_validation_count("TASK-schema", "REQ-schema")
            .expect("schema validation evidence count"),
        1
    );
}
