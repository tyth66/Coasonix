use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use coagent_runtime_core::{
    kernel::{RuntimeConfig, RuntimeKernel},
    policy::ToolRegistry,
    schema::{SchemaError, SchemaRegistry},
};
use rmcp::{
    ErrorData, ServiceExt,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock, ServerCapabilities, ServerInfo},
    tool, tool_router,
    transport::stdio,
};
use tokio::sync::Mutex;

mod backends;
mod config;
mod pipeline;
mod tools;

use backends::{
    AgentBackend,
    acp_backend::{AcpBackend, MockBackend},
    agent_profile::AgentProfile,
    backend_trait::{BackendRegistry, BackendSelector, DefaultBackendSelector},
    context::ContextProjection,
    mock::PureReviewResult,
};
use config::{BackendId, Config};
use pipeline::{ArtifactPaths, ExecutorContext, RuntimeToolExecutor, ValidationError};
use tools::review_diff::{CoagentReviewWrapper, ReviewDiffInput, ReviewMetadata};
use tools::tool_spec::ToolSpecRegistry;

#[derive(Clone)]
struct CoagentServer {
    executor: RuntimeToolExecutor,
    backend_id: String,
    repo_root: PathBuf,
    reasonix_backend: Option<Arc<AcpBackend>>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct RuntimeStatusResponse {
    backend: String,
    repo_root: String,
    reasonix: Option<backends::acp_backend::ReasonixRuntimeStatus>,
}

fn select_startup_backend(
    backend_override: Option<BackendId>,
    tool_spec: &tools::tool_spec::ToolSpec,
    mock_backend: Arc<dyn AgentBackend>,
    reasonix_backend: Arc<dyn AgentBackend>,
) -> Arc<dyn AgentBackend> {
    match backend_override {
        Some(BackendId::Mock) => mock_backend,
        Some(BackendId::Reasonix) => reasonix_backend,
        None => {
            let selector = DefaultBackendSelector;
            let selected_id = selector.select(
                &tool_spec.required_capability,
                &tool_spec.default_backend_id,
                &[reasonix_backend.as_ref(), mock_backend.as_ref()],
            );
            if selected_id == reasonix_backend.backend_id() {
                reasonix_backend
            } else {
                mock_backend
            }
        }
    }
}

fn runtime_status_response(
    backend_id: &str,
    repo_root: &Path,
    reasonix_backend: Option<&AcpBackend>,
) -> RuntimeStatusResponse {
    RuntimeStatusResponse {
        backend: backend_id.into(),
        repo_root: repo_root.to_string_lossy().into_owned(),
        reasonix: reasonix_backend.map(AcpBackend::runtime_status),
    }
}

#[tool_router]
impl CoagentServer {
    #[tool(
        name = "coagent.review_diff",
        description = "Review a prepared diff through the Coagent runtime gate."
    )]
    async fn review_diff(
        &self,
        Parameters(input): Parameters<ReviewDiffInput>,
    ) -> Result<CallToolResult, ErrorData> {
        // Build artifact paths: collect all optional context files
        let artifact_paths = ArtifactPaths::collect_read(
            &input.artifacts.diff_path,
            &[
                input.artifacts.context_path.as_deref(),
                input.artifacts.test_log_path.as_deref(),
                input.artifacts.build_log_path.as_deref(),
            ],
        )
        .with_write(vec![format!(
            ".agent/results/{}.json",
            input.request_id.as_deref().unwrap_or("auto")
        )]);

        let goal = input.goal.clone();
        let diff_path = input.artifacts.diff_path.clone();

        // Build context projection from all input fields
        let context = ContextProjection::from_input(
            goal.clone(),
            diff_path.clone(),
            input.artifacts.context_path.clone(),
            input.artifacts.test_log_path.clone(),
            input.artifacts.build_log_path.clone(),
            input.focus.clone(),
            input.constraints.clone(),
            input.repo.base_branch.clone(),
            input.repo.working_branch.clone(),
        );

        // Delegate to the unified executor pipeline
        self.executor
            .execute(
                input.task_id.clone(),
                input.request_id.clone(),
                &input,
                artifact_paths,
                // Backend call closure
                |backend: std::sync::Arc<dyn AgentBackend>| {
                    let ctx = context.clone();
                    async move {
                        let request =
                            ctx.to_backend_request("coagent.review_diff", "pure_review_result_v1");
                        let response = backend.invoke(request).await.map_err(|e| e.to_string())?;
                        serde_json::from_value(response.payload)
                            .map_err(|e| format!("deserialize review: {e}"))
                    }
                },
                // Output validation closure
                |review: &PureReviewResult| {
                    review
                        .validate()
                        .map_err(|e| ValidationError::new(e.path, e.message))
                },
                // Wrapper construction closure
                |review: PureReviewResult, task_id: &str, request_id: &str| CoagentReviewWrapper {
                    review,
                    metadata: ReviewMetadata {
                        schema_version: "review_result_v1".into(),
                        task_id: task_id.into(),
                        request_id: request_id.into(),
                        status: "ok".into(),
                        operation: "coagent.review_diff".into(),
                        runtime_decision: "allow".into(),
                    },
                },
            )
            .await
    }

    #[tool(
        name = "coagent.runtime_status",
        description = "Return the current Coagent runtime status without invoking any backend."
    )]
    async fn runtime_status(&self) -> Result<CallToolResult, ErrorData> {
        let status = runtime_status_response(
            &self.backend_id,
            &self.repo_root,
            self.reasonix_backend.as_deref(),
        );
        let text = serde_json::to_string(&status)
            .unwrap_or_else(|_| r#"{"error":"runtime status serialization failed"}"#.into());
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }
}

