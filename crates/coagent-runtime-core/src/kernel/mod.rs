use std::path::PathBuf;

use serde_json::{Value, json};
use thiserror::Error;

use crate::{
    artifact::ArtifactPolicyError,
    policy::{PolicyEngine, RuntimeOperationRequest, ToolRegistry},
    state::{TaskState, TaskStateValue},
    storage::{AuditEventInput, AuditEventRecord, RuntimeDecisionRecord, RuntimeStore, StoreError},
};

pub use crate::policy::RuntimeDecisionValue;

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub repo_root: PathBuf,
}

#[derive(Debug)]
pub struct RuntimeKernel {
    store: RuntimeStore,
    policy_engine: PolicyEngine,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineResults {
    pub state: RuntimeDecisionValue,
    pub policy: RuntimeDecisionValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeDecision {
    pub task_id: String,
    pub request_id: Option<String>,
    pub operation: String,
    pub decision: RuntimeDecisionValue,
    pub engine_results: EngineResults,
    pub reasons: Vec<String>,
    pub audit_event_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvent {
    pub task_id: String,
    pub event_type: String,
    pub summary: String,
    pub payload_json: String,
}

pub type AuditWriteResult = AuditEventRecord;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("artifact policy error: {0:?}")]
    Artifact(ArtifactPolicyError),
    #[error("store error: {0}")]
    Store(#[from] StoreError),
}

impl RuntimeKernel {
    pub fn initialize(config: RuntimeConfig) -> Result<Self, RuntimeError> {
        Self::initialize_with_tool_registry(config, ToolRegistry::review_diff())
    }

    pub fn initialize_with_tool_registry(
        config: RuntimeConfig,
        tool_registry: ToolRegistry,
    ) -> Result<Self, RuntimeError> {
        let store = RuntimeStore::initialize(&config.repo_root)?;
        let policy_engine = PolicyEngine::from_tool_registry(&config.repo_root, tool_registry)
            .map_err(RuntimeError::Artifact)?;

        Ok(Self {
            store,
            policy_engine,
        })
    }

    pub fn evaluate_operation(&mut self, request: RuntimeOperationRequest) -> RuntimeDecision {
        let (state_decision, state_reasons, current_state) = self.evaluate_state(&request.task_id);
        let policy_result = self.policy_engine.evaluate(&request);
        let engine_results = EngineResults {
            state: state_decision,
            policy: policy_result.decision,
        };

        let mut reasons = Vec::new();
        reasons.extend(state_reasons);
        reasons.extend(policy_result.reasons);

        let mut decision = RuntimeDecision {
            task_id: request.task_id.clone(),
            request_id: request.request_id.clone(),
            operation: request.operation.clone(),
            decision: Self::merge_decisions(engine_results.clone()),
            engine_results,
            reasons,
            audit_event_id: None,
        };

        let step_id = match self.start_runtime_step(&request) {
            Ok(step_id) => step_id,
            Err(error) => {
                decision.decision = RuntimeDecisionValue::FatalError;
                decision.reasons.push(format!("storage error: {error}"));
                return decision;
            }
        };

        match self.persist_runtime_decision(&decision) {
            Ok(audit) => {
                decision.audit_event_id = Some(audit.id);
                if let Err(error) = self.write_policy_evaluated_event(&decision, step_id) {
                    decision.decision = RuntimeDecisionValue::FatalError;
                    decision.reasons.push(format!("storage error: {error}"));
                    return decision;
                }
                if decision.decision == RuntimeDecisionValue::Allow {
                    self.persist_running_state(&request.task_id, current_state);
                } else if let Err(error) = self.close_runtime_step(
                    &request.task_id,
                    request.request_id.as_deref(),
                    &request.operation,
                    runtime_decision_to_str(decision.decision),
                ) {
                    decision.decision = RuntimeDecisionValue::FatalError;
                    decision.reasons.push(format!("storage error: {error}"));
                    return decision;
                }
            }
            Err(error) => {
                decision.decision = RuntimeDecisionValue::FatalError;
                decision.reasons.push(format!("storage error: {error}"));
            }
        }

        decision
    }

    pub fn write_audit(&mut self, event: AuditEvent) -> Result<AuditWriteResult, RuntimeError> {
        Ok(self.store.write_audit_event(&AuditEventInput {
            task_id: event.task_id,
            event_type: event.event_type,
            summary: event.summary,
            payload_json: event.payload_json,
        })?)
    }

    /// Transition task to Completed and write audit.
    pub fn complete_operation(
        &mut self,
        task_id: &str,
        request_id: Option<&str>,
        operation: &str,
    ) -> Result<TaskStateValue, RuntimeError> {
        let mut state = self.load_or_create_task_state(task_id);
        state
            .transition_to(TaskStateValue::Completed)
            .map_err(|e| RuntimeError::Store(StoreError::InvalidTaskState(format!("{e:?}"))))?;

        let audit = AuditEventInput {
            task_id: task_id.to_string(),
            event_type: "task_completed".to_string(),
            summary: format!("Task {task_id} completed for {operation}"),
            payload_json: json!({
                "task_id": task_id,
                "request_id": request_id,
                "operation": operation
            })
            .to_string(),
        };
        self.store
            .transition_state_with_audit(task_id, TaskStateValue::Completed, &audit)?;
        self.close_runtime_step(task_id, request_id, operation, "completed")?;
        Ok(TaskStateValue::Completed)
    }

    /// Transition task to Failed, write audit with error info.
    pub fn fail_operation(
        &mut self,
        task_id: &str,
        request_id: Option<&str>,
        operation: &str,
        error_code: &str,
        error_message: &str,
    ) -> Result<TaskStateValue, RuntimeError> {
        let mut state = self.load_or_create_task_state(task_id);
        state
            .transition_to(TaskStateValue::Failed)
            .map_err(|e| RuntimeError::Store(StoreError::InvalidTaskState(format!("{e:?}"))))?;

        let audit = AuditEventInput {
            task_id: task_id.to_string(),
            event_type: "task_failed".to_string(),
            summary: format!("Task {task_id} failed ({error_code}): {error_message}"),
            payload_json: json!({
                "task_id": task_id,
                "request_id": request_id,
                "operation": operation,
                "error_code": error_code,
                "error_message": error_message
            })
            .to_string(),
        };
        self.store
            .transition_state_with_audit(task_id, TaskStateValue::Failed, &audit)?;
        self.close_runtime_step(task_id, request_id, operation, "failed")?;
        Ok(TaskStateValue::Failed)
    }

    fn load_or_create_task_state(&self, task_id: &str) -> TaskState {
        self.store
            .load_task_state(task_id)
            .unwrap_or_else(|_| TaskState::new(task_id))
    }
    pub fn merge_decisions(results: EngineResults) -> RuntimeDecisionValue {
        if results.policy == RuntimeDecisionValue::Deny {
            return RuntimeDecisionValue::Deny;
        }
        if [results.state, results.policy].contains(&RuntimeDecisionValue::FatalError) {
            return RuntimeDecisionValue::FatalError;
        }
        if [results.state, results.policy].contains(&RuntimeDecisionValue::Deny) {
            return RuntimeDecisionValue::Deny;
        }
        if [results.state, results.policy].contains(&RuntimeDecisionValue::RequireApproval) {
            return RuntimeDecisionValue::RequireApproval;
        }
        if [results.state, results.policy].contains(&RuntimeDecisionValue::RetryableError) {
            return RuntimeDecisionValue::RetryableError;
        }
        RuntimeDecisionValue::Allow
    }

    fn evaluate_state(&self, task_id: &str) -> (RuntimeDecisionValue, Vec<String>, TaskState) {
        match self.store.load_task_state(task_id) {
            Ok(state)
                if matches!(
                    state.value(),
                    TaskStateValue::Completed | TaskStateValue::Failed | TaskStateValue::Cancelled
                ) =>
            {
                (
                    RuntimeDecisionValue::Deny,
                    vec![format!(
                        "task {task_id} is in terminal state {:?}",
                        state.value()
                    )],
                    state,
                )
            }
            Ok(state) => (RuntimeDecisionValue::Allow, Vec::new(), state),
            Err(StoreError::TaskStateNotFound(_)) => (
                RuntimeDecisionValue::Allow,
                Vec::new(),
                TaskState::new(task_id),
            ),
            Err(error) => (
                RuntimeDecisionValue::FatalError,
                vec![format!("state load failed: {error}")],
                TaskState::new(task_id),
            ),
        }
    }

    fn persist_runtime_decision(
        &self,
        decision: &RuntimeDecision,
    ) -> Result<AuditEventRecord, StoreError> {
        let record = RuntimeDecisionRecord {
            task_id: decision.task_id.clone(),
            request_id: decision.request_id.clone(),
            operation: decision.operation.clone(),
            decision: decision.decision,
            reasons: decision.reasons.clone(),
            command_hash: None,
        };
        let audit = AuditEventInput {
            task_id: decision.task_id.clone(),
            event_type: format!("runtime_decision_{:?}", decision.decision).to_lowercase(),
            summary: format!(
                "Runtime decision {:?} for {}",
                decision.decision, decision.operation
            ),
            payload_json: decision.to_payload().to_string(),
        };
        self.store
            .commit_runtime_decision_with_audit(&record, &audit)
    }

    fn start_runtime_step(&self, request: &RuntimeOperationRequest) -> Result<i64, StoreError> {
        let step = self.store.start_runtime_step(
            &request.task_id,
            request.request_id.as_deref(),
            &request.operation,
        )?;
        self.store.write_runtime_event(
            &request.task_id,
            request.request_id.as_deref(),
            Some(step.id),
            "step_started",
            &json!({
                "task_id": request.task_id,
                "request_id": request.request_id,
                "operation": request.operation
            })
            .to_string(),
        )?;
        Ok(step.id)
    }

    fn write_policy_evaluated_event(
        &self,
        decision: &RuntimeDecision,
        step_id: i64,
    ) -> Result<(), StoreError> {
        self.store.write_runtime_event(
            &decision.task_id,
            decision.request_id.as_deref(),
            Some(step_id),
            "policy_evaluated",
            &json!({
                "task_id": decision.task_id,
                "request_id": decision.request_id,
                "operation": decision.operation,
                "decision": runtime_decision_to_str(decision.decision),
                "state_decision": runtime_decision_to_str(decision.engine_results.state),
                "policy_decision": runtime_decision_to_str(decision.engine_results.policy),
                "reasons": decision.reasons
            })
            .to_string(),
        )?;
        Ok(())
    }

    fn close_runtime_step(
        &self,
        task_id: &str,
        request_id: Option<&str>,
        operation: &str,
        state: &str,
    ) -> Result<(), StoreError> {
        let Some(step) = self
            .store
            .runtime_step_for_request(task_id, request_id, operation)?
        else {
            return Ok(());
        };
        self.store.finish_runtime_step(step.id, state)?;
        self.store.write_runtime_event(
            task_id,
            request_id,
            Some(step.id),
            "lifecycle_closed",
            &json!({
                "task_id": task_id,
                "request_id": request_id,
                "operation": operation,
                "state": state
            })
            .to_string(),
        )?;
        Ok(())
    }

    fn persist_running_state(&self, task_id: &str, current_state: TaskState) {
        if current_state.value() == TaskStateValue::Queued {
            let mut next = current_state;
            if next.transition_to(TaskStateValue::Running).is_ok() {
                let _ = self.store.upsert_task_state(&next);
            }
        } else {
            let _ = self.store.upsert_task_state(&TaskState::restore(
                task_id,
                current_state.value(),
                current_state.agent_calls(),
            ));
        }
    }
}

impl RuntimeDecision {
    pub fn to_payload(&self) -> Value {
        let mut payload = json!({
            "schema_version": "runtime_decision_v1",
            "task_id": &self.task_id,
            "operation": &self.operation,
            "decision": runtime_decision_to_str(self.decision),
            "engine_results": {
                "state": runtime_decision_to_str(self.engine_results.state),
                "policy": runtime_decision_to_str(self.engine_results.policy)
            },
            "reasons": &self.reasons,
        });
        if let Some(request_id) = &self.request_id {
            payload["request_id"] = json!(request_id);
        }
        if let Some(audit_event_id) = self.audit_event_id {
            payload["audit_event_id"] = json!(audit_event_id.to_string());
        }
        payload
    }
}

pub fn engine_results(state: RuntimeDecisionValue, policy: RuntimeDecisionValue) -> EngineResults {
    EngineResults { state, policy }
}

fn runtime_decision_to_str(value: RuntimeDecisionValue) -> &'static str {
    match value {
        RuntimeDecisionValue::Allow => "allow",
        RuntimeDecisionValue::Deny => "deny",
        RuntimeDecisionValue::RequireApproval => "require_approval",
        RuntimeDecisionValue::RetryableError => "retryable_error",
        RuntimeDecisionValue::FatalError => "fatal_error",
    }
}
