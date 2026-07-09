use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use coagent_runtime_core::{
    kernel::{AuditEvent, RuntimeConfig, RuntimeDecisionValue, RuntimeKernel, engine_results},
    policy::{
        ApprovalPolicy, BackendBinding, PermissionLevel, ResourceSet, RuntimeOperationRequest,
        ToolCapabilities, ToolDefinition, ToolRegistry,
    },
    state::{TaskState, TaskStateValue},
    storage::RuntimeStore,
};

fn temp_repo(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("coagent-kernel-{name}-{unique}"));
    fs::create_dir_all(root.join(".agent/diffs")).expect("create diffs");
    fs::create_dir_all(root.join(".agent/results")).expect("create results");
    root
}

fn config(repo_root: PathBuf) -> RuntimeConfig {
    RuntimeConfig { repo_root }
}

fn allowed_request(task_id: &str, request_id: &str) -> RuntimeOperationRequest {
    RuntimeOperationRequest {
        task_id: task_id.to_string(),
        request_id: Some(request_id.to_string()),
        operation: "coagent.review_diff".to_string(),
        permission_level: PermissionLevel::L1DiffReview,
        resources: ResourceSet {
            read_paths: vec![".agent/diffs/current.diff".to_string()],
            write_paths: vec![".agent/results/review.json".to_string()],
            network: false,
        },
    }
}

#[test]
fn kernel_initializes_and_allows_review_diff() {
    let repo = temp_repo("allow");
    let mut kernel = RuntimeKernel::initialize(config(repo)).expect("initialize kernel");

    let decision = kernel.evaluate_operation(allowed_request("TASK-kernel", "REQ-kernel"));

    assert_eq!(decision.decision, RuntimeDecisionValue::Allow);
    assert_eq!(decision.engine_results.state, RuntimeDecisionValue::Allow);
    assert_eq!(decision.engine_results.policy, RuntimeDecisionValue::Allow);
    assert!(decision.reasons.is_empty());
    assert!(decision.audit_event_id.is_some());
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
            .runtime_decision_count("TASK-policy-deny", RuntimeDecisionValue::Deny)
            .expect("count persisted deny"),
        1
    );
}

#[test]
fn unknown_operation_is_denied_by_policy_gate() {
    let repo = temp_repo("unknown-operation");
    let mut kernel = RuntimeKernel::initialize(config(repo)).expect("initialize kernel");
    let mut request = allowed_request("TASK-unknown-operation", "REQ-unknown-operation");
    request.operation = "agent.unknown".to_string();

    let decision = kernel.evaluate_operation(request);

    assert_eq!(decision.decision, RuntimeDecisionValue::Deny);
    assert_eq!(decision.engine_results.policy, RuntimeDecisionValue::Deny);
    assert!(
        decision
            .reasons
            .iter()
            .any(|reason| reason.contains("unknown operation"))
    );
}

#[test]
fn kernel_can_initialize_with_custom_tool_registry() {
    let repo = temp_repo("custom-registry");
    fs::create_dir_all(repo.join("docs")).expect("create docs");
    let registry = ToolRegistry::new().register(ToolDefinition::new(
        "agent.docs_read",
        PermissionLevel::L0Readonly,
        BackendBinding::Mock,
        ApprovalPolicy::Never,
        "docs_read_input_v1",
        "docs_read_result_v1",
        ToolCapabilities {
            read_allow: vec!["docs/**".to_string()],
            write_allow: vec![],
            deny: vec![".git/**".to_string()],
            network: false,
        },
    ));
    let mut kernel = RuntimeKernel::initialize_with_tool_registry(config(repo), registry)
        .expect("initialize kernel");

    let docs_decision = kernel.evaluate_operation(RuntimeOperationRequest {
        task_id: "TASK-docs".to_string(),
        request_id: Some("REQ-docs".to_string()),
        operation: "agent.docs_read".to_string(),
        permission_level: PermissionLevel::L0Readonly,
        resources: ResourceSet {
            read_paths: vec!["docs/README.md".to_string()],
            write_paths: vec![],
            network: false,
        },
    });
    assert_eq!(docs_decision.decision, RuntimeDecisionValue::Allow);

    let review_decision =
        kernel.evaluate_operation(allowed_request("TASK-review-unknown", "REQ-review-unknown"));
    assert_eq!(review_decision.decision, RuntimeDecisionValue::Deny);
    assert!(
        review_decision
            .reasons
            .iter()
            .any(|reason| reason.contains("unknown operation"))
    );
}

