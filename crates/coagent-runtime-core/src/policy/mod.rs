use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::sync::{Arc, RwLock};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendBinding {
    Mock,
    ReasonixAcp,
}

/// Approval policy for a tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalPolicy {
    /// No approval needed — tool executes immediately.
    Never,
    /// Approval is required before execution. Runtime gates the call:
    /// task transitions to WaitingApproval, caller must approve before tool runs.
    /// Must specify an approval timeout.
    Required,
}

impl ApprovalPolicy {
    /// Whether this policy gates execution (returns RequireApproval decision).
    pub fn is_gated(&self) -> bool {
        matches!(self, Self::Required)
    }

    /// Return the runtime decision if the approval policy blocks execution.
    pub fn enforce(&self) -> RuntimeDecisionValue {
        match self {
            Self::Never => RuntimeDecisionValue::Allow,
            Self::Required => RuntimeDecisionValue::RequireApproval,
        }
    }
}

impl fmt::Display for ApprovalPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Never => write!(f, "never"),
            Self::Required => write!(f, "required"),
        }
    }
}

/// A tool registered in the runtime, with its full contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDefinition {
    operation: String,
    required_permission: PermissionLevel,
    backend_binding: BackendBinding,
    approval_policy: ApprovalPolicy,
    input_schema: String,
    output_schema: String,
    capabilities: ToolCapabilities,
    /// Whether the tool is currently enabled (can be toggled at runtime).
    enabled: bool,
    /// Version of this tool definition (for dynamic upgrades).
    version: u32,
}

impl ToolDefinition {
    pub fn new(
        operation: impl Into<String>,
        required_permission: PermissionLevel,
        backend_binding: BackendBinding,
        approval_policy: ApprovalPolicy,
        input_schema: impl Into<String>,
        output_schema: impl Into<String>,
        capabilities: ToolCapabilities,
    ) -> Self {
        Self {
            operation: operation.into(),
            required_permission,
            backend_binding,
            approval_policy,
            input_schema: input_schema.into(),
            output_schema: output_schema.into(),
            capabilities,
            enabled: true,
            version: 1,
        }
    }

