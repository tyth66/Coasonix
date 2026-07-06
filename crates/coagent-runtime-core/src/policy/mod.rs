use std::collections::HashMap;

use crate::artifact::{ArtifactPolicy, ResourceAccess};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionLevel {
    L0Readonly,
    L1DiffReview,
    L2PatchOnly,
    L3IsolatedWorktree,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeDecisionValue {
    Allow,
    Deny,
    RequireApproval,
    RetryableError,
    FatalError,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceSet {
    pub read_paths: Vec<String>,
    pub write_paths: Vec<String>,
    pub network: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeOperationRequest {
    pub task_id: String,
    pub request_id: Option<String>,
    pub operation: String,
    pub permission_level: PermissionLevel,
    pub resources: ResourceSet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyEvaluationResult {
    pub decision: RuntimeDecisionValue,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone)]
struct RegisteredOperation {
    operation: String,
    required_permission: PermissionLevel,
}

#[derive(Debug, Clone)]
pub struct PolicyEngine {
    operations: HashMap<String, RegisteredOperation>,
    artifact_policy: ArtifactPolicy,
}

impl PolicyEngine {
    pub fn new(artifact_policy: ArtifactPolicy) -> Self {
        Self {
            operations: HashMap::new(),
            artifact_policy,
        }
    }

    pub fn register_operation(
        mut self,
        operation: impl Into<String>,
        required_permission: PermissionLevel,
    ) -> Self {
        let op = operation.into();
        self.operations.insert(
            op.clone(),
            RegisteredOperation {
                operation: op,
                required_permission,
            },
        );
        self
    }

    pub fn review_diff(artifact_policy: ArtifactPolicy) -> Self {
        Self::new(artifact_policy)
            .register_operation("reasonix.review_diff", PermissionLevel::L1DiffReview)
    }

    pub fn evaluate(&self, request: &RuntimeOperationRequest) -> PolicyEvaluationResult {
        let mut reasons = Vec::new();

        let Some(registered) = self.operations.get(&request.operation) else {
            reasons.push(format!("unknown operation {}", request.operation));
            return PolicyEvaluationResult {
                decision: RuntimeDecisionValue::Deny,
                reasons,
            };
        };

        if request.permission_level != registered.required_permission {
            reasons.push(format!(
                "permission level does not match {}",
                registered.operation
            ));
        }

        if request.resources.network {
            reasons.push("network access is denied by default".to_string());
        }

        for path in &request.resources.read_paths {
            if let Err(error) = self.artifact_policy.authorize(ResourceAccess::Read, path) {
                reasons.push(format!("read path denied: {error:?}"));
            }
        }

        for path in &request.resources.write_paths {
            if let Err(error) = self.artifact_policy.authorize(ResourceAccess::Write, path) {
                reasons.push(format!("write path denied: {error:?}"));
            }
        }

        PolicyEvaluationResult {
            decision: if reasons.is_empty() {
                RuntimeDecisionValue::Allow
            } else {
                RuntimeDecisionValue::Deny
            },
            reasons,
        }
    }
}

// ---- Legacy test constructors kept for external test compatibility ----

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyEvaluationRequest {
    pub operation: String,
    pub permission_level: PermissionLevel,
    pub resources: ResourceSet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeDecision {
    pub task_id: String,
    pub request_id: Option<String>,
    pub operation: String,
    pub decision: RuntimeDecisionValue,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutingMetadata {
    pub project_key_hash: String,
    pub session_key_hash: String,
    pub lane: String,
}
