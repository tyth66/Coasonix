#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStateValue {
    Created,
    Running,
    Completed,
    Failed,
}

impl TaskStateValue {
    fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskState {
    task_id: String,
    value: TaskStateValue,
    agent_calls: u32,
    required_verification_gaps: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStateError {
    IllegalTransition,
    TerminalState,
    RequiredVerificationGaps,
}

impl TaskState {
    pub fn new(task_id: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            value: TaskStateValue::Created,
            agent_calls: 0,
            required_verification_gaps: Vec::new(),
        }
    }

    pub fn restore(task_id: impl Into<String>, value: TaskStateValue, agent_calls: u32) -> Self {
        Self {
            task_id: task_id.into(),
            value,
            agent_calls,
            required_verification_gaps: Vec::new(),
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

    pub fn add_required_verification_gap(&mut self, gap: impl Into<String>) {
        self.required_verification_gaps.push(gap.into());
    }

    pub fn transition_to(&mut self, next: TaskStateValue) -> Result<(), TaskStateError> {
        if self.value.is_terminal() {
            return Err(TaskStateError::TerminalState);
        }

        if next == TaskStateValue::Completed && !self.required_verification_gaps.is_empty() {
            return Err(TaskStateError::RequiredVerificationGaps);
        }

        if !is_legal_transition(self.value, next) {
            return Err(TaskStateError::IllegalTransition);
        }

        self.value = next;
        Ok(())
    }

    pub fn note_adapter_observed_agent_attempt(&mut self) {}

    pub fn note_runtime_owned_agent_call(&mut self) {
        self.agent_calls += 1;
    }
}

fn is_legal_transition(current: TaskStateValue, next: TaskStateValue) -> bool {
    matches!(
        (current, next),
        (TaskStateValue::Created, TaskStateValue::Running)
            | (TaskStateValue::Running, TaskStateValue::Completed)
            | (TaskStateValue::Running, TaskStateValue::Failed)
    )
}

