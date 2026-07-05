use std::path::PathBuf;

use coasonix_runtime_core::{
    artifact::ArtifactPolicy,
    policy::{
        CommandInvocation, PermissionLevel, PolicyEngine, PolicyEvaluationRequest, ResourceSet,
        RoutingMetadata, RuntimeDecision, RuntimeDecisionValue, RuntimeOperationRequest,
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
    PolicyEngine::review_diff("reasonix", artifact_policy)
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
            command: Some(CommandInvocation::Argv(vec![
                "reasonix".to_string(),
                "review-diff".to_string(),
            ])),
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
fn shell_string_is_rejected() {
    let repo = std::env::current_dir().expect("cwd");
    let engine = review_diff_engine(repo);
    let mut request = review_diff_request();
    request.resources.command = Some(CommandInvocation::Shell(
        "reasonix review-diff && curl https://example.invalid".to_string(),
    ));

    let result = engine.evaluate(&request);

    assert_eq!(result.decision, RuntimeDecisionValue::Deny);
    assert!(
        result
            .reasons
            .iter()
            .any(|reason| reason.contains("shell string"))
    );
}

#[test]
fn argv_substring_bypass_is_rejected() {
    let repo = std::env::current_dir().expect("cwd");
    let engine = review_diff_engine(repo);
    let mut request = review_diff_request();
    request.resources.command = Some(CommandInvocation::Argv(vec![
        "reasonix-malicious".to_string(),
        "review-diff".to_string(),
    ]));

    let result = engine.evaluate(&request);

    assert_eq!(result.decision, RuntimeDecisionValue::Deny);
    assert!(
        result
            .reasons
            .iter()
            .any(|reason| reason.contains("argv[0]"))
    );
}

#[test]
fn argv_extra_argument_bypass_is_rejected() {
    let repo = std::env::current_dir().expect("cwd");
    let engine = review_diff_engine(repo);
    let mut request = review_diff_request();
    request.resources.command = Some(CommandInvocation::Argv(vec![
        "reasonix".to_string(),
        "review-diff".to_string(),
        "--network".to_string(),
    ]));

    let result = engine.evaluate(&request);

    assert_eq!(result.decision, RuntimeDecisionValue::Deny);
    assert!(result.reasons.iter().any(|reason| reason.contains("argv args")));
}

#[test]
fn denied_path_blocks_operation_before_read() {
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
fn allowed_review_diff_request_records_command_hash() {
    let repo = std::env::current_dir().expect("cwd");
    let engine = review_diff_engine(repo);

    let result = engine.evaluate(&review_diff_request());

    assert_eq!(result.decision, RuntimeDecisionValue::Allow);
    assert!(result.reasons.is_empty());
    assert!(
        result
            .command_hash
            .as_deref()
            .is_some_and(|hash| hash.starts_with("sha256:"))
    );
}

#[test]
fn m2_minimum_owned_types_are_constructible() {
    let resources = ResourceSet {
        read_paths: vec![],
        write_paths: vec![],
        network: false,
        command: None,
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
        command_hash: None,
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

