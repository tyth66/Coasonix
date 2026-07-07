use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Core backend trait
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// A request dispatched to a backend agent.
#[derive(Debug, Clone)]
pub struct BackendRequest {
    /// The goal/prompt for the agent.
    pub goal: String,
    /// The operation being executed.
    pub operation: String,
    /// Required input schema the response must conform to.
    pub output_schema: String,
    /// Artifact paths the backend may read.
    pub read_paths: Vec<String>,
    /// Structured context projection (tool-specific).
    pub context: Value,
}

/// A response from a backend agent.
#[derive(Debug, Clone)]
pub struct BackendResponse {
    /// Output schema the payload conforms to.
    pub output_schema: String,
    /// The structured result payload.
    pub payload: Value,
}

/// Capabilities a backend advertises.
#[derive(Debug, Clone, Default)]
pub struct BackendCapabilities {
    pub tags: Vec<String>,
    pub max_tokens: Option<u64>,
    pub supports_streaming: bool,
}

/// Errors from backend invocation.
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("backend unavailable: {0}")]
    Unavailable(String),
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("schema violation: {0}")]
    SchemaViolation(String),
    #[error("timeout")]
    Timeout,
}

/// The trait every backend must implement.
#[async_trait]
pub trait AgentBackend: Send + Sync {
    /// Invoke the backend with a request.
    async fn invoke(&self, request: BackendRequest) -> Result<BackendResponse, BackendError>;

    /// Unique identifier for this backend instance.
    fn backend_id(&self) -> &str;

    /// Capabilities this backend provides.
    fn capabilities(&self) -> BackendCapabilities;
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Backend registry
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Thread-safe registry of available backends.
#[derive(Default)]
pub struct BackendRegistry {
    backends: HashMap<String, Box<dyn AgentBackend>>,
}

impl BackendRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, backend: Box<dyn AgentBackend>) {
        self.backends
            .insert(backend.backend_id().to_string(), backend);
    }

    pub fn get(&self, id: &str) -> Option<&dyn AgentBackend> {
        self.backends.get(id).map(|b| b.as_ref())
    }

    /// Return the first backend whose capabilities include the given tag, or the default.
    pub fn select_by_tag(&self, tag: &str, default_id: &str) -> &dyn AgentBackend {
        for backend in self.backends.values() {
            if backend.capabilities().tags.iter().any(|t| t == tag) {
                return backend.as_ref();
            }
        }
        self.get(default_id).expect("default backend not found")
    }

    pub fn list_ids(&self) -> Vec<String> {
        self.backends.keys().cloned().collect()
    }
}



/// Selects a backend for a given tool specification.
pub trait BackendSelector: Send + Sync {
    /// Given a tool spec and available backends, pick one.
    /// Returns the selected backend's ID.
    fn select(
        &self,
        tool_required_capability: &str,
        tool_default_backend: &str,
        available: &[&dyn AgentBackend],
    ) -> String;
}

/// Simple selector: capability match first, then fallback to default.
pub struct DefaultBackendSelector;

impl BackendSelector for DefaultBackendSelector {
    fn select(
        &self,
        tool_required_capability: &str,
        tool_default_backend: &str,
        available: &[&dyn AgentBackend],
    ) -> String {
        // Try capability match first
        for b in available {
            if b.capabilities().tags.iter().any(|t| t == tool_required_capability) {
                return b.backend_id().to_string();
            }
        }
        // Fallback to default
        tool_default_backend.to_string()
    }
}

/// Selector with explicit preference order.
pub struct PreferredBackendSelector {
    pub preferred_backend: String,
    pub fallback_backend: String,
}

impl BackendSelector for PreferredBackendSelector {
    fn select(
        &self,
        _tool_required_capability: &str,
        _tool_default_backend: &str,
        available: &[&dyn AgentBackend],
    ) -> String {
        // Try preferred first
        if available.iter().any(|b| b.backend_id() == self.preferred_backend) {
            return self.preferred_backend.clone();
        }
        // Fallback
        self.fallback_backend.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestBackend {
        id: String,
    }

    #[async_trait]
    impl AgentBackend for TestBackend {
        async fn invoke(&self, _request: BackendRequest) -> Result<BackendResponse, BackendError> {
            Ok(BackendResponse {
                output_schema: "test".into(),
                payload: serde_json::json!({"status": "ok"}),
            })
        }

        fn backend_id(&self) -> &str {
            &self.id
        }

        fn capabilities(&self) -> BackendCapabilities {
            BackendCapabilities {
                tags: vec!["test".into()],
                ..Default::default()
            }
        }
    }

    #[test]
    fn registry_register_and_retrieve() {
        let mut registry = BackendRegistry::new();
        registry.register(Box::new(TestBackend {
            id: "test-1".into(),
        }));

        let backend = registry.get("test-1").expect("backend exists");
        assert_eq!(backend.backend_id(), "test-1");
    }

    #[test]
    fn registry_select_by_tag_finds_matching_backend() {
        let mut registry = BackendRegistry::new();
        registry.register(Box::new(TestBackend {
            id: "default".into(),
        }));
        registry.register(Box::new(TestBackend {
            id: "default".into(),
        }));

        // Both have tag "test", so the first one registered is returned
        let selected = registry.select_by_tag("test", "default");
        assert_eq!(selected.backend_id(), "default");
    }
}
