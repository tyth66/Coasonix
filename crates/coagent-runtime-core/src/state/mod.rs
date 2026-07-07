use std::time::Duration;

/// Expanded task state: 10 states covering queued, blocked, approval, retry, partial-completion.
///
/// ┌─────────────┐
/// │   Queued    │ ←── entry point (was Created)
/// └──────┬──────┘
///        │
///   ┌────▼─────┐     ┌──────────────┐
///   │  Running  │────▶│   Blocked    │ (dependency, resource, lock)
///   └────┬─────┘     └──────┬───────┘
///        │                  │
///   ┌────▼─────────┐       │ (unblock)
///   │   Retrying    │◄──────┘
///   └────┬─────────┘
///        │
///   ┌────▼──────────────┐
///   │ WaitingApproval    │ ←── paused, human-in-the-loop
///   └────┬──────────────┘
///        │
///   ┌────▼──────────────────┐
///   │ PartiallyCompleted    │ ←── subtasks done, awaiting rest
///   └────┬──────────────────┘
///        │
///   ┌────▼─────┐
///   │ Completed │ (terminal)
///   └──────────┘
///
///   Any non-terminal state → Failed (terminal)
///   Any non-terminal state → Cancelled (terminal)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskStateValue {
    Queued,
    Running,
    Blocked,
    WaitingApproval,
    Retrying,
    PartiallyCompleted,
    Completed,
    Failed,
    Cancelled,
}