#[test]
fn kernel_emits_runtime_events_for_evaluate_and_complete() {
    let repo = temp_repo("events");
    let mut kernel = RuntimeKernel::initialize(config(repo.clone())).expect("initialize kernel");

    let decision = kernel.evaluate_operation(allowed_request("TASK-events", "REQ-events"));
    assert_eq!(decision.decision, RuntimeDecisionValue::Allow);

    kernel
        .complete_operation("TASK-events", Some("REQ-events"), "coagent.review_diff")
        .expect("complete operation");

    let store = RuntimeStore::initialize(repo).expect("reopen store");
    let events = store
        .runtime_events("TASK-events")
        .expect("load runtime events");
    let event_types: Vec<_> = events
        .iter()
        .map(|event| event.event_type.as_str())
        .collect();

    assert_eq!(
        event_types,
        vec!["step_started", "policy_evaluated", "lifecycle_closed"]
    );
    assert!(events.iter().all(|event| event.step_id.is_some()));
    assert!(
        events
            .iter()
            .any(|event| event.payload_json.contains("\"decision\":\"allow\""))
    );

    let step = store
        .runtime_step(events[0].step_id.expect("step id"))
        .expect("load runtime step");
    assert_eq!(step.state, "completed");
}

#[test]
fn kernel_closes_runtime_step_when_policy_denies() {
    let repo = temp_repo("events-deny");
    let mut kernel = RuntimeKernel::initialize(config(repo.clone())).expect("initialize kernel");
    let mut request = allowed_request("TASK-denied-events", "REQ-denied-events");
    request.resources.network = true;

    let decision = kernel.evaluate_operation(request);
    assert_eq!(decision.decision, RuntimeDecisionValue::Deny);

    let store = RuntimeStore::initialize(repo).expect("reopen store");
    let events = store
        .runtime_events("TASK-denied-events")
        .expect("load runtime events");
    let event_types: Vec<_> = events
        .iter()
        .map(|event| event.event_type.as_str())
        .collect();
    assert_eq!(
        event_types,
        vec!["step_started", "policy_evaluated", "lifecycle_closed"]
    );

    let step = store
        .runtime_step(events[0].step_id.expect("step id"))
        .expect("load runtime step");
    assert_eq!(step.state, "deny");
}

#[test]
fn state_denial_beats_policy_allow() {
    let repo = temp_repo("state-deny");
    let store = RuntimeStore::initialize(&repo).expect("initialize store");
    let terminal = TaskState::restore("TASK-state-deny", TaskStateValue::Cancelled, 0);
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
            .any(|reason| reason.contains("cancelled"))
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
            RuntimeDecisionValue::RequireApproval,
            RuntimeDecisionValue::Allow,
        )),
        RuntimeDecisionValue::RequireApproval
    );
    assert_eq!(
        RuntimeKernel::merge_decisions(engine_results(
            RuntimeDecisionValue::Allow,
            RuntimeDecisionValue::RetryableError,
        )),
        RuntimeDecisionValue::RetryableError
    );
    assert_eq!(
        RuntimeKernel::merge_decisions(engine_results(
            RuntimeDecisionValue::FatalError,
            RuntimeDecisionValue::RetryableError,
        )),
        RuntimeDecisionValue::FatalError
    );
    assert_eq!(
        RuntimeKernel::merge_decisions(engine_results(
            RuntimeDecisionValue::Allow,
            RuntimeDecisionValue::Deny,
        )),
        RuntimeDecisionValue::Deny
    );
}

