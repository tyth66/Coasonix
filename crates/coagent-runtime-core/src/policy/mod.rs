use std::collections::HashMap;

use std::path::Path;

use crate::artifact::{ArtifactPolicy, ArtifactPolicyError, ResourceAccess};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCapabilities {
    pub read_allow: Vec<String>,
    pub write_allow: Vec<String>,
    pub deny: Vec<String>,
    pub network: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDefinition {
    operation: String,
    required_permission: PermissionLevel,
    input_schema: String,
    output_schema: String,
    capabilities: ToolCapabilities,
}

impl ToolDefinition {
    pub fn new(
        operation: impl Into<String>,
        required_permission: PermissionLevel,
        input_schema: impl Into<String>,
        output_schema: impl Into<String>,
        capabilities: ToolCapabilities,
    ) -> Self {
        Self {
            operation: operation.into(),
            required_permission,
            input_schema: input_schema.into(),
            output_schema: output_schema.into(),
            capabilities,
        }
    }

    pub fn operation(&self) -> &str {
        &self.operation
    }

    pub fn required_permission(&self) -> PermissionLevel {
        self.required_permission
    }

    pub fn input_schema(&self) -> &str {
        &self.input_schema
    }

    pub fn output_schema(&self) -> &str {
        &self.output_schema
    }

    pub fn capabilities(&self) -> &ToolCapabilities {
        &self.capabilities
    }
}

#[derive(Debug, Clone, Default)]
pub struct ToolRegistry {
    tools: HashMap<String, ToolDefinition>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn review_diff() -> Self {
        Self::new().register(ToolDefinition::new(
            "reasonix.review_diff",
            PermissionLevel::L1DiffReview,
            "review_diff_input_v1",
            "coagent_review_wrapper_v1",
            ToolCapabilities {
                read_allow: vec![
                    ".agent/context/**".to_string(),
                    ".agent/diffs/**".to_string(),
                    ".agent/logs/**".to_string(),
                    "docs/**".to_string(),
                    "crates/**".to_string(),
                    "packages/**".to_string(),
                    "schemas/**".to_string(),
                ],
                write_allow: vec![
                    ".agent/results/**".to_string(),
                    ".agent/logs/**".to_string(),
                ],
                deny: vec![".agent/secrets/**".to_string(), ".git/**".to_string()],
                network: false,
            },
        ))
    }

    pub fn register(mut self, tool: ToolDefinition) -> Self {
        self.tools.insert(tool.operation.clone(), tool);
        self
    }

    pub fn get(&self, operation: &str) -> Option<&ToolDefinition> {
        self.tools.get(operation)
    }

    fn into_tools(self) -> impl Iterator<Item = ToolDefinition> {
        self.tools.into_values()
    }
}

#[derive(Debug, Clone)]
struct RegisteredOperation {
    operation: String,
    required_permission: PermissionLevel,
    capabilities: ToolCapabilities,
    artifact_policy: ArtifactPolicy,
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
                capabilities: ToolCapabilities {
                    read_allow: Vec::new(),
                    write_allow: Vec::new(),
                    deny: Vec::new(),
                    network: false,
                },
                artifact_policy: self.artifact_policy.clone(),
            },
        );
        self
    }

    pub fn review_diff(artifact_policy: ArtifactPolicy) -> Self {
        Self::new(artifact_policy)
            .register_operation("reasonix.review_diff", PermissionLevel::L1DiffReview)
    }

    pub fn from_tool_registry(
        repo_root: impl AsRef<Path>,
        registry: ToolRegistry,
    ) -> Result<Self, ArtifactPolicyError> {
        let mut engine = Self {
            operations: HashMap::new(),
            artifact_policy: ArtifactPolicy::new(&repo_root)?,
        };

        for tool in registry.into_tools() {
            let artifact_policy = ArtifactPolicy::new(&repo_root)?
                .allow_read(tool.capabilities.read_allow.clone())
                .allow_write(tool.capabilities.write_allow.clone())
                .deny(tool.capabilities.deny.clone());
            engine.operations.insert(
                tool.operation.clone(),
                RegisteredOperation {
                    operation: tool.operation,
                    required_permission: tool.required_permission,
                    capabilities: tool.capabilities,
                    artifact_policy,
                },
            );
        }

        Ok(engine)
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

        if request.resources.network && !registered.capabilities.network {
            reasons.push("network access is denied by default".to_string());
        }

        for path in &request.resources.read_paths {
            if let Err(error) = registered
                .artifact_policy
                .authorize(ResourceAccess::Read, path)
            {
                reasons.push(format!("read path denied: {error:?}"));
            }
        }

        for path in &request.resources.write_paths {
            if let Err(error) = registered
                .artifact_policy
                .authorize(ResourceAccess::Write, path)
            {
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
