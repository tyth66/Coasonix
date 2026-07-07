use std::sync::Arc;

use coagent_runtime_core::{
    kernel::{RuntimeConfig, RuntimeKernel},
    policy::ToolRegistry,
    schema::{SchemaError, SchemaRegistry},
};
use rmcp::{
    ErrorData, ServiceExt,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ServerCapabilities, ServerInfo},
    tool, tool_router,
    transport::stdio,
};
use tokio::sync::Mutex;

mod backends;
mod config;
mod pipeline;
mod tools;

use backends::{Backend, context::ContextProjection, mock::PureReviewResult};
use config::Config;
use pipeline::{ArtifactPaths, ExecutorContext, RuntimeToolExecutor, ValidationError};
use tools::review_diff::{CoagentReviewWrapper, ReviewDiffInput, ReviewMetadata};

#[derive(Clone)]
struct CoagentServer {
    executor: RuntimeToolExecutor,
}

#[tool_router]
impl CoagentServer {
    #[tool(
        name = "reasonix.review_diff",
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
                |backend| {
                    let ctx = context.clone();
                    async move {
                        match backend {
                            Backend::Mock => Ok(PureReviewResult::mock_pass()),
                            Backend::Reasonix(runner) => runner
                                .run(&ctx.goal, &ctx.diff_path, &ctx)
                                .await
                                .map_err(|e| e.to_string()),
                        }
                    }
                },
                // Output validation closure
                |review: &PureReviewResult| {
                    review
                        .validate()
                        .map_err(|e| ValidationError::new(e.path, e.message))
                },
                // Wrapper construction closure
                |review: PureReviewResult| CoagentReviewWrapper {
                    review,
                    metadata: ReviewMetadata {
                        schema_version: "review_result_v1".into(),
                        task_id: String::new(), // filled by executor from IDs
                        request_id: String::new(), // filled by executor from IDs
                        status: "ok".into(),
                        operation: "reasonix.review_diff".into(),
                        runtime_decision: "allow".into(),
                    },
                },
            )
            .await
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
        .get("reasonix.review_diff")
        .expect("review_diff tool definition")
        .clone();
    let backend = Backend::from_configured_tool(
        config.backend_override,
        &review_tool,
        &config.reasonix_model,
        &config.repo_root,
    );

    let kernel = RuntimeKernel::initialize(RuntimeConfig {
        repo_root: config.repo_root.clone(),
    })
    .map_err(|e| format!("failed to initialize runtime kernel: {e}"))?;
    let schema_registry = embedded_schema_registry()
        .map_err(|e| format!("failed to initialize schema registry: {e}"))?;

    let executor = RuntimeToolExecutor::new(ExecutorContext {
        require_external_ids: config.require_external_ids,
        kernel: Arc::new(Mutex::new(kernel)),
        backend,
        schema_registry: Arc::new(schema_registry),
        tool: review_tool,
    });

    let server = CoagentServer { executor };

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
    use crate::backends::mock::{Finding, PureReviewResult, Severity};

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
}