#[rmcp_macros::tool_handler]
impl rmcp::handler::server::ServerHandler for CoagentServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env()?;
    let review_tool = ToolRegistry::review_diff()
        .get("coagent.review_diff")
        .expect("review_diff tool definition")
        .clone();
    // Build BackendRegistry with available backends
    let mut backend_registry = BackendRegistry::new();
    let mock_backend: Arc<dyn AgentBackend> = Arc::new(MockBackend::new("mock"));
    backend_registry.register_arc(mock_backend.clone());

    let reasonix_acp_backend = Arc::new(AcpBackend::new(AgentProfile::reasonix(
        config.repo_root.clone(),
        &config.reasonix_model,
    )));
    let reasonix_backend: Arc<dyn AgentBackend> = reasonix_acp_backend.clone();
    backend_registry.register_arc(reasonix_backend.clone());

    // Build ToolSpec registry
    let tool_registry = ToolSpecRegistry::default_registry();
    let tool_spec = tool_registry
        .get("coagent.review_diff")
        .expect("coagent.review_diff tool spec");

    let backend = select_startup_backend(
        config.backend_override,
        tool_spec,
        mock_backend.clone(),
        reasonix_backend.clone(),
    );
    let backend_id = backend.backend_id().to_string();
    let selected_reasonix_backend =
        (backend_id == reasonix_acp_backend.backend_id()).then_some(reasonix_acp_backend.clone());

    let kernel = RuntimeKernel::initialize(RuntimeConfig {
        repo_root: config.repo_root.clone(),
    })
    .map_err(|e| format!("failed to initialize runtime kernel: {e}"))?;
    let schema_registry = embedded_schema_registry()
        .map_err(|e| format!("failed to initialize schema registry: {e}"))?;

    let executor = RuntimeToolExecutor::new(ExecutorContext {
        require_external_ids: config.require_external_ids,
        kernel: Arc::new(Mutex::new(kernel)),
        backend: backend.clone(),
        backend_registry: config
            .backend_override
            .is_none()
            .then_some(Arc::new(backend_registry)),
        schema_registry: Arc::new(schema_registry),
        tool: review_tool,
        required_capability: "code.review.diff".into(),
        default_backend_id: "mock".into(),
        backend_output_schema: "pure_review_result_v1".into(),
        complete_task_on_success: true,
    });

    let server = CoagentServer {
        executor,
        backend_id,
        repo_root: config.repo_root,
        reasonix_backend: selected_reasonix_backend,
    };

    let service = server.serve(stdio()).await?;
    service.waiting().await?;

    Ok(())
}

fn embedded_schema_registry() -> Result<SchemaRegistry, SchemaError> {
    SchemaRegistry::load_from_str(
        "coagent-v1.schema.json",
        include_str!("../../../schemas/coagent-v1.schema.json"),
    )
}

#[cfg(test)]
mod tests {
    use super::{runtime_status_response, select_startup_backend};
    use crate::backends::mock::{Finding, PureReviewResult, Severity};
    use crate::backends::{
        AgentBackend,
        acp_backend::{AcpBackend, MockBackend},
        agent_profile::AgentProfile,
    };
    use crate::config::BackendId;
    use crate::tools::tool_spec::ToolSpec;
    use std::{path::PathBuf, sync::Arc};

