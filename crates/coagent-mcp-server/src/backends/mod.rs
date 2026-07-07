pub mod acp_backend;
pub mod acp_client;
pub mod agent_profile;
pub mod backend_trait;

pub mod context;
pub mod mock;
pub mod reasonix;

use std::path::Path;

use coagent_runtime_core::policy::{BackendBinding, ToolDefinition};

use crate::config::BackendId;

// Re-export the new v3 trait and types
// v3 trait re-exports — used by acp_backend.rs and future tool specs
pub use backend_trait::{AgentBackend, BackendCapabilities, BackendError, BackendRegistry, BackendRequest, BackendResponse, BackendSelector, DefaultBackendSelector, PreferredBackendSelector};

// Legacy v2 Backend enum — kept for backward compatibility during transition.
// New code should use AgentBackend trait via BackendRegistry.
#[derive(Clone)]
pub enum Backend {
    Mock,
    Reasonix(reasonix::ReasonixRunner),
}

impl Backend {
    pub fn from_config(backend_id: BackendId, reasonix_model: &str, repo_root: &Path) -> Self {
        match backend_id {
            BackendId::Mock => Self::Mock,
            BackendId::Reasonix => Self::Reasonix(reasonix::ReasonixRunner::new(
                reasonix_model,
                repo_root.to_path_buf(),
            )),
        }
    }

    pub fn from_tool_binding(
        tool: &ToolDefinition,
        reasonix_model: &str,
        repo_root: &Path,
    ) -> Self {
        match tool.backend_binding() {
            BackendBinding::Mock => Self::Mock,
            BackendBinding::ReasonixAcp => Self::Reasonix(reasonix::ReasonixRunner::new(
                reasonix_model,
                repo_root.to_path_buf(),
            )),
        }
    }

    pub fn from_configured_tool(
        backend_override: Option<BackendId>,
        tool: &ToolDefinition,
        reasonix_model: &str,
        repo_root: &Path,
    ) -> Self {
        match backend_override {
            Some(backend_id) => Self::from_config(backend_id, reasonix_model, repo_root),
            None => Self::from_tool_binding(tool, reasonix_model, repo_root),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coagent_runtime_core::policy::ToolRegistry;

    #[test]
    fn backend_can_be_constructed_from_registered_tool_binding() {
        let tool = ToolRegistry::review_diff()
            .get("coagent.review_diff")
            .expect("review_diff tool")
            .clone();

        let backend = Backend::from_tool_binding(&tool, "fake-model", Path::new("."));

        assert!(matches!(backend, Backend::Reasonix(_)));
    }

    #[test]
    fn configured_backend_override_takes_precedence_over_tool_binding() {
        let tool = ToolRegistry::review_diff()
            .get("coagent.review_diff")
            .expect("review_diff tool")
            .clone();

        let backend = Backend::from_configured_tool(
            Some(BackendId::Mock),
            &tool,
            "fake-model",
            Path::new("."),
        );

        assert!(matches!(backend, Backend::Mock));
    }
}