impl TaskStateValue {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    pub fn is_alive(self) -> bool {
        !self.is_terminal()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubtaskDependency {
    pub subtask_id: String,
    pub required_state: TaskStateValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskTimeout {
    pub max_duration: Duration,
    pub max_blocked_duration: Duration,
    pub max_approval_duration: Duration,
    pub max_retries: u32,
}

impl Default for TaskTimeout {
    fn default() -> Self {
        Self {
            max_duration: Duration::from_secs(3600),
            max_blocked_duration: Duration::from_secs(600),
            max_approval_duration: Duration::from_secs(1800),
            max_retries: 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskState {
    task_id: String,
    value: TaskStateValue,
    agent_calls: u32,
    required_verification_gaps: Vec<String>,
    subtasks: Vec<SubtaskDependency>,
    timeout: TaskTimeout,
    retry_count: u32,
    cancel_propagation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStateError {
    IllegalTransition,
    TerminalState,
    RequiredVerificationGaps,
    SubtaskDependenciesUnresolved(Vec<String>),
    MaxRetriesExceeded,
    TimeoutExceeded,
}

impl TaskState {
    pub fn new(task_id: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            value: TaskStateValue::Queued,
            agent_calls: 0,
            required_verification_gaps: Vec::new(),
            subtasks: Vec::new(),
            timeout: TaskTimeout::default(),
            retry_count: 0,
            cancel_propagation: true,
        }
    }

    pub fn restore(task_id: impl Into<String>, value: TaskStateValue, agent_calls: u32) -> Self {
        Self {
            task_id: task_id.into(),
            value,
            agent_calls,
            required_verification_gaps: Vec::new(),
            subtasks: Vec::new(),
            timeout: TaskTimeout::default(),
            retry_count: 0,
            cancel_propagation: true,
        }
    }

    pub fn restore_full(
        task_id: impl Into<String>,
        value: TaskStateValue,
        agent_calls: u32,
        retry_count: u32,
        subtasks: Vec<SubtaskDependency>,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            value,
            agent_calls,
            required_verification_gaps: Vec::new(),
            subtasks,
            timeout: TaskTimeout::default(),
            retry_count,
            cancel_propagation: true,
        }
    }

    pub fn task_id(&self) -> &str {
        &self.task_id
    }
    pub fn value(&self) -> TaskStateValue {
        self.value
    }
    pub fn agent_calls(&self) -> u32 {
        self.agent_calls
    }
    pub fn retry_count(&self) -> u32 {
        self.retry_count
    }
    pub fn subtasks(&self) -> &[SubtaskDependency] {
        &self.subtasks
    }
    pub fn timeout(&self) -> &TaskTimeout {
        &self.timeout
    }
    pub fn set_timeout(&mut self, timeout: TaskTimeout) {
        self.timeout = timeout;
    }
    pub fn set_cancel_propagation(&mut self, propagate: bool) {
        self.cancel_propagation = propagate;
    }
    pub fn add_required_verification_gap(&mut self, gap: impl Into<String>) {
        self.required_verification_gaps.push(gap.into());
    }

    pub fn add_subtask(&mut self, subtask_id: impl Into<String>, required_state: TaskStateValue) {
        self.subtasks.push(SubtaskDependency {
            subtask_id: subtask_id.into(),
            required_state,
        });
    }

    pub fn resolve_subtask(&mut self, subtask_id: &str) {
        self.subtasks.retain(|dep| dep.subtask_id != subtask_id);
    }

    pub fn subtasks_resolved(&self) -> bool {
        self.subtasks.is_empty()
    }

    pub fn note_adapter_observed_agent_attempt(&mut self) {}
    pub fn note_runtime_owned_agent_call(&mut self) {
        self.agent_calls += 1;
    }

    pub fn transition_to(&mut self, next: TaskStateValue) -> Result<(), TaskStateError> {
        if self.value.is_terminal() {
            return Err(TaskStateError::TerminalState);
        }
        if next == TaskStateValue::Completed {
            if !self.required_verification_gaps.is_empty() {
                return Err(TaskStateError::RequiredVerificationGaps);
            }
            if !self.subtasks_resolved() {
                let unresolved: Vec<String> = self
                    .subtasks
                    .iter()
                    .map(|dep| format!("{}=>{:?}", dep.subtask_id, dep.required_state))
                    .collect();
                return Err(TaskStateError::SubtaskDependenciesUnresolved(unresolved));
            }
        }
        if next == TaskStateValue::Retrying {
            if self.retry_count >= self.timeout.max_retries {
                return Err(TaskStateError::MaxRetriesExceeded);
            }
            self.retry_count += 1;
        }
        if !is_legal_transition(self.value, next) {
            return Err(TaskStateError::IllegalTransition);
        }
        self.value = next;
        Ok(())
    }

    pub fn resolve_subtask_and_progress(
        &mut self,
        subtask_id: &str,
        _subtask_state: TaskStateValue,
    ) -> Result<Option<TaskStateValue>, TaskStateError> {
        self.resolve_subtask(subtask_id);
        if self.subtasks_resolved() {
            if self.value == TaskStateValue::PartiallyCompleted {
                if self.required_verification_gaps.is_empty() {
                    self.transition_to(TaskStateValue::Completed)?;
                    return Ok(Some(TaskStateValue::Completed));
                }
                return Ok(None);
            }
            return Ok(None);
        }
        Ok(None)
    }
}

fn is_legal_transition(current: TaskStateValue, next: TaskStateValue) -> bool {
    use TaskStateValue::*;
    matches!(
        (current, next),
        (Queued, Running)
            | (Queued, Cancelled)
            | (Running, Completed)
            | (Running, Failed)
            | (Running, Cancelled)
            | (Running, Blocked)
            | (Running, WaitingApproval)
            | (Running, Retrying)
            | (Running, PartiallyCompleted)
            | (Blocked, Running)
            | (Blocked, Cancelled)
            | (WaitingApproval, Running)
            | (WaitingApproval, Cancelled)
            | (WaitingApproval, Failed)
            | (Retrying, Running)
            | (Retrying, Cancelled)
            | (PartiallyCompleted, Completed)
            | (PartiallyCompleted, Failed)
            | (PartiallyCompleted, Cancelled)
    )
}



/// Per-backend-invocation attempt within an operation.
/// Each operation may have multiple attempts (retries, fallbacks).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptStateValue {
    /// Attempt is queued, not yet dispatched.
    Pending,
    /// Attempt is in progress (backend is executing).
    Running,
    /// Attempt completed successfully.
    Succeeded,
    /// Attempt failed (backend error, timeout, protocol error).
    Failed,
}

impl AttemptStateValue {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed)
    }
}

/// Record of a single backend invocation attempt.
#[derive(Debug, Clone)]
pub struct OperationAttempt {
    pub task_id: String,
    pub request_id: Option<String>,
    pub operation: String,
    pub backend_id: String,
    pub attempt_number: u32,
    pub state: AttemptStateValue,
    pub error: Option<String>,
}

impl OperationAttempt {
    pub fn new(
        task_id: impl Into<String>,
        request_id: Option<String>,
        operation: impl Into<String>,
        backend_id: impl Into<String>,
        attempt_number: u32,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            request_id,
            operation: operation.into(),
            backend_id: backend_id.into(),
            attempt_number,
            state: AttemptStateValue::Pending,
            error: None,
        }
    }

    pub fn start(&mut self) {
        self.state = AttemptStateValue::Running;
    }

    pub fn succeed(&mut self) {
        self.state = AttemptStateValue::Succeeded;
    }