#[test]
fn cancel_task_transitions_running_to_cancelled() {
    let root = temp_repo("cancel_task");
    let mut kernel = RuntimeKernel::initialize(config(root.clone())).expect("init");

    let request = RuntimeOperationRequest {
        task_id: "TASK-cancel-test".into(),
        request_id: Some("REQ-1".into()),
        operation: "coagent.review_diff".into(),
        permission_level: PermissionLevel::L1DiffReview,
        resources: ResourceSet {
            read_paths: vec![".agent/diffs/test.diff".into()],
            write_paths: vec![".agent/results/out.json".into()],
            network: false,
        },
    };

    // First evaluate to transition task to Running
    let decision = kernel.evaluate_operation(request);
    assert_eq!(decision.decision, RuntimeDecisionValue::Allow);

    // Now cancel
    let state = kernel.cancel_task("TASK-cancel-test").expect("cancel");
    assert_eq!(state, TaskStateValue::Cancelled);

    // Verify it's in the store as cancelled
    let stored = RuntimeStore::initialize(&root).expect("reopen store");
    let task_state = stored.load_task_state("TASK-cancel-test").expect("load");
    assert_eq!(task_state.value(), TaskStateValue::Cancelled);

    fs::remove_dir_all(&root).ok();
}

#[test]
fn cancel_task_on_terminal_is_error() {
    let root = temp_repo("cancel_terminal");
    let mut kernel = RuntimeKernel::initialize(config(root.clone())).expect("init");

    let request = RuntimeOperationRequest {
        task_id: "TASK-done".into(),
        request_id: Some("REQ-1".into()),
        operation: "coagent.review_diff".into(),
        permission_level: PermissionLevel::L1DiffReview,
        resources: ResourceSet {
            read_paths: vec![".agent/diffs/test.diff".into()],
            write_paths: vec![".agent/results/out.json".into()],
            network: false,
        },
    };
    kernel.evaluate_operation(request);

    // Complete the task first
    kernel.complete_task("TASK-done").expect("complete");

    // Cancelling a completed task should fail
    let result = kernel.cancel_task("TASK-done");
    assert!(result.is_err());

    fs::remove_dir_all(&root).ok();
}

#[test]
fn approve_task_transitions_waiting_approval_to_running() {
    let root = temp_repo("approve_task");
    let mut kernel = RuntimeKernel::initialize(config(root.clone())).expect("init");

    // Create a task and move it to WaitingApproval
    let mut state = TaskState::new("TASK-approve");
    state.transition_to(TaskStateValue::Running).unwrap();
    state.transition_to(TaskStateValue::WaitingApproval).unwrap();
    let store = RuntimeStore::initialize(&root).expect("store");
    store.upsert_task_state(&state).expect("upsert");

    // Approve it
    let new_state = kernel.approve_task("TASK-approve").expect("approve");
    assert_eq!(new_state, TaskStateValue::Running);

    // Verify in store
    let stored = RuntimeStore::initialize(&root).expect("reopen");
    let task = stored.load_task_state("TASK-approve").expect("load");
    assert_eq!(task.value(), TaskStateValue::Running);

    fs::remove_dir_all(&root).ok();
}

#[test]
fn approve_task_rejects_non_waiting_state() {
    let root = temp_repo("approve_reject");
    let mut kernel = RuntimeKernel::initialize(config(root.clone())).expect("init");

    let store = RuntimeStore::initialize(&root).expect("store");
    let mut state = TaskState::new("TASK-running");
    state.transition_to(TaskStateValue::Running).unwrap();
    store.upsert_task_state(&state).expect("upsert");

    // Trying to approve a running task should fail
    let result = kernel.approve_task("TASK-running");
    assert!(result.is_err());

    // Trying to approve a completed task should fail
    let mut state = TaskState::new("TASK-done");
    state.transition_to(TaskStateValue::Running).unwrap();
    state.transition_to(TaskStateValue::Completed).unwrap();
    store.upsert_task_state(&state).expect("upsert");
    let result = kernel.approve_task("TASK-done");
    assert!(result.is_err());

    fs::remove_dir_all(&root).ok();
}

