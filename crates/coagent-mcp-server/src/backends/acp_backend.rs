use std::path::PathBuf;

use async_trait::async_trait;

use super::backend_trait::{AgentBackend, BackendCapabilities, BackendError, BackendRequest, BackendResponse};
use super::reasonix::{ReasonixError, ReasonixRunner};

/// An ACP backend wrapping a ReasonixRunner, implementing the AgentBackend trait.
/// This is the bridge from the v2 Reasonix-specific code to the v3 generic backend model.
pub struct AcpBackend {
    id: String,
    runner: ReasonixRunner,
}

impl AcpBackend {
    pub fn new(id: impl Into<String>, model: impl Into<String>, cwd: PathBuf) -> Self {
        Self {
            id: id.into(),
            runner: ReasonixRunner::new(model, cwd),
        }
    }
}

#[async_trait]
impl AgentBackend for AcpBackend {
    async fn invoke(&self, request: BackendRequest) -> Result<BackendResponse, BackendError> {
        // Extract goal and diff_path from context for the Reasonix runner.
        // In v3, this projection would be handled by ToolSpec.context_projector.
        let goal = &request.goal;
        let diff_path = request
            .context
            .get("diff_path")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let context = crate::backends::context::ContextProjection::from_input(
            goal.clone(),
            diff_path.to_string(),
            request.context.get("context_path").and_then(|v| v.as_str()).map(String::from),
            request.context.get("test_log_path").and_then(|v| v.as_str()).map(String::from),
            request.context.get("build_log_path").and_then(|v| v.as_str()).map(String::from),
            request.context.get("focus").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default(),
            request.context.get("constraints").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default(),
            request.context.get("base_branch").and_then(|v| v.as_str()).map(String::from),
            request.context.get("working_branch").and_then(|v| v.as_str()).map(String::from),
        );

        let review = self
            .runner
            .run(goal, diff_path, &context)
            .await
            .map_err(|e| match e {
                ReasonixError::Spawn(msg) | ReasonixError::Io(msg) => {
                    BackendError::Unavailable(msg)
                }
                ReasonixError::Protocol(msg) => BackendError::Protocol(msg),
                ReasonixError::Timeout(_msg) => BackendError::Timeout,
            })?;

        let payload = serde_json::to_value(&review)
            .map_err(|e| BackendError::Protocol(format!("serialization: {e}")))?;

        Ok(BackendResponse {
            output_schema: request.output_schema.clone(),
            payload,
        })
    }

    fn backend_id(&self) -> &str {
        &self.id
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            tags: vec!["code.review.diff".into()],
            max_tokens: None,
            supports_streaming: true,
        }
    }
}

/// A mock backend that returns a pass review instantly.
pub struct MockBackend {
    id: String,
}

impl MockBackend {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

#[async_trait]
impl AgentBackend for MockBackend {
    async fn invoke(&self, _request: BackendRequest) -> Result<BackendResponse, BackendError> {
        let review = super::mock::PureReviewResult::mock_pass();
        let payload = serde_json::to_value(&review)
            .map_err(|e| BackendError::Protocol(format!("serialization: {e}")))?;
        Ok(BackendResponse {
            output_schema: "review_result_v1".into(),
            payload,
        })
    }

    fn backend_id(&self) -> &str {
        &self.id
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            tags: vec!["mock".into()],
            max_tokens: None,
            supports_streaming: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_backend_invoke_returns_pass() {
        let backend = MockBackend::new("mock");
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(backend.invoke(BackendRequest {
            goal: "test".into(),
            operation: "review_diff".into(),
            output_schema: "review_result_v1".into(),
            read_paths: vec![],
            context: serde_json::json!({}),
        }));
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.payload["verdict"], "pass");
    }

    #[test]
    fn acp_backend_registers_with_id() {
        let backend = AcpBackend::new("reasonix-1", "deepseek-v4-flash", PathBuf::from("."));
        assert_eq!(backend.backend_id(), "reasonix-1");
        assert!(backend.capabilities().tags.contains(&"code.review.diff".to_string()));
    }
}
