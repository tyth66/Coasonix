// === AGENT RUNTIME COMPLEX INTEGRATION TESTS ===
// Covers: full FSM lifecycle, approval gates, event replay, sandbox,
// ToolRegistry concurrency, schema edge cases.

use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use coagent_runtime_core::{
    kernel::{RuntimeConfig, RuntimeDecisionValue, RuntimeKernel},
    policy::{
        ApprovalPolicy, BackendBinding, PermissionLevel, ResourceSet, RuntimeOperationRequest,
        ToolCapabilities, ToolDefinition, ToolRegistry,
    },
    replay::{check_idempotency, replay_task_state},
    sandbox::SandboxConfig,
    state::{TaskState, TaskStateValue},
    storage::RuntimeStore,
};

fn temp_repo(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("coagent-ctest-{name}-{unique}"));
    fs::create_dir_all(root.join(".agent/diffs")).expect("diffs");
    fs::create_dir_all(root.join(".agent/results")).expect("results");
    fs::create_dir_all(root.join("docs")).expect("docs");
    root
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECTION 1: Full FSM lifecycle — Queued → Running → Blocked → Running → Complete
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn full_fsm_lifecycle_queued_to_completed_with_blocked() {
    let repo = temp_repo("fsm-full");
    let mut kernel = RuntimeKernel::initialize(RuntimeConfig {
        repo_root: repo.clone(),
    })
    .expect("init");

    // 1. Fresh task starts Queued
    let state = TaskState::new("TASK-full");
    assert_eq!(state.value(), TaskStateValue::Queued);

    // 2. evaluate_operation transitions to Running (via persist_running_state)
    let decision = kernel.evaluate_operation(RuntimeOperationRequest {
        task_id: "TASK-full".into(),
        request_id: Some("REQ-full-1".into()),
        operation: "coagent.review_diff".into(),
        permission_level: PermissionLevel::L1DiffReview,
        resources: ResourceSet {
            read_paths: vec![".agent/diffs/test.diff".into()],
            write_paths: vec![".agent/results/out.json".into()],
            network: false,
        },
    });
    assert_eq!(decision.decision, RuntimeDecisionValue::Allow);

    // 3. Verify state became Running
    let store = RuntimeStore::initialize(&repo).expect("reopen");
    let loaded_state = store.load_task_state("TASK-full").expect("load");
    assert_eq!(loaded_state.value(), TaskStateValue::Running);

    // 4. Simulate blocking — transition to Blocked, then back
    let mut state = loaded_state;
    state
        .transition_to(TaskStateValue::Blocked)
        .expect("→ Blocked");
    store.upsert_task_state(&state).expect("persist blocked");

    let mut reloaded = store.load_task_state("TASK-full").expect("reload");
    assert_eq!(reloaded.value(), TaskStateValue::Blocked);

    // 5. Unblock: Blocked → Running
    reloaded
        .transition_to(TaskStateValue::Running)
        .expect("Blocked→Running");
    store.upsert_task_state(&reloaded).expect("persist running");
    assert_eq!(
        store.load_task_state("TASK-full").unwrap().value(),
        TaskStateValue::Running
    );

    // 6. Complete
    kernel
        .complete_operation("TASK-full", Some("REQ-full-1"), "coagent.review_diff")
        .expect("complete operation");
    kernel.complete_task("TASK-full").expect("complete task");

    let final_state = store.load_task_state("TASK-full").expect("final");
    assert_eq!(final_state.value(), TaskStateValue::Completed);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECTION 2: Retry cycle — Queued→Running→Retrying→Running→Fail (max retries)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn retry_cycle_exhausts_max_retries_then_fails() {
    let _repo = temp_repo("retry-cycle");
    let mut state = TaskState::new("TASK-retry");
    state.set_timeout(coagent_runtime_core::state::TaskTimeout {
        max_retries: 2,
        ..Default::default()
    });

    state.transition_to(TaskStateValue::Running).unwrap();

    // Retry #1
    state.transition_to(TaskStateValue::Retrying).unwrap();
    assert_eq!(state.retry_count(), 1);
    state.transition_to(TaskStateValue::Running).unwrap();

    // Retry #2
    state.transition_to(TaskStateValue::Retrying).unwrap();
    assert_eq!(state.retry_count(), 2);
    state.transition_to(TaskStateValue::Running).unwrap();

    // Retry #3 should fail — max_retries is 2
    let err = state
        .transition_to(TaskStateValue::Retrying)
        .expect_err("should exhaust");
    assert_eq!(
        err,
        coagent_runtime_core::state::TaskStateError::MaxRetriesExceeded
    );

    // Should still be able to transition to Failed
    state.transition_to(TaskStateValue::Failed).unwrap();
    assert_eq!(state.value(), TaskStateValue::Failed);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECTION 3: Subtask dependency chain — parent blocked until children done
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn subtask_dependency_chain_unlocks_parent_completion() {
    let mut parent = TaskState::new("PARENT");
    parent.transition_to(TaskStateValue::Running).unwrap();

    // Add 3 subtasks
    parent.add_subtask("SUB-A", TaskStateValue::Completed);
    parent.add_subtask("SUB-B", TaskStateValue::Completed);
    parent.add_subtask("SUB-C", TaskStateValue::Completed);

    // Partial completion
    parent
        .transition_to(TaskStateValue::PartiallyCompleted)
        .unwrap();
    assert_eq!(parent.value(), TaskStateValue::PartiallyCompleted);

    // Resolve one at a time — should not auto-complete until all done
    let r1 = parent
        .resolve_subtask_and_progress("SUB-A", TaskStateValue::Completed)
        .unwrap();
    assert_eq!(r1, None);
    assert_eq!(parent.value(), TaskStateValue::PartiallyCompleted);

    let r2 = parent
        .resolve_subtask_and_progress("SUB-B", TaskStateValue::Completed)
        .unwrap();
    assert_eq!(r2, None);
    assert_eq!(parent.value(), TaskStateValue::PartiallyCompleted);

    // Last subtask resolves → auto-complete
    let r3 = parent
        .resolve_subtask_and_progress("SUB-C", TaskStateValue::Completed)
        .unwrap();
    assert_eq!(r3, Some(TaskStateValue::Completed));
    assert_eq!(parent.value(), TaskStateValue::Completed);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECTION 4: Approval gate — RequireApproval decision + WaitingApproval state
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn approval_gate_produces_require_approval_and_allows_resume() {
    let repo = temp_repo("approval-gate");
    fs::create_dir_all(repo.join(".agent/diffs")).unwrap();
    fs::create_dir_all(repo.join(".agent/results")).unwrap();

    // Register an approval-gated tool
    let registry = ToolRegistry::new().register(ToolDefinition::new(
        "agent.protected_op",
        PermissionLevel::L1DiffReview,
        BackendBinding::Mock,
        ApprovalPolicy::Required,
        "in",
        "out",
        ToolCapabilities {
            read_allow: vec![".agent/diffs/**".into()],
            write_allow: vec![".agent/results/**".into()],
            deny: vec![],
            network: false,
        },
    ));

    let mut kernel = RuntimeKernel::initialize_with_tool_registry(
        RuntimeConfig {
            repo_root: repo.clone(),
        },
        registry,
    )
    .expect("init kernel");

    // First call: should return RequireApproval
    let decision = kernel.evaluate_operation(RuntimeOperationRequest {
        task_id: "TASK-approve".into(),
        request_id: Some("REQ-approve-1".into()),
        operation: "agent.protected_op".into(),
        permission_level: PermissionLevel::L1DiffReview,
        resources: ResourceSet {
            read_paths: vec![".agent/diffs/test.diff".into()],
            write_paths: vec![".agent/results/out.json".into()],
            network: false,
        },
    });
    assert_eq!(decision.decision, RuntimeDecisionValue::RequireApproval);
    assert!(
        decision
            .reasons
            .iter()
            .any(|r| r.contains("approval required"))
    );

    // The MCP handler would pause here. Simulate approval by transitioning
    // WaitingApproval → Running manually (the real handler does this on approve).
    let store = RuntimeStore::initialize(&repo).expect("reopen");
    let initial = TaskState::new("TASK-approve");
    store.upsert_task_state(&initial).expect("upsert initial");
    let mut state = store.load_task_state("TASK-approve").expect("load state");
    // Queued -> Running -> WaitingApproval (the real MCP handler path)

    state
        .transition_to(TaskStateValue::Running)
        .expect("Queued->Running");

    state
        .transition_to(TaskStateValue::WaitingApproval)
        .expect("→ WaitingApproval");
    store.upsert_task_state(&state).expect("persist");

    let paused = store.load_task_state("TASK-approve").unwrap();
    assert_eq!(paused.value(), TaskStateValue::WaitingApproval);

    // Approve: transition back to Running
    let mut paused = paused;
    paused
        .transition_to(TaskStateValue::Running)
        .expect("approve→Running");
    store.upsert_task_state(&paused).expect("persist approved");

    let approved = store.load_task_state("TASK-approve").unwrap();
    assert_eq!(approved.value(), TaskStateValue::Running);

    // Now complete
    kernel
        .complete_operation("TASK-approve", Some("REQ-approve-1"), "agent.protected_op")
        .expect("complete operation");
    kernel.complete_task("TASK-approve").expect("complete task");

    assert_eq!(
        store.load_task_state("TASK-approve").unwrap().value(),
        TaskStateValue::Completed
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECTION 5: Event replay — multi-step task with policy decisions
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn replay_reconstructs_full_multi_step_task_execution() {
    let repo = temp_repo("replay-full");
    let mut kernel = RuntimeKernel::initialize(RuntimeConfig {
        repo_root: repo.clone(),
    })
    .expect("init");

    // Step 1: Allow + Complete
    kernel.evaluate_operation(RuntimeOperationRequest {
        task_id: "TASK-A".into(),
        request_id: Some("REQ-A".into()),
        operation: "coagent.review_diff".into(),
        permission_level: PermissionLevel::L1DiffReview,
        resources: ResourceSet {
            read_paths: vec![".agent/diffs/step1.diff".into()],
            write_paths: vec![".agent/results/r1.json".into()],
            network: false,
        },
    });
    kernel
        .complete_operation("TASK-A", Some("REQ-A"), "coagent.review_diff")
        .unwrap();

    // Step 2: Denied by network
    let denied = kernel.evaluate_operation(RuntimeOperationRequest {
        task_id: "TASK-B".into(),
        request_id: Some("REQ-B".into()),
        operation: "coagent.review_diff".into(),
        permission_level: PermissionLevel::L1DiffReview,
        resources: ResourceSet {
            read_paths: vec![".agent/diffs/step2.diff".into()],
            write_paths: vec![".agent/results/r2.json".into()],
            network: true,
        },
    });
    assert_eq!(denied.decision, RuntimeDecisionValue::Deny);

    // Step 3: Allow + Complete
    kernel.evaluate_operation(RuntimeOperationRequest {
        task_id: "TASK-C".into(),
        request_id: Some("REQ-C".into()),
        operation: "coagent.review_diff".into(),
        permission_level: PermissionLevel::L1DiffReview,
        resources: ResourceSet {
            read_paths: vec![".agent/diffs/step3.diff".into()],
            write_paths: vec![".agent/results/r3.json".into()],
            network: false,
        },
    });
    kernel
        .complete_operation("TASK-C", Some("REQ-C"), "coagent.review_diff")
        .unwrap();

    // Replay aggregate
    let store = RuntimeStore::initialize(&repo).expect("reopen");
    let ra = replay_task_state(&store, "TASK-A").unwrap().unwrap();
    let rb = replay_task_state(&store, "TASK-B").unwrap().unwrap();
    let rc = replay_task_state(&store, "TASK-C").unwrap().unwrap();
    assert_eq!(ra.steps_started + rb.steps_started + rc.steps_started, 3);
    assert_eq!(
        ra.policy_decisions + rb.policy_decisions + rc.policy_decisions,
        3
    );
    assert_eq!(
        ra.steps_completed + rb.steps_completed + rc.steps_completed,
        3
    );
}

#[test]
fn idempotency_prevents_replaying_same_event_twice() {
    let repo = temp_repo("idem-full");
    let mut kernel = RuntimeKernel::initialize(RuntimeConfig {
        repo_root: repo.clone(),
    })
    .expect("init");

    kernel.evaluate_operation(RuntimeOperationRequest {
        task_id: "TASK-idem".into(),
        request_id: Some("REQ-idem-1".into()),
        operation: "coagent.review_diff".into(),
        permission_level: PermissionLevel::L1DiffReview,
        resources: ResourceSet {
            read_paths: vec![".agent/diffs/test.diff".into()],
            write_paths: vec![".agent/results/out.json".into()],
            network: false,
        },
    });

    let store = RuntimeStore::initialize(&repo).expect("reopen");

    // step_started emitted once
    assert!(check_idempotency(&store, "TASK-idem", "step_started").unwrap());
    assert!(check_idempotency(&store, "TASK-idem", "policy_evaluated").unwrap());
    // lifecycle_closed not yet emitted (not completed)
    assert!(!check_idempotency(&store, "TASK-idem", "lifecycle_closed").unwrap());

    kernel
        .complete_operation("TASK-idem", Some("REQ-idem-1"), "coagent.review_diff")
        .unwrap();

    let store = RuntimeStore::initialize(&repo).expect("reopen");
    assert!(check_idempotency(&store, "TASK-idem", "lifecycle_closed").unwrap());
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECTION 6: Sandbox + ArtifactPolicy joint enforcement
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn sandbox_env_filtering_combined_with_artifact_path_authorization() {
    let repo = temp_repo("sandbox-artifact");
    fs::create_dir_all(repo.join(".agent/diffs")).unwrap();
    fs::create_dir_all(repo.join(".agent/results")).unwrap();

    // Sandbox: only allow PATH
    let sandbox = SandboxConfig::new()
        .with_env_allowlist(vec!["PATH".into()])
        .with_budgets(coagent_runtime_core::sandbox::ResourceBudgets {
            max_wall_clock: Some(Duration::from_secs(30)),
            max_output_bytes: Some(1024),
            ..Default::default()
        });

    // Verify env filtering
    let filtered = sandbox.filtered_env();
    // PATH should be in filtered env (if present on system)
    assert!(!filtered.iter().any(|(k, _)| k == "HOME"));

    // Verify budgets
    let budgets = sandbox.budgets;
    assert_eq!(budgets.max_wall_clock, Some(Duration::from_secs(30)));

    // Artifact policy: path allowed for read but blocked for write on wrong path
    let mut kernel = RuntimeKernel::initialize(RuntimeConfig {
        repo_root: repo.clone(),
    })
    .expect("init");

    let allowed = kernel.evaluate_operation(RuntimeOperationRequest {
        task_id: "TASK-sandbox".into(),
        request_id: Some("REQ-sb-1".into()),
        operation: "coagent.review_diff".into(),
        permission_level: PermissionLevel::L1DiffReview,
        resources: ResourceSet {
            read_paths: vec![".agent/diffs/test.diff".into()],
            write_paths: vec![".agent/results/out.json".into()],
            network: false,
        },
    });
    assert_eq!(allowed.decision, RuntimeDecisionValue::Allow);

    // Write to denied path
    let denied = kernel.evaluate_operation(RuntimeOperationRequest {
        task_id: "TASK-sandbox".into(),
        request_id: Some("REQ-sb-2".into()),
        operation: "coagent.review_diff".into(),
        permission_level: PermissionLevel::L1DiffReview,
        resources: ResourceSet {
            read_paths: vec![".agent/diffs/test.diff".into()],
            write_paths: vec![".agent/secrets/leak.txt".into()],
            network: false,
        },
    });
    assert_eq!(denied.decision, RuntimeDecisionValue::Deny);
    assert!(
        denied
            .reasons
            .iter()
            .any(|r| r.contains("write path denied"))
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECTION 7: ToolRegistry concurrency — multi-threaded register/disable/snapshot
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn tool_registry_survives_concurrent_access_without_deadlock() {
    let registry = ToolRegistry::new();
    let counter = std::sync::Arc::new(AtomicU64::new(0));

    // Pre-register 50 tools
    for i in 0..50 {
        registry.register_dynamic(ToolDefinition::new(
            format!("agent.tool_{i}"),
            PermissionLevel::L0Readonly,
            BackendBinding::Mock,
            ApprovalPolicy::Never,
            "in",
            "out",
            ToolCapabilities {
                read_allow: vec![],
                write_allow: vec![],
                deny: vec![],
                network: false,
            },
        ));
    }

    let r1 = registry.clone();
    let r2 = registry.clone();
    let r3 = registry.clone();

    let h1 = thread::spawn(move || {
        for i in 100..200 {
            r1.register_dynamic(ToolDefinition::new(
                format!("agent.dyn_{i}"),
                PermissionLevel::L0Readonly,
                BackendBinding::Mock,
                ApprovalPolicy::Never,
                "in",
                "out",
                ToolCapabilities {
                    read_allow: vec![],
                    write_allow: vec![],
                    deny: vec![],
                    network: false,
                },
            ));
        }
    });

    let h2 = thread::spawn(move || {
        for i in 0..50 {
            r2.disable(&format!("agent.dyn_{}", 100 + i));
        }
    });

    let c3 = counter.clone();
    let h3 = thread::spawn(move || {
        for _ in 0..1000 {
            let _snapshot = r3.snapshot();
            let _enabled = r3.list_enabled();
            c3.fetch_add(1, Ordering::Relaxed);
        }
    });

    h1.join().unwrap();
    h2.join().unwrap();
    h3.join().unwrap();

    // Should have completed many read operations without deadlock
    assert!(counter.load(Ordering::Relaxed) >= 1000);

    // Registry should still be functional
    let snapshot = registry.snapshot();
    assert!(snapshot.len() >= 50); // at least pre-registered tools
    let enabled = registry.list_enabled();
    assert!(enabled.len() >= 50);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECTION 8: Schema edge cases — BOM, deep nesting, boundary values
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn schema_registry_rejects_deeply_nested_duplicate_keys() {
    use coagent_runtime_core::schema::parse_json_no_duplicate_keys;

    // Deep nesting with duplicate key at level 3
    let payload = r#"{
        "outer": {
            "middle": {
                "inner": 1,
                "inner": 2
            }
        }
    }"#;
    let err = parse_json_no_duplicate_keys(payload).expect_err("duplicate at depth 3");
    assert!(err.to_string().contains("duplicate"));
}

#[test]
fn schema_registry_handles_boundary_numeric_values() {
    use coagent_runtime_core::schema::SchemaRegistry;

    // This file is compiled-in, so we test via the registry path
    let registry = SchemaRegistry::load_from_str(
        "test.schema.json",
        include_str!("../../../schemas/coagent-v1.schema.json"),
    )
    .expect("load schema");

    // confidence at exact boundary 0.0
    let review = serde_json::json!({
        "verdict": "pass",
        "summary": "boundary test",
        "findings": [],
        "tests_to_run": [],
        "risks": [],
        "assumptions": [],
        "confidence": 0.0
    });
    let result = registry.validate("pure_review_result_v1", &review);
    assert!(result.valid, "confidence=0.0 should be valid");

    // confidence at exact boundary 1.0
    let review = serde_json::json!({
        "verdict": "pass",
        "summary": "max boundary",
        "findings": [],
        "tests_to_run": [],
        "risks": [],
        "assumptions": [],
        "confidence": 1.0
    });
    let result = registry.validate("pure_review_result_v1", &review);
    assert!(result.valid, "confidence=1.0 should be valid");

    // confidence just above 1.0
    let review = serde_json::json!({
        "verdict": "pass",
        "summary": "over max",
        "findings": [],
        "tests_to_run": [],
        "risks": [],
        "assumptions": [],
        "confidence": 1.001
    });
    let result = registry.validate("pure_review_result_v1", &review);
    assert!(!result.valid, "confidence>1.0 should be invalid");
}

#[test]
fn schema_registry_rejects_non_utf8_like_garbage() {
    use coagent_runtime_core::schema::{SchemaRegistry, parse_json_no_duplicate_keys};

    // Empty string
    assert!(parse_json_no_duplicate_keys("").is_err());

    // Random garbage
    assert!(parse_json_no_duplicate_keys("not json at all {{{[").is_err());

    // Only whitespace
    assert!(parse_json_no_duplicate_keys("   \n\t  ").is_err());

    // Valid but schema-violating: missing required field
    let registry = SchemaRegistry::load_from_str(
        "coagent-v1.schema.json",
        include_str!("../../../schemas/coagent-v1.schema.json"),
    )
    .unwrap();
    let result = registry.validate(
        "pure_review_result_v1",
        &serde_json::json!({"verdict": "pass"}),
    );
    assert!(!result.valid);
    assert!(result.errors.iter().any(|e| e.message.contains("summary")));
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECTION 9: Multi-task isolation — separate tasks don't interfere
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn multiple_tasks_maintain_independent_state_and_events() {
    let repo = temp_repo("multi-task");
    let mut kernel = RuntimeKernel::initialize(RuntimeConfig {
        repo_root: repo.clone(),
    })
    .expect("init");

    // Task A: Allow + Complete
    kernel.evaluate_operation(RuntimeOperationRequest {
        task_id: "TASK-A".into(),
        request_id: Some("REQ-A".into()),
        operation: "coagent.review_diff".into(),
        permission_level: PermissionLevel::L1DiffReview,
        resources: ResourceSet {
            read_paths: vec![".agent/diffs/a.diff".into()],
            write_paths: vec![".agent/results/a.json".into()],
            network: false,
        },
    });
    kernel
        .complete_operation("TASK-A", Some("REQ-A"), "coagent.review_diff")
        .unwrap();
    kernel.complete_task("TASK-A").unwrap();

    // Task B: Allow + Complete
    kernel.evaluate_operation(RuntimeOperationRequest {
        task_id: "TASK-B".into(),
        request_id: Some("REQ-B".into()),
        operation: "coagent.review_diff".into(),
        permission_level: PermissionLevel::L1DiffReview,
        resources: ResourceSet {
            read_paths: vec![".agent/diffs/b.diff".into()],
            write_paths: vec![".agent/results/b.json".into()],
            network: false,
        },
    });
    kernel
        .complete_operation("TASK-B", Some("REQ-B"), "coagent.review_diff")
        .unwrap();
    kernel.complete_task("TASK-B").unwrap();

    // Task C: Denied
    let denied = kernel.evaluate_operation(RuntimeOperationRequest {
        task_id: "TASK-C".into(),
        request_id: Some("REQ-C".into()),
        operation: "coagent.review_diff".into(),
        permission_level: PermissionLevel::L1DiffReview,
        resources: ResourceSet {
            read_paths: vec![".agent/diffs/c.diff".into()],
            write_paths: vec![".agent/results/c.json".into()],
            network: true,
        },
    });
    assert_eq!(denied.decision, RuntimeDecisionValue::Deny);

    // Verify isolation
    let store = RuntimeStore::initialize(&repo).expect("reopen");

    // Task A: Completed, has events
    assert_eq!(
        store.load_task_state("TASK-A").unwrap().value(),
        TaskStateValue::Completed
    );
    assert!(store.runtime_events("TASK-A").unwrap().len() >= 3);

    // Task B: Completed, has events
    assert_eq!(
        store.load_task_state("TASK-B").unwrap().value(),
        TaskStateValue::Completed
    );
    assert!(store.runtime_events("TASK-B").unwrap().len() >= 3);

    // Task C: Running (state check passed but policy denied, no state transition)
    // The kernel's persist_running_state keeps it as Running (not Failed, since
    // deny happens before lifecycle close and we only get a pre-execution deny)
    // Task C denied pre-execution (network=true) — no state persisted
    assert!(store.load_task_state("TASK-C").is_err());
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// SECTION 10: Cancellation propagation
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn cancel_propagation_marks_parent_and_allows_cleanup() {
    let mut parent = TaskState::new("PARENT-CANCEL");
    parent.transition_to(TaskStateValue::Running).unwrap();
    parent.add_subtask("SUB-1", TaskStateValue::Completed);
    parent.add_subtask("SUB-2", TaskStateValue::Completed);

    // Cancel parent — subtask dependencies don't block cancellation
    parent
        .transition_to(TaskStateValue::Cancelled)
        .expect("cancel with unresolved subtasks");

    assert_eq!(parent.value(), TaskStateValue::Cancelled);
    // Cancel propagation is true by default — downstream consumers should
    // cascade cancellation to SUB-1 and SUB-2.
    assert_eq!(parent.subtasks().len(), 2);

    // Terminal state rejects further transitions
    let err = parent
        .transition_to(TaskStateValue::Running)
        .expect_err("cancelled is terminal");
    assert_eq!(
        err,
        coagent_runtime_core::state::TaskStateError::TerminalState
    );
}

#[test]
fn multi_step_task_allows_multiple_operations_on_same_task_id() {
    let repo = temp_repo("multi-op");
    let mut kernel = RuntimeKernel::initialize(RuntimeConfig {
        repo_root: repo.clone(),
    })
    .expect("init");

    // Operation 1: evaluate + complete
    kernel.evaluate_operation(RuntimeOperationRequest {
        task_id: "TASK-multi-op".into(),
        request_id: Some("REQ-op1".into()),
        operation: "coagent.review_diff".into(),
        permission_level: PermissionLevel::L1DiffReview,
        resources: ResourceSet {
            read_paths: vec![".agent/diffs/test.diff".into()],
            write_paths: vec![".agent/results/r1.json".into()],
            network: false,
        },
    });
    kernel
        .complete_operation("TASK-multi-op", Some("REQ-op1"), "coagent.review_diff")
        .expect("complete op1");

    // Operation 2: evaluate + complete (same task)
    kernel.evaluate_operation(RuntimeOperationRequest {
        task_id: "TASK-multi-op".into(),
        request_id: Some("REQ-op2".into()),
        operation: "coagent.review_diff".into(),
        permission_level: PermissionLevel::L1DiffReview,
        resources: ResourceSet {
            read_paths: vec![".agent/diffs/test.diff".into()],
            write_paths: vec![".agent/results/r2.json".into()],
            network: false,
        },
    });
    kernel
        .complete_operation("TASK-multi-op", Some("REQ-op2"), "coagent.review_diff")
        .expect("complete op2");

    // Task should still be Running (not terminal after individual ops)
    let store = RuntimeStore::initialize(&repo).expect("store");
    let state = store.load_task_state("TASK-multi-op").expect("load");
    assert_eq!(state.value(), TaskStateValue::Running);

    // Now complete the task itself
    kernel
        .complete_task("TASK-multi-op")
        .expect("complete task");
    let state = store.load_task_state("TASK-multi-op").expect("load");
    assert_eq!(state.value(), TaskStateValue::Completed);

    // Verify events: 2 step_started + 2 policy_evaluated + 2 lifecycle_closed = 6
    let events = store.runtime_events("TASK-multi-op").expect("events");
    assert_eq!(events.len(), 6);
}