#[test]
fn transition_to_waiting_approval_updates_state() {
    let root = temp_repo("wait_approval");
    let mut kernel = RuntimeKernel::initialize(config(root.clone())).expect("init");

    let store = RuntimeStore::initialize(&root).expect("store");
    let mut state = TaskState::new("TASK-wait");
    state.transition_to(TaskStateValue::Running).unwrap();
    store.upsert_task_state(&state).expect("upsert");

    kernel.transition_to_waiting_approval("TASK-wait").expect("transition");

    let stored = RuntimeStore::initialize(&root).expect("reopen");
    let task = stored.load_task_state("TASK-wait").expect("load");
    assert_eq!(task.value(), TaskStateValue::WaitingApproval);

    fs::remove_dir_all(&root).ok();
}

#[test]
fn export_task_returns_full_history() {
    let root = temp_repo("export_task");
    let mut kernel = RuntimeKernel::initialize(config(root.clone())).expect("init");

    // Create task with operations
    let request = RuntimeOperationRequest {
        task_id: "TASK-export".into(),
        request_id: Some("REQ-1".into()),
        operation: "coagent.review_diff".into(),
        permission_level: PermissionLevel::L1DiffReview,
        resources: ResourceSet {
            read_paths: vec![".agent/diffs/test.diff".into()],
            write_paths: vec![".agent/results/out.json".into()],
            network: false,
        },
    };
    kernel.evaluate_operation(request);
    kernel.complete_operation("TASK-export", Some("REQ-1"), "coagent.review_diff").expect("complete op");
    kernel.complete_task("TASK-export").expect("complete task");

    let exported = kernel.export_task("TASK-export").expect("export").expect("some");
    assert_eq!(exported["task_id"], "TASK-export");
    assert_eq!(exported["state"], "completed");
    assert_eq!(exported["schema_version"], "coagent_export_v1");
    // Should have at least one decision and one step
    assert!(exported["decisions"].as_array().unwrap().len() > 0, "should have decisions");
    assert!(exported["steps"].as_array().unwrap().len() > 0, "should have steps");

    fs::remove_dir_all(&root).ok();
}

#[test]
fn export_task_returns_none_for_unknown_task() {
    let root = temp_repo("export_unknown");
    let kernel = RuntimeKernel::initialize(config(root.clone())).expect("init");
    let result = kernel.export_task("TASK-nonexistent").expect("export");
    assert!(result.is_none());
    fs::remove_dir_all(&root).ok();
}

#[test]
fn resume_task_suggests_action_for_running() {
    let root = temp_repo("resume_task");
    let mut kernel = RuntimeKernel::initialize(config(root.clone())).expect("init");

    let request = RuntimeOperationRequest {
        task_id: "TASK-resume".into(),
        request_id: Some("REQ-1".into()),
        operation: "coagent.review_diff".into(),
        permission_level: PermissionLevel::L1DiffReview,
        resources: ResourceSet {
            read_paths: vec![".agent/diffs/test.diff".into()],
            write_paths: vec![".agent/results/out.json".into()],
            network: false,
        },
    };
    kernel.evaluate_operation(request);

    let result = kernel.resume_task("TASK-resume").expect("resume").expect("some");
    assert_eq!(result["task_id"], "TASK-resume");
    assert_eq!(result["state"], "running");
    assert!(!result["is_terminal"].as_bool().unwrap());
    assert!(result["suggested_action"].as_str().unwrap().len() > 0);

    fs::remove_dir_all(&root).ok();
}



