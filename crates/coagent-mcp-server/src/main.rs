use std::sync::Arc;

use coagent_runtime_core::{
    kernel::{RuntimeConfig, RuntimeKernel},
    policy::{ResourceSet, RuntimeOperationRequest, ToolDefinition, ToolRegistry},
    schema::{SchemaError, SchemaRegistry, SchemaValidationResult},
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
mod tools;

use backends::{Backend, mock::PureReviewResult};
use config::Config;
use tools::review_diff::{CoagentReviewWrapper, ReviewDiffInput, ReviewMetadata};

#[derive(Clone)]
struct CoagentServer {
    kernel: Arc<Mutex<RuntimeKernel>>,
    backend: Backend,
    schema_registry: Arc<SchemaRegistry>,
    review_tool: ToolDefinition,
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
        validate_review_input_schema(&self.schema_registry, &self.review_tool, &input)
            .map_err(|e| ErrorData::invalid_params(e, None))?;

        if let Err(e) = input.validate() {
            return Err(ErrorData::invalid_params(
                format!("{}: {}", e.path, e.message),
                None,
            ));
        }

        let task_id = input
            .task_id
            .clone()
            .unwrap_or_else(|| format!("TASK-{}", uuid::Uuid::new_v4()));
        let request_id = input
            .request_id
            .clone()
            .unwrap_or_else(|| format!("REQ-{}", uuid::Uuid::new_v4()));

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
                operation: self.review_tool.operation().into(),
                permission_level: self.review_tool.required_permission(),
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
            let _ =
                self.kernel
                    .lock()
                    .await
                    .write_audit(coagent_runtime_core::kernel::AuditEvent {
                        task_id: task_id.clone(),
                        event_type: "runtime_policy_denied".into(),
                        summary: format!("Policy denied: {:?}", decision.reasons),
                        payload_json: serde_json::to_string(&decision.reasons).unwrap_or_default(),
                    });
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
                        self.review_tool.operation(),
                        "worker_schema_invalid",
                        &e.message,
                    );
                    let err_text = format!("worker_schema_invalid: {}: {}", e.path, e.message);
                    return Ok(CallToolResult::error(vec![ContentBlock::text(err_text)]));
                }

                let wrapper = CoagentReviewWrapper {
                    metadata: ReviewMetadata {
                        schema_version: "review_result_v1".into(),
                        task_id: task_id.clone(),
                        request_id: request_id.clone(),
                        status: "ok".into(),
                        operation: self.review_tool.operation().into(),
                        runtime_decision: "allow".into(),
                    },
                    review,
                };

                if let Err(message) = validate_review_wrapper_schema(
                    &self.schema_registry,
                    &self.review_tool,
                    &wrapper,
                ) {
                    let _ = self.kernel.lock().await.fail_operation(
                        &task_id,
                        Some(&request_id),
                        self.review_tool.operation(),
                        "worker_schema_invalid",
                        &message,
                    );
                    return Ok(CallToolResult::error(vec![ContentBlock::text(message)]));
                }

                // 5. Complete task lifecycle
                let _ = self.kernel.lock().await.complete_operation(
                    &task_id,
                    Some(&request_id),
                    self.review_tool.operation(),
                );

                // 6. Serialize wrapped review result
                let text = serde_json::to_string(&wrapper)
                    .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.into());

                Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
            }
            Err(error) => {
                let _ = self.kernel.lock().await.fail_operation(
                    &task_id,
                    Some(&request_id),
                    self.review_tool.operation(),
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
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
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
    let schema_registry = embedded_schema_registry()
        .map_err(|e| format!("failed to initialize schema registry: {e}"))?;
    let review_tool = ToolRegistry::review_diff()
        .get("reasonix.review_diff")
        .expect("review_diff tool definition")
        .clone();

    let server = CoagentServer {
        kernel: Arc::new(Mutex::new(kernel)),
        backend,
        schema_registry: Arc::new(schema_registry),
        review_tool,
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

fn validate_review_input_schema(
    registry: &SchemaRegistry,
    tool: &ToolDefinition,
    input: &ReviewDiffInput,
) -> Result<(), String> {
    let payload =
        serde_json::to_value(input).map_err(|e| format!("schema_serialization_failed: {e}"))?;
    validate_schema_payload(
        registry,
        tool.input_schema(),
        "schema_validation_failed",
        &payload,
    )
}

fn validate_review_wrapper_schema(
    registry: &SchemaRegistry,
    tool: &ToolDefinition,
    wrapper: &CoagentReviewWrapper,
) -> Result<(), String> {
    let payload =
        serde_json::to_value(wrapper).map_err(|e| format!("schema_serialization_failed: {e}"))?;
    validate_schema_payload(
        registry,
        tool.output_schema(),
        "worker_schema_invalid",
        &payload,
    )
}

fn validate_schema_payload(
    registry: &SchemaRegistry,
    schema_name: &str,
    error_prefix: &str,
    payload: &serde_json::Value,
) -> Result<(), String> {
    let result = registry.validate(schema_name, payload);
    if result.valid {
        Ok(())
    } else {
        Err(format_schema_error(error_prefix, &result))
    }
}

fn format_schema_error(prefix: &str, result: &SchemaValidationResult) -> String {
    let details = result
        .errors
        .iter()
        .map(|error| {
            if error.path.is_empty() {
                error.message.clone()
            } else {
                format!("{}: {}", error.path, error.message)
            }
        })
        .collect::<Vec<_>>()
        .join("; ");
    format!("{prefix}: {}: {details}", result.expected_schema)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::mock::PureReviewResult;

    #[test]
    fn review_wrapper_schema_rejects_finding_missing_required_fields() {
        let registry = embedded_schema_registry().expect("schema registry");
        let tool = ToolRegistry::review_diff()
            .get("reasonix.review_diff")
            .expect("review_diff tool")
            .clone();
        let wrapper = CoagentReviewWrapper {
            review: PureReviewResult {
                verdict: "needs_fix".into(),
                summary: "Finding is structurally incomplete.".into(),
                findings: vec![serde_json::json!({
                    "issue": "Missing required schema fields"
                })],
                tests_to_run: vec![],
                risks: vec![],
                assumptions: vec![],
                confidence: 0.8,
            },
            metadata: ReviewMetadata {
                schema_version: "review_result_v1".into(),
                task_id: "TASK-schema".into(),
                request_id: "REQ-schema".into(),
                status: "ok".into(),
                operation: "reasonix.review_diff".into(),
                runtime_decision: "allow".into(),
            },
        };

        let error = validate_review_wrapper_schema(&registry, &tool, &wrapper)
            .expect_err("invalid finding structure should fail schema validation");

        assert!(error.contains("worker_schema_invalid"));
        assert!(error.contains("severity"));
    }
}
