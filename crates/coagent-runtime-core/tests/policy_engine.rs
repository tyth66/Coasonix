use std::path::PathBuf;

use coagent_runtime_core::{
    artifact::ArtifactPolicy,
    policy::{
        PermissionLevel, PolicyEngine, PolicyEvaluationRequest, ResourceSet, RoutingMetadata,
        RuntimeDecision, RuntimeDecisionValue, RuntimeOperationRequest,
    },
};

fn review_diff_engine(repo_root: impl Into<PathBuf>) -> PolicyEngine {
    let artifact_policy = ArtifactPolicy::new(repo_root.into())
        .expect("artifact policy")
        .allow_read([
            ".agent/context/**",
            ".agent/diffs/**",
            ".agent/logs/**",
            "docs/**",
            "crates/**",
            "packages/**",
        ])
        .allow_write([".agent/results/**", ".agent/logs/**"])
        .deny([".agent/secrets/**", ".git/**"]);
    PolicyEngine::review_diff(artifact_policy)
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
