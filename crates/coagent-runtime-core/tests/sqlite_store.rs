use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use coagent_runtime_core::{
    policy::RuntimeDecisionValue,
    state::{TaskState, TaskStateValue},
    storage::{AuditEventInput, CacheMetadata, RuntimeDecisionRecord, RuntimeStore, StoreError},
};

fn temp_repo(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("coagent-store-{name}-{unique}"));
    fs::create_dir_all(&root).expect("create temp repo");
    root
}

fn audit(task_id: &str, event_type: &str) -> AuditEventInput {
    AuditEventInput {
        task_id: task_id.to_string(),
        event_type: event_type.to_string(),
        summary: format!("{event_type} summary"),
        payload_json: "{}".to_string(),
    }
}

fn deny_decision(task_id: &str, request_id: &str) -> RuntimeDecisionRecord {
    RuntimeDecisionRecord {
        task_id: task_id.to_string(),
        request_id: Some(request_id.to_string()),
        operation: "reasonix.review_diff".to_string(),
        decision: RuntimeDecisionValue::Deny,
        reasons: vec!["policy denied".to_string()],
        command_hash: Some(
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
        ),
    }
}

#[test]
fn database_created_under_agent_directory_with_required_pragmas() {
    let repo = temp_repo("created");
    let store = RuntimeStore::initialize(&repo).expect("initialize store");

    assert_eq!(store.database_path(), repo.join(".agent/coagent.sqlite"));
    assert!(store.database_path().exists());
    assert!(store.foreign_keys_enabled().expect("foreign keys pragma"));
    assert_eq!(store.journal_mode().expect("journal mode"), "wal");
    assert_eq!(store.synchronous_level().expect("synchronous pragma"), 2);
    assert_eq!(store.busy_timeout_ms().expect("busy timeout pragma"), 5000);
}

#[test]
fn migrations_create_tables_in_required_order_before_initialize_succeeds() {
    let repo = temp_repo("migrations");
    let store = RuntimeStore::initialize(&repo).expect("initialize store");

    assert_eq!(
        store.migration_tables().expect("migration table order"),
        vec![
            "runtime_metadata",
            "tasks",
            "audit_events",
            "task_state",
            "runtime_decisions",
            "schema_validation_results",
            "policy_evaluation_results",
            "runtime_steps",
            "runtime_events",
            "locks",
            "artifacts",
            "operation_attempts",
            "cache_entries",
        ]
    );
}

#[test]
fn failed_migration_blocks_store_initialization_and_side_effects() {
    let repo = temp_repo("failed-migration");

    let error = RuntimeStore::initialize_with_extra_migration(&repo, "INVALID SQL")
        .expect_err("invalid migration should fail");

    assert!(matches!(error, StoreError::MigrationFailed(_)));
    assert!(!repo.join(".agent/coagent.sqlite").exists());
}

#[test]
fn audit_events_are_append_only_and_sequences_are_monotonic() {
    let repo = temp_repo("audit");
    let store = RuntimeStore::initialize(&repo).expect("initialize store");

    let first = store
        .write_audit_event(&audit("TASK-audit", "created"))
        .expect("first audit");
    let second = store
        .write_audit_event(&audit("TASK-audit", "decision_denied"))
        .expect("second audit");
    let other_task = store
        .write_audit_event(&audit("TASK-other", "created"))
        .expect("other task audit");

    assert!(second.id > first.id);
    assert!(other_task.id > second.id);
    assert_eq!(first.task_sequence, 1);
    assert_eq!(second.task_sequence, 2);
    assert_eq!(other_task.task_sequence, 1);

    let update_error = store
        .try_update_audit_summary(first.id, "tamper")
        .expect_err("audit update rejected");
    assert!(matches!(update_error, StoreError::AppendOnlyViolation));

    let delete_error = store
        .try_delete_audit_event(first.id)
        .expect_err("audit delete rejected");
    assert!(matches!(delete_error, StoreError::AppendOnlyViolation));
}

#[test]
fn deny_decision_is_persisted() {
    let repo = temp_repo("deny");
    let store = RuntimeStore::initialize(&repo).expect("initialize store");

    store
        .commit_runtime_decision_with_audit(
            &deny_decision("TASK-deny", "REQ-deny"),
            &audit("TASK-deny", "decision_denied"),
        )
        .expect("commit deny decision with audit");

    assert_eq!(
        store
            .runtime_decision_count("TASK-deny", RuntimeDecisionValue::Deny)
            .expect("count deny decisions"),
        1
    );
}

#[test]
fn runtime_decision_and_audit_commit_atomically() {
    let repo = temp_repo("decision-audit");
    let store = RuntimeStore::initialize(&repo).expect("initialize store");
    let decision = deny_decision("TASK-decision-audit", "REQ-decision-audit");

    let audit_record = store
        .commit_runtime_decision_with_audit(
            &decision,
            &audit("TASK-decision-audit", "decision_denied"),
        )
        .expect("decision and audit commit");

    assert_eq!(audit_record.task_sequence, 1);
    assert_eq!(
        store
            .runtime_decision_count("TASK-decision-audit", RuntimeDecisionValue::Deny)
            .expect("count committed decision"),
        1
    );
}

#[test]
fn failed_audit_insert_rolls_back_runtime_decision() {
    let repo = temp_repo("decision-rollback");
    let store = RuntimeStore::initialize(&repo).expect("initialize store");
    let decision = deny_decision("TASK-decision-rollback", "REQ-decision-rollback");

    store
        .commit_runtime_decision_with_audit(&decision, &audit("TASK-decision-rollback", ""))
        .expect_err("invalid audit rolls back decision");

    assert_eq!(
        store
            .runtime_decision_count("TASK-decision-rollback", RuntimeDecisionValue::Deny)
            .expect("count rolled back decision"),
        0
    );
}

