use std::collections::HashMap;

use crate::storage::{RuntimeStore, StoreError};

/// State reconstructed from replaying the event log.
#[derive(Debug, Clone, Default)]
pub struct ReplayedTaskState {
    pub steps_started: u64,
    pub steps_completed: u64,
    pub policy_decisions: u64,
    pub last_decision: Option<String>,
    pub last_lifecycle_state: Option<String>,
}

/// Replay a task's event log and rebuild its current state.
pub fn replay_task_state(
    store: &RuntimeStore,
    task_id: &str,
) -> Result<Option<ReplayedTaskState>, StoreError> {
    let events = store.runtime_events(task_id)?;
    if events.is_empty() {
        return Ok(None);
    }

    let mut replayed = ReplayedTaskState::default();
    let mut idempotency_seen: HashMap<String, i64> = HashMap::new();

    for event in &events {
        let idem_key = format!(
            "{}:{}:{}",
            event.task_id,
            event.request_id.as_deref().unwrap_or(""),
            event.event_type
        );

        if let Some(seen_seq) = idempotency_seen.get(&idem_key)
            && event.task_sequence <= *seen_seq
        {
            continue;
        }
        idempotency_seen.insert(idem_key.clone(), event.task_sequence);

        let payload: serde_json::Value =
            serde_json::from_str(&event.payload_json).unwrap_or(serde_json::Value::Null);

        match event.event_type.as_str() {
            "step_started" => {
                replayed.steps_started += 1;
            }
            "policy_evaluated" => {
                replayed.policy_decisions += 1;
                if let Some(decision) = payload.get("decision").and_then(|v| v.as_str()) {
                    replayed.last_decision = Some(decision.to_string());
                }
            }
            "lifecycle_closed" => {
                replayed.steps_completed += 1;
                if let Some(state) = payload.get("state").and_then(|v| v.as_str()) {
                    replayed.last_lifecycle_state = Some(state.to_string());
                }
            }
            _ => {}
        }
    }

    Ok(Some(replayed))
}

/// Check if an event type was already emitted for a task (idempotency check).
pub fn check_idempotency(
    store: &RuntimeStore,
    task_id: &str,
    event_type: &str,
) -> Result<bool, StoreError> {
    let events = store.runtime_events(task_id)?;
    Ok(events.iter().any(|e| e.event_type == event_type))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::RuntimeStore;
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_repo(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("coagent-replay-{name}-{unique}"));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn replay_produces_correct_step_counts() {
        let repo = temp_repo("replay");
        let store = RuntimeStore::initialize(&repo).unwrap();
        let step = store
            .start_runtime_step("TASK-r", Some("REQ-r"), "reasonix.review_diff")
            .unwrap();
        store
            .write_runtime_event("TASK-r", Some("REQ-r"), Some(step.id), "step_started", "{}")
            .unwrap();
        store
            .write_runtime_event(
                "TASK-r",
                Some("REQ-r"),
                Some(step.id),
                "policy_evaluated",
                r#"{"decision":"allow"}"#,
            )
            .unwrap();
        store.finish_runtime_step(step.id, "completed").unwrap();
        store
            .write_runtime_event(
                "TASK-r",
                Some("REQ-r"),
                Some(step.id),
                "lifecycle_closed",
                r#"{"state":"completed"}"#,
            )
            .unwrap();

        let replayed = replay_task_state(&store, "TASK-r").unwrap().unwrap();
        assert_eq!(replayed.steps_started, 1);
        assert_eq!(replayed.policy_decisions, 1);
        assert_eq!(replayed.steps_completed, 1);
        assert_eq!(replayed.last_decision.as_deref(), Some("allow"));
    }

    #[test]
    fn replay_nonexistent_task() {
        let repo = temp_repo("replay-none");
        let store = RuntimeStore::initialize(&repo).unwrap();
        assert!(replay_task_state(&store, "TASK-none").unwrap().is_none());
    }

    #[test]
    fn idempotency_detects_events() {
        let repo = temp_repo("idem");
        let store = RuntimeStore::initialize(&repo).unwrap();
        let step = store
            .start_runtime_step("TASK-idem", Some("REQ-idem"), "op")
            .unwrap();
        store
            .write_runtime_event(
                "TASK-idem",
                Some("REQ-idem"),
                Some(step.id),
                "step_started",
                "{}",
            )
            .unwrap();
        assert!(check_idempotency(&store, "TASK-idem", "step_started").unwrap());
        assert!(!check_idempotency(&store, "TASK-idem", "lifecycle_closed").unwrap());
    }
}
