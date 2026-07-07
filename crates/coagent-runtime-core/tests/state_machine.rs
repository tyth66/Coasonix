use coagent_runtime_core::state::{TaskState, TaskStateError, TaskStateValue};

#[test]
fn illegal_state_transition_is_denied() {
    let mut state = TaskState::new("TASK-state");

    let error = state
        .transition_to(TaskStateValue::Completed)
        .expect_err("queued task cannot complete directly");

    assert_eq!(error, TaskStateError::IllegalTransition);
    assert_eq!(state.value(), TaskStateValue::Queued);
}

#[test]
fn terminal_state_rejects_mutation() {
    let mut state = TaskState::new("TASK-state");
    state
        .transition_to(TaskStateValue::Running)
        .expect("queued -> running");
    state
        .transition_to(TaskStateValue::Failed)
        .expect("running -> failed");

    let error = state
        .transition_to(TaskStateValue::Running)
        .expect_err("terminal state must not mutate");

    assert_eq!(error, TaskStateError::TerminalState);
    assert_eq!(state.value(), TaskStateValue::Failed);
}

#[test]
fn completion_is_blocked_while_required_gaps_exist() {
    let mut state = TaskState::new("TASK-state");
    state.add_required_verification_gap("cargo test --workspace");
    state
        .transition_to(TaskStateValue::Running)
        .expect("queued -> running");

    let error = state
        .transition_to(TaskStateValue::Completed)
        .expect_err("required verification gap blocks completion");

    assert_eq!(error, TaskStateError::RequiredVerificationGaps);
    assert_eq!(state.value(), TaskStateValue::Running);
}

#[test]
fn agent_call_count_only_increments_through_runtime_decision() {
    let mut state = TaskState::new("TASK-state");

    state.note_adapter_observed_agent_attempt();
    assert_eq!(state.agent_calls(), 0);

    state.note_runtime_owned_agent_call();
    assert_eq!(state.agent_calls(), 1);
}

// Phase 1: New tests for 10-state FSM
#[test]
fn blocked_and_unblocked_allows_completion() {
    let mut state = TaskState::new("TASK-state");
    state.transition_to(TaskStateValue::Running).unwrap();
    state.transition_to(TaskStateValue::Blocked).unwrap();
    assert_eq!(state.value(), TaskStateValue::Blocked);
    state.transition_to(TaskStateValue::Running).unwrap();
    state.transition_to(TaskStateValue::Completed).unwrap();
    assert_eq!(state.value(), TaskStateValue::Completed);
}

#[test]
fn waiting_approval_then_approved_then_complete() {
    let mut state = TaskState::new("TASK-state");
    state.transition_to(TaskStateValue::Running).unwrap();
    state
        .transition_to(TaskStateValue::WaitingApproval)
        .unwrap();
    state.transition_to(TaskStateValue::Running).unwrap();
    state.transition_to(TaskStateValue::Completed).unwrap();
}

#[test]
fn waiting_approval_can_be_rejected_to_failed() {
    let mut state = TaskState::new("TASK-state");
    state.transition_to(TaskStateValue::Running).unwrap();
    state
        .transition_to(TaskStateValue::WaitingApproval)
        .unwrap();
    state.transition_to(TaskStateValue::Failed).unwrap();
    assert_eq!(state.value(), TaskStateValue::Failed);
}

#[test]
fn retry_then_succeed() {
    let mut state = TaskState::new("TASK-state");
    state.transition_to(TaskStateValue::Running).unwrap();
    state.transition_to(TaskStateValue::Retrying).unwrap();
    assert_eq!(state.retry_count(), 1);
    state.transition_to(TaskStateValue::Running).unwrap();
    state.transition_to(TaskStateValue::Completed).unwrap();
}

#[test]
fn subtask_dependencies_block_completion() {
    let mut state = TaskState::new("TASK-state");
    state.add_subtask("SUB-1", TaskStateValue::Completed);
    state.transition_to(TaskStateValue::Running).unwrap();
    let err = state.transition_to(TaskStateValue::Completed).unwrap_err();
    assert!(matches!(
        err,
        TaskStateError::SubtaskDependenciesUnresolved(_)
    ));
}

#[test]
fn partially_completed_then_all_subtasks_done_auto_completes() {
    let mut state = TaskState::new("TASK-state");
    state.add_subtask("SUB-1", TaskStateValue::Completed);
    state.transition_to(TaskStateValue::Running).unwrap();
    state
        .transition_to(TaskStateValue::PartiallyCompleted)
        .unwrap();
    let result = state
        .resolve_subtask_and_progress("SUB-1", TaskStateValue::Completed)
        .unwrap();
    assert_eq!(result, Some(TaskStateValue::Completed));
}