    pub fn fail(&mut self, error: impl Into<String>) {
        self.state = AttemptStateValue::Failed;
        self.error = Some(error.into());
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_is_entry_point() {
        let state = TaskState::new("TASK-q");
        assert_eq!(state.value(), TaskStateValue::Queued);
        assert_eq!(state.retry_count(), 0);
        assert!(state.subtasks_resolved());
    }

    #[test]
    fn queued_to_running_allowed() {
        let mut state = TaskState::new("TASK-q");
        state.transition_to(TaskStateValue::Running).unwrap();
        assert_eq!(state.value(), TaskStateValue::Running);
    }

    #[test]
    fn queued_to_completed_illegal() {
        let mut state = TaskState::new("TASK-q");
        let err = state.transition_to(TaskStateValue::Completed).unwrap_err();
        assert_eq!(err, TaskStateError::IllegalTransition);
    }

    #[test]
    fn running_blocked_roundtrip() {
        let mut state = TaskState::new("TASK-q");
        state.transition_to(TaskStateValue::Running).unwrap();
        state.transition_to(TaskStateValue::Blocked).unwrap();
        assert_eq!(state.value(), TaskStateValue::Blocked);
        state.transition_to(TaskStateValue::Running).unwrap();
        assert_eq!(state.value(), TaskStateValue::Running);
    }

    #[test]
    fn retry_count_increments() {
        let mut state = TaskState::new("TASK-q");
        state.transition_to(TaskStateValue::Running).unwrap();
        state.transition_to(TaskStateValue::Retrying).unwrap();
        assert_eq!(state.retry_count(), 1);
        state.transition_to(TaskStateValue::Running).unwrap();
        state.transition_to(TaskStateValue::Retrying).unwrap();
        assert_eq!(state.retry_count(), 2);
    }

    #[test]
    fn max_retries_blocks_retry() {
        let mut state = TaskState::new("TASK-q");
        state.set_timeout(TaskTimeout {
            max_retries: 1,
            ..TaskTimeout::default()
        });
        state.transition_to(TaskStateValue::Running).unwrap();
        state.transition_to(TaskStateValue::Retrying).unwrap();
        state.transition_to(TaskStateValue::Running).unwrap();
        let err = state.transition_to(TaskStateValue::Retrying).unwrap_err();
        assert_eq!(err, TaskStateError::MaxRetriesExceeded);
    }

    #[test]
    fn subtask_dependencies_block_completion() {
        let mut state = TaskState::new("TASK-q");
        state.transition_to(TaskStateValue::Running).unwrap();
        state.add_subtask("T1", TaskStateValue::Completed);
        state.add_subtask("T2", TaskStateValue::Completed);
        let err = state.transition_to(TaskStateValue::Completed).unwrap_err();
        assert!(matches!(
            err,
            TaskStateError::SubtaskDependenciesUnresolved(_)
        ));
        state.resolve_subtask("T1");
        state.resolve_subtask("T2");
        state.transition_to(TaskStateValue::Completed).unwrap();
    }

    #[test]
    fn partially_completed_auto_completes() {
        let mut state = TaskState::new("TASK-q");
        state.transition_to(TaskStateValue::Running).unwrap();
        state.add_subtask("T1", TaskStateValue::Completed);
        state
            .transition_to(TaskStateValue::PartiallyCompleted)
            .unwrap();
        let result = state
            .resolve_subtask_and_progress("T1", TaskStateValue::Completed)
            .unwrap();
        assert_eq!(result, Some(TaskStateValue::Completed));
        assert_eq!(state.value(), TaskStateValue::Completed);
    }

    #[test]
    fn terminal_state_rejects_all() {
        for terminal in &[
            TaskStateValue::Completed,
            TaskStateValue::Failed,
            TaskStateValue::Cancelled,
        ] {
            let mut state = TaskState::restore("TASK-t", *terminal, 0);
            let err = state.transition_to(TaskStateValue::Running).unwrap_err();
            assert_eq!(err, TaskStateError::TerminalState);
        }
    }

    #[test]
    fn backward_compat_restore_works() {
        for value in &[
            TaskStateValue::Queued,
            TaskStateValue::Running,
            TaskStateValue::Blocked,
            TaskStateValue::WaitingApproval,
            TaskStateValue::Retrying,
            TaskStateValue::PartiallyCompleted,
        ] {
            let state = TaskState::restore("TASK-old", *value, 5);
            assert_eq!(state.value(), *value);
            assert_eq!(state.agent_calls(), 5);
        }
    }

    #[test]
    fn is_alive_is_terminal_semantics() {
        assert!(TaskStateValue::Queued.is_alive());
        assert!(!TaskStateValue::Completed.is_alive());
        assert!(!TaskStateValue::Failed.is_alive());
        assert!(!TaskStateValue::Cancelled.is_alive());
    }

    #[test]
    fn all_historical_transitions_still_valid() {
        // Ensure the old Created→Running transition is preserved (as Queued→Running).
        let mut state = TaskState::new("TASK-hist");
        state.transition_to(TaskStateValue::Running).unwrap();
        state.transition_to(TaskStateValue::Completed).unwrap();
        assert_eq!(state.value(), TaskStateValue::Completed);

        let mut state = TaskState::new("TASK-hist2");
        state.transition_to(TaskStateValue::Running).unwrap();
        state.transition_to(TaskStateValue::Failed).unwrap();
        assert_eq!(state.value(), TaskStateValue::Failed);

        let mut state = TaskState::new("TASK-hist3");
        state.transition_to(TaskStateValue::Cancelled).unwrap();
        assert_eq!(state.value(), TaskStateValue::Cancelled);
    }
}