    #[test]
    fn review_finding_validation_rejects_empty_issue() {
        let review = PureReviewResult {
            verdict: "needs_fix".into(),
            summary: "Finding has empty issue.".into(),
            findings: vec![Finding {
                id: None,
                severity: Severity::Major,
                category: "correctness".into(),
                file: None,
                line: None,
                issue: "".into(),
                evidence: None,
                recommendation: None,
                confidence: 0.5,
            }],
            tests_to_run: vec![],
            risks: vec![],
            assumptions: vec![],
            confidence: 0.8,
        };
        let err = review
            .validate()
            .expect_err("empty issue should be rejected");
        assert!(err.path.contains("issue"));
    }

    #[test]
    fn review_finding_validation_rejects_invalid_confidence() {
        let review = PureReviewResult {
            verdict: "needs_fix".into(),
            summary: "Finding has out-of-range confidence.".into(),
            findings: vec![Finding {
                id: None,
                severity: Severity::Minor,
                category: "style".into(),
                file: None,
                line: None,
                issue: "Some issue".into(),
                evidence: None,
                recommendation: None,
                confidence: 2.5,
            }],
            tests_to_run: vec![],
            risks: vec![],
            assumptions: vec![],
            confidence: 0.8,
        };
        let err = review
            .validate()
            .expect_err("out-of-range confidence should be rejected");
        assert!(err.path.contains("confidence"));
    }

    #[test]
    fn startup_backend_selection_respects_mock_override() {
        let tool_spec = ToolSpec::review_diff();
        let mock_backend: Arc<dyn AgentBackend> = Arc::new(MockBackend::new("mock"));
        let reasonix_backend: Arc<dyn AgentBackend> = Arc::new(AcpBackend::new(
            AgentProfile::reasonix(PathBuf::from("."), "fake-model"),
        ));

        let selected = select_startup_backend(
            Some(BackendId::Mock),
            &tool_spec,
            mock_backend,
            reasonix_backend,
        );

        assert_eq!(selected.backend_id(), "mock");
    }

    #[test]
    fn startup_backend_selection_respects_reasonix_override() {
        let tool_spec = ToolSpec::review_diff();
        let mock_backend: Arc<dyn AgentBackend> = Arc::new(MockBackend::new("mock"));
        let reasonix_backend: Arc<dyn AgentBackend> = Arc::new(AcpBackend::new(
            AgentProfile::reasonix(PathBuf::from("."), "fake-model"),
        ));

        let selected = select_startup_backend(
            Some(BackendId::Reasonix),
            &tool_spec,
            mock_backend,
            reasonix_backend,
        );

        assert_eq!(selected.backend_id(), "reasonix");
    }

    #[test]
    fn startup_backend_selection_uses_capability_when_unset() {
        let tool_spec = ToolSpec::review_diff();
        let mock_backend: Arc<dyn AgentBackend> = Arc::new(MockBackend::new("mock"));
        let reasonix_backend: Arc<dyn AgentBackend> = Arc::new(AcpBackend::new(
            AgentProfile::reasonix(PathBuf::from("."), "fake-model"),
        ));

        let selected = select_startup_backend(None, &tool_spec, mock_backend, reasonix_backend);

        assert_eq!(selected.backend_id(), "reasonix");
    }

    #[test]
    fn runtime_status_reports_reasonix_stats_when_reasonix_is_selected() {
        let repo_root = PathBuf::from("D:/repo");
        let reasonix_backend =
            AcpBackend::new(AgentProfile::reasonix(repo_root.clone(), "fake-model"));

        let status = runtime_status_response("reasonix", &repo_root, Some(&reasonix_backend));

        assert_eq!(status.backend, "reasonix");
        assert_eq!(status.repo_root, repo_root.to_string_lossy());
        let reasonix = status.reasonix.expect("reasonix status");
        assert!(!reasonix.has_session);
        assert_eq!(reasonix.session_created_count, 0);
        assert_eq!(reasonix.prompt_count, 0);
        assert_eq!(reasonix.reconnect_count, 0);
        assert_eq!(reasonix.timeout_count, 0);
        assert_eq!(reasonix.last_error, None);
    }

    #[test]
    fn runtime_status_omits_reasonix_stats_for_mock_backend() {
        let repo_root = PathBuf::from("D:/repo");

        let status = runtime_status_response("mock", &repo_root, None);

        assert_eq!(status.backend, "mock");
        assert_eq!(status.repo_root, repo_root.to_string_lossy());
        assert!(status.reasonix.is_none());
    }
}
