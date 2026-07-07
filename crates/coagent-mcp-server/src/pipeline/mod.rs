use std::sync::Arc;

use coagent_runtime_core::{
    kernel::{AuditEvent, RuntimeDecisionValue, RuntimeKernel},
    policy::{ResourceSet, RuntimeOperationRequest, ToolDefinition},
    schema::SchemaRegistry,
};
use rmcp::{
    ErrorData,
    model::{CallToolResult, ContentBlock},
};
use serde::Serialize;
use tokio::sync::Mutex;

use crate::backends::Backend;

/// Shared server state passed to the executor pipeline.
#[derive(Clone)]
pub struct ExecutorContext {
    pub require_external_ids: bool,
    pub kernel: Arc<Mutex<RuntimeKernel>>,
    pub backend: Backend,
    pub schema_registry: Arc<SchemaRegistry>,
    pub tool: ToolDefinition,
}

/// The result of each pipeline stage.
/// Unified runtime tool execution pipeline.
///
/// Every MCP tool handler follows the same 6-stage pattern:
/// validate input → runtime gate → invoke backend → validate output →
/// lifecycle close → serialize response.
///
/// This executor owns the pipeline. Each tool handler becomes a thin
/// declarative wrapper that provides:
/// - input deserialization + validation
/// - artifact path plan
/// - backend invocation closure
/// - output wrapper construction
#[derive(Clone)]
pub struct RuntimeToolExecutor {
    ctx: ExecutorContext,
}

impl RuntimeToolExecutor {
    pub fn new(ctx: ExecutorContext) -> Self {
        Self { ctx }
    }