#[test]
fn state_and_audit_commit_atomically_and_rollback_on_audit_failure() {
    let repo = temp_repo("atomic");
    let store = RuntimeStore::initialize(&repo).expect("initialize store");
    let mut state = TaskState::new("TASK-atomic");
    state
        .transition_to(TaskStateValue::Running)
        .expect("created -> running");
    store.upsert_task_state(&state).expect("insert state");

    store
        .transition_state_with_audit(
            "TASK-atomic",
            TaskStateValue::Failed,
            &audit("TASK-atomic", "failed"),
        )
        .expect("state and audit commit");
    assert_eq!(
        store
            .load_task_state("TASK-atomic")
            .expect("load task state")
            .value(),
        TaskStateValue::Failed
    );

    let mut state = TaskState::new("TASK-rollback");
    state
        .transition_to(TaskStateValue::Running)
        .expect("created -> running");
    store
        .upsert_task_state(&state)
        .expect("insert rollback state");

    let failed_audit = audit("TASK-rollback", "");
    store
        .transition_state_with_audit("TASK-rollback", TaskStateValue::Failed, &failed_audit)
        .expect_err("invalid audit rolls back state update");
    assert_eq!(
        store
            .load_task_state("TASK-rollback")
            .expect("load rollback task state")
            .value(),
        TaskStateValue::Running
    );
}

#[test]
fn worker_restart_recovers_task_state() {
    let repo = temp_repo("restart");
    {
        let store = RuntimeStore::initialize(&repo).expect("initialize store");
        let mut state = TaskState::new("TASK-restart");
        state
            .transition_to(TaskStateValue::Running)
            .expect("created -> running");
        store
            .upsert_task_state(&state)
            .expect("persist running state");
    }

    let reopened = RuntimeStore::initialize(&repo).expect("reopen store");

    assert_eq!(
        reopened
            .load_task_state("TASK-restart")
            .expect("load after restart")
            .value(),
        TaskStateValue::Running
    );
}

#[test]
fn stale_locks_are_detected_on_startup() {
    let repo = temp_repo("locks");
    let store = RuntimeStore::initialize(&repo).expect("initialize store");
    store
        .insert_lock("LOCK-stale", "TASK-lock", 1_000)
        .expect("insert stale lock");
    store
        .insert_lock("LOCK-fresh", "TASK-lock", 9_900)
        .expect("insert fresh lock");

    let stale = store
        .stale_locks(10_000, 5_000)
        .expect("detect stale locks");

    assert_eq!(stale, vec!["LOCK-stale".to_string()]);
}

#[test]
fn cache_metadata_can_be_recorded_but_reuse_stays_disabled() {
    let repo = temp_repo("cache");
    let store = RuntimeStore::initialize(&repo).expect("initialize store");
    let metadata = CacheMetadata {
        cache_key: "CACHE-key".to_string(),
        payload_hash: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .to_string(),
    };

    store
        .record_cache_metadata(&metadata)
        .expect("record cache metadata");

    assert_eq!(store.cache_entry_count().expect("cache count"), 1);
    assert!(
        !store
            .cache_reuse_allowed("CACHE-key", &metadata.payload_hash)
            .expect("cache reuse disabled")
    );
}

#[test]
fn cache_corruption_denies_reuse_only() {
    let repo = temp_repo("cache-corrupt");
    let store = RuntimeStore::initialize(&repo).expect("initialize store");
    store
        .record_cache_metadata(&CacheMetadata {
            cache_key: "CACHE-key".to_string(),
            payload_hash: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
        })
        .expect("record cache metadata");

    assert!(
        !store
            .cache_reuse_allowed(
                "CACHE-key",
                "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            )
            .expect("cache corruption denies reuse without failing store")
    );

    store
        .write_audit_event(&audit("TASK-cache", "cache_corruption_detected"))
        .expect("store remains usable after corrupt cache metadata");
}

#[test]
fn runtime_steps_and_events_are_persisted_in_task_order() {
    let repo = temp_repo("events");
    let store = RuntimeStore::initialize(&repo).expect("initialize store");

    let step = store
        .start_runtime_step("TASK-events", Some("REQ-events"), "reasonix.review_diff")
        .expect("start step");
    assert_eq!(step.task_id, "TASK-events");
    assert_eq!(step.request_id.as_deref(), Some("REQ-events"));
    assert_eq!(step.operation, "reasonix.review_diff");
    assert_eq!(step.state, "running");

    let first = store
        .write_runtime_event(
            "TASK-events",
            Some("REQ-events"),
            Some(step.id),
            "step_started",
            "{}",
        )
        .expect("write first event");
    let second = store
        .write_runtime_event(
            "TASK-events",
            Some("REQ-events"),
            Some(step.id),
            "policy_evaluated",
            r#"{"decision":"allow"}"#,
        )
        .expect("write second event");

    assert_eq!(first.task_sequence, 1);
    assert_eq!(second.task_sequence, 2);

    store
        .finish_runtime_step(step.id, "completed")
        .expect("finish step");

    let events = store
        .runtime_events("TASK-events")
        .expect("load runtime events");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event_type, "step_started");
    assert_eq!(events[1].event_type, "policy_evaluated");
    assert_eq!(events[1].payload_json, r#"{"decision":"allow"}"#);

    let step = store.runtime_step(step.id).expect("load step");
    assert_eq!(step.state, "completed");
}
