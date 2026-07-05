use std::collections::HashMap;

use sha2::{Digest, Sha256};

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
pub enum CommandInvocation {
    Argv(Vec<String>),
    Shell(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceSet {
    pub read_paths: Vec<String>,
    pub write_paths: Vec<String>,
    pub network: bool,
    pub command: Option<CommandInvocation>,
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
    pub command_hash: Option<String>,
}

#[derive(Debug, Clone)]
struct RegisteredOperation {
    operation: String,
    required_permission: PermissionLevel,
    reasonix_executable: String,
    subcommand: String,
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
        reasonix_executable: impl Into<String>,
        subcommand: impl Into<String>,
    ) -> Self {
        let op = operation.into();
        self.operations.insert(
            op.clone(),
            RegisteredOperation {
                operation: op,
                required_permission,
                reasonix_executable: reasonix_executable.into(),
                subcommand: subcommand.into(),
            },
        );
        self
    }

    pub fn review_diff(
        reasonix_executable: impl Into<String>,
        artifact_policy: ArtifactPolicy,
    ) -> Self {
        Self::new(artifact_policy).register_operation(
            "reasonix.review_diff",
            PermissionLevel::L1DiffReview,
            reasonix_executable,
            "review-diff",
        )
    }

    pub fn evaluate(&self, request: &RuntimeOperationRequest) -> PolicyEvaluationResult {
        let mut reasons = Vec::new();
        let mut computed_command_hash = None;

        let Some(registered) = self.operations.get(&request.operation) else {
            reasons.push(format!("unknown operation {}", request.operation));
            return PolicyEvaluationResult {
                decision: RuntimeDecisionValue::Deny,
                reasons,
                command_hash: None,
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

        match &request.resources.command {
            Some(CommandInvocation::Argv(argv)) => {
                if argv.is_empty() {
                    reasons.push("argv command is empty".to_string());
                } else if argv[0] != registered.reasonix_executable {
                    reasons.push(
                        "argv[0] does not match configured Reasonix executable".to_string(),
                    );
                } else if argv.len() != 2 || argv.get(1).map(String::as_str) != Some(&registered.subcommand) {
                    reasons.push(format!(
                        "argv args do not match {} profile",
                        registered.operation
                    ));
                } else {
                    computed_command_hash = Some(command_hash(argv));
                }
            }
            Some(CommandInvocation::Shell(_)) => {
                reasons.push("shell string commands are rejected".to_string());
            }
            None => {
                reasons.push("command is required".to_string());
            }
        }

        PolicyEvaluationResult {
            decision: if reasons.is_empty() {
                RuntimeDecisionValue::Allow
            } else {
                RuntimeDecisionValue::Deny
            },
            reasons,
            command_hash: computed_command_hash,
        }
    }
}


// ── Composite request/eval types (kept for test constructibility) ──

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
    pub command_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutingMetadata {
    pub project_key_hash: String,
    pub session_key_hash: String,
    pub lane: String,
}
fn command_hash(argv: &[String]) -> String {
    let mut hasher = Sha256::new();
    for arg in argv {
        hasher.update(arg.as_bytes());
        hasher.update([0]);
    }
    format!("sha256:{:x}", hasher.finalize())
}