    pub fn operation(&self) -> &str {
        &self.operation
    }
    pub fn required_permission(&self) -> PermissionLevel {
        self.required_permission
    }
    pub fn backend_binding(&self) -> BackendBinding {
        self.backend_binding
    }
    pub fn approval_policy(&self) -> ApprovalPolicy {
        self.approval_policy
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
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn version(&self) -> u32 {
        self.version
    }
}

/// Thread-safe runtime tool registry.
/// Supports compile-time registration (via `review_diff()`) and runtime
/// dynamic addition/removal/enable/disable.
#[derive(Debug, Clone)]
pub struct ToolRegistry {
    tools: Arc<RwLock<HashMap<String, ToolDefinition>>>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create the default registry with `reasonix.review_diff` registered.
    pub fn review_diff() -> Self {
        Self::new().register(ToolDefinition::new(
            "reasonix.review_diff",
            PermissionLevel::L1DiffReview,
            BackendBinding::ReasonixAcp,
            ApprovalPolicy::Never,
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

    /// Register a tool (compile-time or runtime). Returns self for chaining.
    pub fn register(self, tool: ToolDefinition) -> Self {
        if let Ok(mut tools) = self.tools.write() {
            tools.insert(tool.operation.clone(), tool);
        }
        self
    }

    /// Dynamically add a tool at runtime. Returns the previous definition if any.
    pub fn register_dynamic(&self, tool: ToolDefinition) -> Option<ToolDefinition> {
        self.tools
            .write()
            .ok()?
            .insert(tool.operation.clone(), tool)
    }

    /// Dynamically remove a tool at runtime.
    pub fn unregister(&self, operation: &str) -> Option<ToolDefinition> {
        self.tools.write().ok()?.remove(operation)
    }

    /// Enable a previously disabled tool.
    pub fn enable(&self, operation: &str) -> bool {
        if let Ok(mut tools) = self.tools.write()
            && let Some(tool) = tools.get_mut(operation) {
                tool.enabled = true;
                return true;
            }
        false
    }

    /// Disable a tool (no-op if already disabled). Does not remove from registry.
    pub fn disable(&self, operation: &str) -> bool {
        if let Ok(mut tools) = self.tools.write()
            && let Some(tool) = tools.get_mut(operation) {
                tool.enabled = false;
                return true;
            }
        false
    }

    /// Update a tool definition in-place (version bump). Returns old version.
    pub fn upgrade(&self, mut tool: ToolDefinition) -> Option<u32> {
        if let Ok(mut tools) = self.tools.write()
            && let Some(existing) = tools.get(&tool.operation) {
                tool.version = existing.version + 1;
                let old_version = existing.version;
                tools.insert(tool.operation.clone(), tool);
                return Some(old_version);
            }
        None
    }

    /// Look up a tool. Returns None if not found or disabled.
    pub fn get(&self, operation: &str) -> Option<ToolDefinition> {
        let tools = self.tools.read().ok()?;
        let tool = tools.get(operation)?;
        if !tool.enabled {
            return None;
        }
        Some(tool.clone())
    }

    /// List all enabled tool operations.
    pub fn list_enabled(&self) -> Vec<String> {
        self.tools
            .read()
            .ok()
            .map(|tools| {
                tools
                    .values()
                    .filter(|t| t.enabled)
                    .map(|t| t.operation.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Snapshot current tools for PolicyEngine initialization.
    pub fn snapshot(&self) -> Vec<ToolDefinition> {
        self.tools
            .read()
            .ok()
            .map(|tools| tools.values().filter(|t| t.enabled).cloned().collect())
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone)]
struct RegisteredOperation {
    operation: String,
    required_permission: PermissionLevel,
    approval_policy: ApprovalPolicy,
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
                approval_policy: ApprovalPolicy::Never,
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

    /// Legacy constructor preserved for backward compat.
    pub fn review_diff(artifact_policy: ArtifactPolicy) -> Self {
        Self::new(artifact_policy)
            .register_operation("reasonix.review_diff", PermissionLevel::L1DiffReview)
    }

    /// Build from a full ToolRegistry snapshot.
    pub fn from_tool_registry(
        repo_root: impl AsRef<Path>,
        registry: ToolRegistry,
    ) -> Result<Self, ArtifactPolicyError> {
        let snapshot = registry.snapshot();
        Self::from_tool_snapshot(repo_root, &snapshot)
    }

    /// Build from a snapshot of tool definitions.
    pub fn from_tool_snapshot(
        repo_root: impl AsRef<Path>,
        tools: &[ToolDefinition],
    ) -> Result<Self, ArtifactPolicyError> {
        let mut engine = Self {
            operations: HashMap::new(),
            artifact_policy: ArtifactPolicy::new(&repo_root)?,
        };
        for tool in tools {
            let artifact_policy = ArtifactPolicy::new(&repo_root)?
                .allow_read(tool.capabilities.read_allow.clone())
                .allow_write(tool.capabilities.write_allow.clone())
                .deny(tool.capabilities.deny.clone());
            engine.operations.insert(
                tool.operation.clone(),
                RegisteredOperation {
                    operation: tool.operation.clone(),
                    required_permission: tool.required_permission,
                    approval_policy: tool.approval_policy,
                    capabilities: tool.capabilities.clone(),
                    artifact_policy,
                },
            );
        }
        Ok(engine)
    }

    /// Evaluate a request. Returns Allow/Deny/RequireApproval/etc.
    pub fn evaluate(&self, request: &RuntimeOperationRequest) -> PolicyEvaluationResult {
        let mut reasons = Vec::new();

        let Some(registered) = self.operations.get(&request.operation) else {
            reasons.push(format!("unknown operation {}", request.operation));
            return PolicyEvaluationResult {
                decision: RuntimeDecisionValue::Deny,
                reasons,
            };
        };

        // Approval gate: if the tool requires approval, emit RequireApproval.
        let approval_decision = registered.approval_policy.enforce();
        if approval_decision == RuntimeDecisionValue::RequireApproval {
            reasons.push(format!("approval required for {}", request.operation));
            // Note: permission and path checks still run, but the top result is RequireApproval.
        }

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

        // Priority: Deny beats RequireApproval.
        let has_deny_reasons = reasons
            .iter()
            .any(|r| r.contains("denied") || r.contains("does not match") || r.contains("unknown"));
        PolicyEvaluationResult {
            decision: if has_deny_reasons {
                RuntimeDecisionValue::Deny
            } else if approval_decision == RuntimeDecisionValue::RequireApproval {
                RuntimeDecisionValue::RequireApproval
            } else {
                RuntimeDecisionValue::Allow
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_register_and_unregister() {
        let registry = ToolRegistry::new();
        assert!(registry.get("agent.test").is_none());

        registry.register_dynamic(ToolDefinition::new(
            "agent.test",
            PermissionLevel::L0Readonly,
            BackendBinding::Mock,
            ApprovalPolicy::Never,
            "test_in",
            "test_out",
            ToolCapabilities {
                read_allow: vec![],
                write_allow: vec![],
                deny: vec![],
                network: false,
            },
        ));
        assert!(registry.get("agent.test").is_some());

        registry.unregister("agent.test");
        assert!(registry.get("agent.test").is_none());
    }

    #[test]
    fn disable_tool_hides_it_from_get() {
        let registry = ToolRegistry::new();
        registry.register_dynamic(ToolDefinition::new(
            "agent.toggle",
            PermissionLevel::L0Readonly,
            BackendBinding::Mock,
            ApprovalPolicy::Never,
            "in",
            "out",
            ToolCapabilities {
                read_allow: vec![],
                write_allow: vec![],
                deny: vec![],
                network: false,
            },
        ));
        assert!(registry.get("agent.toggle").is_some());
        assert!(registry.disable("agent.toggle"));
        assert!(registry.get("agent.toggle").is_none());
        assert!(registry.enable("agent.toggle"));
        assert!(registry.get("agent.toggle").is_some());
    }

    #[test]
    fn approval_policy_enforce_yields_require_approval() {
        let policy = ApprovalPolicy::Required;
        assert!(policy.is_gated());
        assert_eq!(policy.enforce(), RuntimeDecisionValue::RequireApproval);

        let policy = ApprovalPolicy::Never;
        assert!(!policy.is_gated());
        assert_eq!(policy.enforce(), RuntimeDecisionValue::Allow);
    }

    #[test]
    fn approval_gated_tool_returns_require_approval_in_policy_eval() {
        let repo = std::env::current_dir().unwrap();
        let tools = vec![ToolDefinition::new(
            "agent.approval_needed",
            PermissionLevel::L1DiffReview,
            BackendBinding::Mock,
            ApprovalPolicy::Required,
            "in",
            "out",
            ToolCapabilities {
                read_allow: vec![".agent/diffs/**".to_string()],
                write_allow: vec![".agent/results/**".to_string()],
                deny: vec![],
                network: false,
            },
        )];
        let engine = PolicyEngine::from_tool_snapshot(repo, &tools).unwrap();

        let result = engine.evaluate(&RuntimeOperationRequest {
            task_id: "TASK-approval".to_string(),
            request_id: Some("REQ-approval".to_string()),
            operation: "agent.approval_needed".to_string(),
            permission_level: PermissionLevel::L1DiffReview,
            resources: ResourceSet {
                read_paths: vec![".agent/diffs/test.diff".to_string()],
                write_paths: vec![".agent/results/out.json".to_string()],
                network: false,
            },
        });
        assert_eq!(result.decision, RuntimeDecisionValue::RequireApproval);
        assert!(
            result
                .reasons
                .iter()
                .any(|r| r.contains("approval required"))
        );
    }

    #[test]
    fn list_enabled_filters_disabled_tools() {
        let registry = ToolRegistry::new();
        registry.register_dynamic(ToolDefinition::new(
            "agent.a",
            PermissionLevel::L0Readonly,
            BackendBinding::Mock,
            ApprovalPolicy::Never,
            "in",
            "out",
            ToolCapabilities::default(),
        ));
        registry.register_dynamic(ToolDefinition::new(
            "agent.b",
            PermissionLevel::L0Readonly,
            BackendBinding::Mock,
            ApprovalPolicy::Never,
            "in",
            "out",
            ToolCapabilities::default(),
        ));
        registry.disable("agent.b");
        let enabled = registry.list_enabled();
        assert_eq!(enabled, vec!["agent.a"]);
    }

    #[test]
    fn registry_upgrade_bumps_version() {
        let registry = ToolRegistry::new();
        registry.register_dynamic(ToolDefinition::new(
            "agent.v",
            PermissionLevel::L0Readonly,
            BackendBinding::Mock,
            ApprovalPolicy::Never,
            "in_v1",
            "out_v1",
            ToolCapabilities::default(),
        ));
        let old = registry.get("agent.v").unwrap();
        assert_eq!(old.version(), 1);

        let upgraded = ToolDefinition::new(
            "agent.v",
            PermissionLevel::L0Readonly,
            BackendBinding::Mock,
            ApprovalPolicy::Never,
            "in_v2",
            "out_v2",
            ToolCapabilities::default(),
        );
        let old_version = registry.upgrade(upgraded).unwrap();
        assert_eq!(old_version, 1);

        let new = registry.get("agent.v").unwrap();
        assert_eq!(new.version(), 2);
        assert_eq!(new.input_schema(), "in_v2");
    }

    impl Default for ToolCapabilities {
        fn default() -> Self {
            Self {
                read_allow: vec![],
                write_allow: vec![],
                deny: vec![],
                network: false,
            }
        }
    }
}
