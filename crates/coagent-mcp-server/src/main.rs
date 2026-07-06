use std::sync::Arc;

use coagent_runtime_core::{
    kernel::{RuntimeConfig, RuntimeKernel},
    policy::{PermissionLevel, ResourceSet, RuntimeOperationRequest},
};
use rmcp::{
    ErrorData,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock, ServerCapabilities, ServerInfo},
    tool, tool_router,
    ServiceExt,
    transport::stdio,
};
use tokio::sync::Mutex;

mod backends;
mod config;
mod tools;

use backends::{Backend, mock::PureReviewResult};
use config::Config;
use tools::review_diff::{CoagentReviewWrapper, ReviewDiffInput, ReviewMetadata};

#[derive(Clone)]
struct CoagentServer {
    kernel: Arc<Mutex<RuntimeKernel>>,
    backend: Backend,
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
        // 1. Validate input
        if let Err(e) = input.validate() {
            return Err(ErrorData::invalid_params(e.message, None));
        }

        let task_id = input.task_id.clone().unwrap_or_else(|| format!("TASK-{}", uuid::Uuid::new_v4()));
        let request_id = input.request_id.clone().unwrap_or_else(|| format!("REQ-{}", uuid::Uuid::new_v4()));

        // 2. Runtime gate (same-process call, no JSON-RPC)
        let read_paths: Vec<String> = [
            input.artifacts.context_path.as_deref(),
            Some(&input.artifacts.diff_path),
            input.artifacts.test_log_path.as_deref(),
            input.artifacts.build_log_path.as_deref(),
        ]
        .into_iter()
        .flatten()
        .map(String::from)
        .collect();

        let decision = {
            let mut kernel = self.kernel.lock().await;
            kernel.evaluate_operation(RuntimeOperationRequest {
                task_id: task_id.clone(),
                request_id: Some(request_id.clone()),
                operation: "reasonix.review_diff".into(),
                permission_level: PermissionLevel::L1DiffReview,
                resources: ResourceSet {
                    read_paths,
                    write_paths: vec![format!(".agent/results/{}.json", request_id)],
                    network: false,
                },
            })
        };

        if decision.decision != coagent_runtime_core::policy::RuntimeDecisionValue::Allow {
            // Policy deny: write audit event without state transition
            // (Created->Failed is illegal; deny happens pre-execution)
            let _ = self.kernel.lock().await.write_audit(
                coagent_runtime_core::kernel::AuditEvent {
                    task_id: task_id.clone(),
                    event_type: "runtime_policy_denied".into(),
                    summary: format!("Policy denied: {:?}", decision.reasons),
                    payload_json: serde_json::to_string(&decision.reasons).unwrap_or_default(),
                },
            );
            let err_text = format!("runtime_policy_denied: {:?}", decision.reasons);
            return Ok(CallToolResult::error(vec![ContentBlock::text(err_text)]));
        }

        // 3. Invoke backend
        let review_result: Result<PureReviewResult, String> = match &self.backend {
            Backend::Mock => Ok(PureReviewResult::mock_pass()),
            Backend::Reasonix(runner) => runner
                .run(&input.goal, &input.artifacts.diff_path)
                .await
                .map_err(|e| e.to_string()),
        };

        match review_result {
            Ok(review) => {
                // 4. Validate output
                if let Err(e) = review.validate() {
                    let _ = self.kernel.lock().await.fail_operation(
                        &task_id,
                        Some(&request_id),
                        "reasonix.review_diff",
                        "worker_schema_invalid",
                        &e.message,
                    );
                    let err_text = format!("worker_schema_invalid: {}", e.message);
                    return Ok(CallToolResult::error(vec![ContentBlock::text(err_text)]));
                }

                // 5. Complete task lifecycle
                let _ = self.kernel.lock().await.complete_operation(
                    &task_id,
                    Some(&request_id),
                    "reasonix.review_diff",
                );

                // 6. Wrap pure review result with Coagent metadata
                let wrapper = CoagentReviewWrapper {
                    metadata: ReviewMetadata {
                        schema_version: "review_result_v1".into(),
                        task_id,
                        request_id,
                        status: "ok".into(),
                        operation: "reasonix.review_diff".into(),
                        runtime_decision: "allow".into(),
                    },
                    review,
                };

                let text = serde_json::to_string(&wrapper)
                    .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.into());

                Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
            }
            Err(error) => {
                let _ = self.kernel.lock().await.fail_operation(
                    &task_id,
                    Some(&request_id),
                    "reasonix.review_diff",
                    "worker_unavailable",
                    &error,
                );
                Ok(CallToolResult::error(vec![ContentBlock::text(error)]))
            }
        }
    }
}

// ServerHandler: provide server metadata
#[rmcp_macros::tool_handler]
impl rmcp::handler::server::ServerHandler for CoagentServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .build(),
        )
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env()?;
    let backend = Backend::from_config(config.backend, &config.reasonix_model, &config.repo_root);

    let kernel = RuntimeKernel::initialize(RuntimeConfig {
        repo_root: config.repo_root.clone(),
    })
    .map_err(|e| format!("failed to initialize runtime kernel: {e}"))?;

    let server = CoagentServer {
        kernel: Arc::new(Mutex::new(kernel)),
        backend,
    };

    let service = server.serve(stdio()).await?;
    service.waiting().await?;

    Ok(())
}







