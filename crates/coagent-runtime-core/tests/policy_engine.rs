use std::path::PathBuf;

use coagent_runtime_core::policy::{
    ApprovalPolicy, BackendBinding, PermissionLevel, PolicyEngine, PolicyEvaluationRequest,
    ResourceSet, RoutingMetadata, RuntimeDecision, RuntimeDecisionValue, RuntimeOperationRequest,
    ToolCapabilities, ToolDefinition, ToolRegistry,
};

fn review_diff_engine(repo_root: impl Into<PathBuf>) -> PolicyEngine {
    PolicyEngine::from_tool_registry(repo_root.into(), ToolRegistry::review_diff())
        .expect("policy engine")
}

fn review_diff_request() -> RuntimeOperationRequest {
    RuntimeOperationRequest {
        task_id: "TASK-policy".to_string(),
        request_id: Some("REQ-policy".to_string()),
        operation: "reasonix.review_diff".to_string(),
        permission_level: PermissionLevel::L1DiffReview,
        resources: ResourceSet {
            read_paths: vec![".agent/diffs/current.diff".to_string()],
            write_paths: vec![".agent/results/review.json".to_string()],
            network: false,
        },
    }
}

#[test]
fn permission_mismatch_is_denied() {
    let repo = std::env::current_dir().expect("cwd");
    let engine = review_diff_engine(repo);
    let mut request = review_diff_request();
    request.permission_level = PermissionLevel::L0Readonly;

    let result = engine.evaluate(&request);

    assert_eq!(result.decision, RuntimeDecisionValue::Deny);
    assert!(
        result
            .reasons
            .iter()
            .any(|reason| reason.contains("permission"))
    );
}

#[test]
fn network_request_is_denied_by_default() {
    let repo = std::env::current_dir().expect("cwd");
    let engine = review_diff_engine(repo);
    let mut request = review_diff_request();
    request.resources.network = true;

    let result = engine.evaluate(&request);

    assert_eq!(result.decision, RuntimeDecisionValue::Deny);
    assert!(
        result
            .reasons
            .iter()
            .any(|reason| reason.contains("network"))
    );
}

#[test]
fn denied_path_blocks_operation() {
    let repo = std::env::current_dir().expect("cwd");
    let engine = review_diff_engine(repo);
    let mut request = review_diff_request();
    request.resources.read_paths = vec![".agent/secrets/token.txt".to_string()];

    let result = engine.evaluate(&request);

    assert_eq!(result.decision, RuntimeDecisionValue::Deny);
    assert!(
        result
            .reasons
            .iter()
            .any(|reason| reason.contains("read path"))
    );
}

#[test]
fn allowed_review_diff_request_passes() {
    let repo = std::env::current_dir().expect("cwd");
    let engine = review_diff_engine(repo);

    let result = engine.evaluate(&review_diff_request());

    assert_eq!(result.decision, RuntimeDecisionValue::Allow);
    assert!(result.reasons.is_empty());
}

#[test]
fn unknown_operation_is_denied() {
    let repo = std::env::current_dir().expect("cwd");
    let engine = review_diff_engine(repo);
    let mut request = review_diff_request();
    request.operation = "agent.unknown".to_string();

    let result = engine.evaluate(&request);

    assert_eq!(result.decision, RuntimeDecisionValue::Deny);
    assert!(
        result
            .reasons
            .iter()
            .any(|reason| reason.contains("unknown operation"))
    );
}

#[test]
fn review_diff_registry_exposes_tool_contract_metadata() {
    let registry = ToolRegistry::review_diff();
    let tool = registry
        .get("reasonix.review_diff")
        .expect("review_diff tool is registered");

    assert_eq!(tool.operation(), "reasonix.review_diff");
    assert_eq!(tool.required_permission(), PermissionLevel::L1DiffReview);
    assert_eq!(tool.backend_binding(), BackendBinding::ReasonixAcp);
    assert_eq!(tool.approval_policy(), ApprovalPolicy::Never);
    assert_eq!(tool.input_schema(), "review_diff_input_v1");
    assert_eq!(tool.output_schema(), "coagent_review_wrapper_v1");
    assert!(!tool.capabilities().network);
    assert!(
        tool.capabilities()
            .read_allow
            .contains(&"docs/**".to_string())
    );
    assert!(
        tool.capabilities()
            .write_allow
            .contains(&".agent/results/**".to_string())
    );
}

#[test]
fn policy_engine_uses_registry_defined_capabilities_per_tool() {
    let repo = std::env::current_dir().expect("cwd");
    let registry = ToolRegistry::new().register(ToolDefinition::new(
        "agent.docs_read",
        PermissionLevel::L0Readonly,
        BackendBinding::Mock,
        ApprovalPolicy::Never,
        "docs_read_input_v1",
        "docs_read_result_v1",
        ToolCapabilities {
            read_allow: vec!["docs/**".to_string()],
            write_allow: vec![],
            deny: vec![".git/**".to_string()],
            network: false,
        },
    ));
    let engine = PolicyEngine::from_tool_registry(repo, registry).expect("policy engine");

    let allowed = engine.evaluate(&RuntimeOperationRequest {
        task_id: "TASK-docs".to_string(),
        request_id: Some("REQ-docs".to_string()),
        operation: "agent.docs_read".to_string(),
        permission_level: PermissionLevel::L0Readonly,
        resources: ResourceSet {
            read_paths: vec!["docs/coagent/README.md".to_string()],
            write_paths: vec![],
            network: false,
        },
    });
    assert_eq!(allowed.decision, RuntimeDecisionValue::Allow);

    let denied = engine.evaluate(&RuntimeOperationRequest {
        task_id: "TASK-docs".to_string(),
        request_id: Some("REQ-docs-denied".to_string()),
        operation: "agent.docs_read".to_string(),
        permission_level: PermissionLevel::L0Readonly,
        resources: ResourceSet {
            read_paths: vec!["crates/coagent-runtime-core/src/lib.rs".to_string()],
            write_paths: vec![],
            network: false,
        },
    });
    assert_eq!(denied.decision, RuntimeDecisionValue::Deny);
    assert!(
        denied
            .reasons
            .iter()
            .any(|reason| reason.contains("read path"))
    );
}

#[test]
fn m2_minimum_owned_types_are_constructible() {
    let resources = ResourceSet {
        read_paths: vec![],
        write_paths: vec![],
        network: false,
    };
    let policy_request = PolicyEvaluationRequest {
        operation: "reasonix.review_diff".to_string(),
        permission_level: PermissionLevel::L1DiffReview,
        resources: resources.clone(),
    };
    let decision = RuntimeDecision {
        task_id: "TASK-policy".to_string(),
        request_id: Some("REQ-policy".to_string()),
        operation: policy_request.operation.clone(),
        decision: RuntimeDecisionValue::Deny,
        reasons: vec!["not evaluated".to_string()],
    };
    let routing = RoutingMetadata {
        project_key_hash: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .to_string(),
        session_key_hash: "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            .to_string(),
        lane: "review".to_string(),
    };

    assert_eq!(policy_request.resources, resources);
    assert_eq!(decision.decision, RuntimeDecisionValue::Deny);
    assert_eq!(routing.lane, "review");
}
