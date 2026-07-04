use std::path::PathBuf;

use serde_json::{Value, json};
use thiserror::Error;

use crate::{
    artifact::{ArtifactPolicy, ArtifactPolicyError},
    policy::{CommandInvocation, PermissionLevel, PolicyEngine, RuntimeOperationRequest},
    schema::{SchemaRegistry, SchemaValidationResult},
    state::{TaskState, TaskStateValue},
    storage::SchemaValidationRecord,
    storage::{AuditEventInput, AuditEventRecord, RuntimeDecisionRecord, RuntimeStore, StoreError},
};

pub use crate::policy::RuntimeDecisionValue;

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub repo_root: PathBuf,
    pub schema_path: PathBuf,
    pub reasonix_executable: String,
}

#[derive(Debug)]
pub struct RuntimeKernel {
    schema_registry: SchemaRegistry,
    store: RuntimeStore,
    policy_engine: PolicyEngine,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineResults {
    pub schema: RuntimeDecisionValue,
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

#[derive(Debug, Clone)]
pub struct SchemaValidationRequest {
    pub task_id: String,
    pub request_id: Option<String>,
    pub expected_schema: String,
    pub payload: Value,
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
    #[error("schema registry error: {0}")]
    Schema(#[from] crate::schema::SchemaError),
    #[error("artifact policy error: {0:?}")]
    Artifact(ArtifactPolicyError),
    #[error("store error: {0}")]
    Store(#[from] StoreError),
}

impl RuntimeKernel {
    pub fn initialize(config: RuntimeConfig) -> Result<Self, RuntimeError> {
        let schema_registry = SchemaRegistry::load_from_path(config.schema_path)?;
        let store = RuntimeStore::initialize(&config.repo_root)?;
        let artifact_policy = ArtifactPolicy::new(&config.repo_root)
            .map_err(RuntimeError::Artifact)?
            .allow_read([
                ".agent/context/**",
                ".agent/diffs/**",
                ".agent/logs/**",
                "docs/**",
                "crates/**",
                "packages/**",
                "schemas/**",
            ])
            .allow_write([".agent/results/**", ".agent/logs/**"])
            .deny([".agent/secrets/**", ".git/**"]);
        let policy_engine = PolicyEngine::review_diff(config.reasonix_executable, artifact_policy);

        Ok(Self {
            schema_registry,
            store,
            policy_engine,
        })
    }

    pub fn validate_schema(&self, request: SchemaValidationRequest) -> SchemaValidationResult {
        let result = self
            .schema_registry
            .validate(&request.expected_schema, &request.payload);
        self.persist_schema_validation(&request, &result);
        result
    }

    pub fn evaluate_operation(&mut self, request: RuntimeOperationRequest) -> RuntimeDecision {
        let schema_result = self.schema_registry.validate(
            "runtime_operation_request_v1",
            &operation_request_payload(&request),
        );
        let schema_decision = if schema_result.valid {
            RuntimeDecisionValue::Allow
        } else {
            RuntimeDecisionValue::Deny
        };

        let (state_decision, state_reasons, current_state) = self.evaluate_state(&request.task_id);
        let policy_result = self.policy_engine.evaluate(&request);
        let engine_results = EngineResults {
            schema: schema_decision,
            state: state_decision,
            policy: policy_result.decision,
        };

        let mut reasons = Vec::new();
        if !schema_result.valid {
            reasons.extend(
                schema_result
                    .errors
                    .iter()
                    .map(|error| format!("schema {}: {}", error.path, error.message)),
            );
        }
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

        match self.persist_runtime_decision(&decision, policy_result.command_hash) {
            Ok(audit) => {
                decision.audit_event_id = Some(audit.id);
                if decision.decision == RuntimeDecisionValue::Allow {
                    self.persist_running_state(&request.task_id, current_state);
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

    pub fn merge_decisions(results: EngineResults) -> RuntimeDecisionValue {
        if results.policy == RuntimeDecisionValue::Deny {
            return RuntimeDecisionValue::Deny;
        }
        if [results.schema, results.state, results.policy]
            .contains(&RuntimeDecisionValue::FatalError)
        {
            return RuntimeDecisionValue::FatalError;
        }
        if [results.schema, results.state, results.policy].contains(&RuntimeDecisionValue::Deny) {
            return RuntimeDecisionValue::Deny;
        }
        if [results.schema, results.state, results.policy]
            .contains(&RuntimeDecisionValue::RequireApproval)
        {
            return RuntimeDecisionValue::RequireApproval;
        }
        if [results.schema, results.state, results.policy]
            .contains(&RuntimeDecisionValue::RetryableError)
        {
            return RuntimeDecisionValue::RetryableError;
        }
        RuntimeDecisionValue::Allow
    }

    fn evaluate_state(&self, task_id: &str) -> (RuntimeDecisionValue, Vec<String>, TaskState) {
        match self.store.load_task_state(task_id) {
            Ok(state)
                if matches!(
                    state.value(),
                    TaskStateValue::Completed | TaskStateValue::Failed
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
        command_hash: Option<String>,
    ) -> Result<AuditEventRecord, StoreError> {
        let record = RuntimeDecisionRecord {
            task_id: decision.task_id.clone(),
            request_id: decision.request_id.clone(),
            operation: decision.operation.clone(),
            decision: decision.decision,
            reasons: decision.reasons.clone(),
            command_hash,
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

    fn persist_schema_validation(
        &self,
        request: &SchemaValidationRequest,
        result: &SchemaValidationResult,
    ) {
        let payload = result
            .to_payload(&request.task_id, request.request_id.as_deref())
            .to_string();
        let errors_json = Value::Array(
            result
                .errors
                .iter()
                .map(|error| {
                    json!({
                        "path": error.path,
                        "message": error.message
                    })
                })
                .collect(),
        )
        .to_string();
        let record = SchemaValidationRecord {
            task_id: request.task_id.clone(),
            request_id: request.request_id.clone(),
            expected_schema: request.expected_schema.clone(),
            valid: result.valid,
            errors_json,
        };
        let audit = AuditEventInput {
            task_id: request.task_id.clone(),
            event_type: if result.valid {
                "schema_validation_passed".to_string()
            } else {
                "schema_validation_failed".to_string()
            },
            summary: format!("Schema validation for {}", request.expected_schema),
            payload_json: payload,
        };
        let _ = self
            .store
            .commit_schema_validation_with_audit(&record, &audit);
    }

    fn persist_running_state(&self, task_id: &str, current_state: TaskState) {
        if current_state.value() == TaskStateValue::Created {
            let mut next = current_state;
            if next.transition_to(TaskStateValue::Running).is_ok() {
                let _ = self.store.upsert_task_state(&next);
            }
        } else {
            let _ = self.store.upsert_task_state(&TaskState::restore(
                task_id,
                current_state.value(),
                current_state.reasonix_calls(),
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
                "schema": runtime_decision_to_str(self.engine_results.schema),
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

pub fn engine_results(
    schema: RuntimeDecisionValue,
    state: RuntimeDecisionValue,
    policy: RuntimeDecisionValue,
) -> EngineResults {
    EngineResults {
        schema,
        state,
        policy,
    }
}

fn operation_request_payload(request: &RuntimeOperationRequest) -> Value {
    let paths = request
        .resources
        .read_paths
        .iter()
        .chain(request.resources.write_paths.iter())
        .collect::<Vec<_>>();
    let mut resources = json!({ "paths": paths });
    if let Some(CommandInvocation::Argv(argv)) = &request.resources.command {
        resources["command"] = json!(argv);
    }

    let mut payload = json!({
        "schema_version": "runtime_operation_request_v1",
        "task_id": &request.task_id,
        "operation": runtime_operation_name(&request.operation),
        "permission_level": permission_level_to_str(request.permission_level),
        "resources": resources,
    });
    if let Some(request_id) = &request.request_id {
        payload["request_id"] = json!(request_id);
    }
    payload
}

fn runtime_operation_name(operation: &str) -> &str {
    match operation {
        "reasonix.review_diff" => "call_reasonix_tool",
        other => other,
    }
}

fn permission_level_to_str(value: PermissionLevel) -> &'static str {
    match value {
        PermissionLevel::L0Readonly => "L0_READONLY",
        PermissionLevel::L1DiffReview => "L1_DIFF_REVIEW",
        PermissionLevel::L2PatchOnly => "L2_PATCH_ONLY",
        PermissionLevel::L3IsolatedWorktree => "L3_ISOLATED_WORKTREE",
    }
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
