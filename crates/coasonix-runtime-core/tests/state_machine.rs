use coasonix_runtime_core::state::{TaskState, TaskStateError, TaskStateValue};

#[test]
fn illegal_state_transition_is_denied() {
    let mut state = TaskState::new("TASK-state");

    let error = state
        .transition_to(TaskStateValue::Completed)
        .expect_err("created task cannot complete directly");

    assert_eq!(error, TaskStateError::IllegalTransition);
    assert_eq!(state.value(), TaskStateValue::Created);
}

#[test]
fn terminal_state_rejects_mutation() {
    let mut state = TaskState::new("TASK-state");
    state
        .transition_to(TaskStateValue::Running)
        .expect("created -> running");
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
        .expect("created -> running");

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