    /// Execute the tool pipeline end-to-end.
    ///
    /// # Type parameters
    /// - `I`: tool-specific input type (must be serde::Serialize for schema validation)
    /// - `O`: backend output type
    /// - `W`: final wrapper type (must be serde::Serialize for schema validation + MCP serialization)
    #[allow(clippy::too_many_arguments)]
    pub async fn execute<I, O, W, BFut>(
        &self,
        task_id: Option<String>,
        request_id: Option<String>,
        input: &I,
        artifact_paths: ArtifactPaths,
        backend_call: impl FnOnce(Backend) -> BFut,
        validate_output: impl FnOnce(&O) -> Result<(), ValidationError>,
        build_wrapper: impl FnOnce(O) -> W,
    ) -> Result<CallToolResult, ErrorData>
    where
        I: Serialize,
        O: Send,
        W: Serialize,
        BFut: std::future::Future<Output = Result<O, String>> + Send,
    {
        // ── Stage 1: Validate input schema ──
        let payload = serde_json::to_value(input).map_err(|e| {
            ErrorData::invalid_params(format!("input serialization failed: {e}"), None)
        })?;
        let validation = self
            .ctx
            .schema_registry
            .validate(self.ctx.tool.input_schema(), &payload);
        if !validation.valid {
            let detail = format_schema_errors(self.ctx.tool.input_schema(), &validation);
            return Err(ErrorData::invalid_params(detail, None));
        }

        // ── Stage 2: Generate or enforce IDs ──
        if self.ctx.require_external_ids && task_id.is_none() {
            return Err(ErrorData::invalid_params(
                "task_id is required when COAGENT_REQUIRE_EXTERNAL_IDS=true",
                None,
            ));
        }
        if self.ctx.require_external_ids && request_id.is_none() {
            return Err(ErrorData::invalid_params(
                "request_id is required when COAGENT_REQUIRE_EXTERNAL_IDS=true",
                None,
            ));
        }
        let task_id = task_id.unwrap_or_else(|| format!("TASK-{}", uuid::Uuid::new_v4()));
        let request_id = request_id.unwrap_or_else(|| format!("REQ-{}", uuid::Uuid::new_v4()));

        // ── Stage 3: Runtime gate ──
        let decision = {
            let mut kernel = self.ctx.kernel.lock().await;
            kernel.evaluate_operation(RuntimeOperationRequest {
                task_id: task_id.clone(),
                request_id: Some(request_id.clone()),
                operation: self.ctx.tool.operation().into(),
                permission_level: self.ctx.tool.required_permission(),
                resources: ResourceSet {
                    read_paths: artifact_paths.read_paths,
                    write_paths: artifact_paths.write_paths,
                    network: false,
                },
            })
        };

        match decision.decision {
            RuntimeDecisionValue::RequireApproval => {
                let _ = self.ctx.kernel.lock().await.write_audit(AuditEvent {
                    task_id: task_id.clone(),
                    event_type: "approval_required".into(),
                    summary: format!("Approval required for {}", self.ctx.tool.operation()),
                    payload_json: serde_json::json!({
                        "task_id": &task_id,
                        "request_id": &request_id,
                        "reasons": decision.reasons
                    })
                    .to_string(),
                });
                let wrapper = serde_json::json!({
                    "status": "approval_required",
                    "task_id": &task_id,
                    "request_id": &request_id,
                    "reasons": decision.reasons
                });
                return Ok(CallToolResult::success(vec![ContentBlock::text(
                    serde_json::to_string(&wrapper).unwrap_or_default(),
                )]));
            }
            RuntimeDecisionValue::Allow => { /* proceed */ }
            _ => {
                let _ = self.ctx.kernel.lock().await.write_audit(AuditEvent {
                    task_id: task_id.clone(),
                    event_type: "runtime_policy_denied".into(),
                    summary: format!("Policy denied: {:?}", decision.reasons),
                    payload_json: serde_json::to_string(&decision.reasons).unwrap_or_default(),
                });
                let err_text = format!("runtime_policy_denied: {:?}", decision.reasons);
                return Ok(CallToolResult::error(vec![ContentBlock::text(err_text)]));
            }
        }

        // ── Stage 4: Invoke backend ──
        let backend_output = match backend_call(self.ctx.backend.clone()).await {
            Ok(output) => output,
            Err(error) => {
                let _ = self.ctx.kernel.lock().await.fail_operation(
                    &task_id,
                    Some(&request_id),
                    self.ctx.tool.operation(),
                    "worker_unavailable",
                    &error,
                );
                return Ok(CallToolResult::error(vec![ContentBlock::text(error)]));
            }
        };

        // ── Stage 5: Validate output ──
        if let Err(e) = validate_output(&backend_output) {
            let _ = self.ctx.kernel.lock().await.fail_operation(
                &task_id,
                Some(&request_id),
                self.ctx.tool.operation(),
                "worker_schema_invalid",
                &e.message,
            );
            let err_text = format!("worker_schema_invalid: {}: {}", e.path, e.message);
            return Ok(CallToolResult::error(vec![ContentBlock::text(err_text)]));
        }

        // ── Stage 6: Build wrapper + validate wrapper schema ──
        let wrapper = build_wrapper(backend_output);
        let wrapper_payload = serde_json::to_value(&wrapper).map_err(|e| {
            ErrorData::internal_error(format!("wrapper serialization failed: {e}"), None)
        })?;
        let wrapper_validation = self
            .ctx
            .schema_registry
            .validate(self.ctx.tool.output_schema(), &wrapper_payload);
        if !wrapper_validation.valid {
            let detail = format_schema_errors(self.ctx.tool.output_schema(), &wrapper_validation);
            let _ = self.ctx.kernel.lock().await.fail_operation(
                &task_id,
                Some(&request_id),
                self.ctx.tool.operation(),
                "worker_schema_invalid",
                &detail,
            );
            return Ok(CallToolResult::error(vec![ContentBlock::text(detail)]));
        }

        // ── Stage 7: Complete lifecycle ──
        let _ = self.ctx.kernel.lock().await.complete_operation(
            &task_id,
            Some(&request_id),
            self.ctx.tool.operation(),
        );

        // ── Stage 8: Serialize final response ──
        let text = serde_json::to_string(&wrapper)
            .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.into());
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }
}

/// Artifact path plan for a tool invocation.
pub struct ArtifactPaths {
    pub read_paths: Vec<String>,
    pub write_paths: Vec<String>,
}

impl ArtifactPaths {
    /// Collect read paths from optional fields, skipping None.
    pub fn collect_read(required: &str, optional: &[Option<&str>]) -> Self {
        let mut read = vec![required.to_string()];
        read.extend(optional.iter().flatten().map(|s| s.to_string()));
        Self {
            read_paths: read,
            write_paths: vec![],
        }
    }

    pub fn with_write(mut self, write_paths: Vec<String>) -> Self {
        self.write_paths = write_paths;
        self
    }
}

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub path: String,
    pub message: String,
}

impl ValidationError {
    pub fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

fn format_schema_errors(
    schema_name: &str,
    result: &coagent_runtime_core::schema::SchemaValidationResult,
) -> String {
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
    format!("schema_validation_failed: {}: {}", schema_name, details)
}
