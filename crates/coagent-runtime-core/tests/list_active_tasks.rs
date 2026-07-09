use std::path::PathBuf;
use coagent_runtime_core::{
    state::{TaskState, TaskStateValue},
    storage::RuntimeStore,
};

use std::fs;

fn temp_repo(name: &str) -> PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("coagent-store-{name}-{unique}"));
    fs::create_dir_all(&root).expect("create temp repo");
    root
}

#[test]
fn list_active_tasks_empty_when_no_tasks() {
    let root = temp_repo("list_active_empty");
    let store = RuntimeStore::initialize(&root).expect("init");
    let tasks = store.list_active_tasks().expect("list");
    assert!(tasks.is_empty());
    fs::remove_dir_all(&root).ok();
}

#[test]
fn list_active_tasks_filters_terminal_states() {
    let root = temp_repo("list_active_filter");
    let store = RuntimeStore::initialize(&root).expect("init");

    // Create a completed task
    let mut completed = TaskState::new("TASK-done");
    completed.transition_to(TaskStateValue::Running).unwrap();
    completed.transition_to(TaskStateValue::Completed).unwrap();
    store.upsert_task_state(&completed).expect("upsert");

    // Create a running task
    let mut running = TaskState::new("TASK-running");
    running.transition_to(TaskStateValue::Running).unwrap();
    store.upsert_task_state(&running).expect("upsert");

    // Create a queued task
    let queued = TaskState::new("TASK-queued");
    store.upsert_task_state(&queued).expect("upsert");

    let tasks = store.list_active_tasks().expect("list");
    // Only running and queued should appear (not completed)
    let task_ids: Vec<&str> = tasks.iter()
        .map(|t| t["task_id"].as_str().unwrap())
        .collect();
    assert!(task_ids.contains(&"TASK-running"));
    assert!(task_ids.contains(&"TASK-queued"));
    assert!(!task_ids.contains(&"TASK-done"));
    assert_eq!(tasks.len(), 2);

    fs::remove_dir_all(&root).ok();
}

#[test]
fn list_active_tasks_includes_step_count() {

    let root = temp_repo("list_active_steps");
    let store = RuntimeStore::initialize(&root).expect("init");

    // Create task with steps
    let mut task = TaskState::new("TASK-steps");
    task.transition_to(TaskStateValue::Running).unwrap();
    store.upsert_task_state(&task).expect("upsert");

    store.start_runtime_step("TASK-steps", Some("REQ-1"), "coagent.review_diff").expect("step1");
    store.start_runtime_step("TASK-steps", Some("REQ-2"), "coagent.review_diff").expect("step2");

    let tasks = store.list_active_tasks().expect("list");
    assert_eq!(tasks.len(), 1);
    let task = &tasks[0];
    assert_eq!(task["step_count"], 2);
    assert!(task["last_step_ms"].is_number() || task["last_step_ms"].is_null());

    fs::remove_dir_all(&root).ok();
}
