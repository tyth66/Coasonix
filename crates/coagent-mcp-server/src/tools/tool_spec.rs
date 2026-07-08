use coagent_runtime_core::policy::PermissionLevel;

/// Declarative specification for a Coagent MCP tool.
///
/// Each tool is defined by its schema, permissions, artifact policy,
/// and backend selection criteria. The RuntimeToolExecutor reads
/// ToolSpec to drive the execute pipeline — no per-tool handler code.
#[derive(Clone)]
#[allow(dead_code)]
pub struct ToolSpec {
    /// MCP tool name (e.g. "coagent.review_diff").
    pub name: String,
    /// Schema version for the tool.
    pub version: String,
    /// Input schema name (must exist in SchemaRegistry $defs).
    pub input_schema: String,
    /// Output schema name that the backend response must conform to.
    pub output_schema: String,
    /// Required permission level for this tool.
    pub permission_level: PermissionLevel,
    /// Artifact read paths this tool may access.
    pub read_paths: Vec<String>,
    /// Artifact write paths this tool may use.
    pub write_paths: Vec<String>,
    /// Backend capability tag required to execute this tool.
    /// The BackendRegistry selects a backend matching this tag.
    pub required_capability: String,
    /// Default backend ID if no capability match is found.
    pub default_backend_id: String,
}

impl ToolSpec {
    /// Create the standard review_diff tool specification.
    pub fn review_diff() -> Self {
        Self {
            name: "coagent.review_diff".into(),
            version: "1.0.0".into(),
            input_schema: "review_diff_input_v1".into(),
            output_schema: "review_result_v1".into(),
            permission_level: PermissionLevel::L1DiffReview,
            read_paths: vec![
                ".agent/diffs/**".into(),
                ".agent/context/**".into(),
                ".agent/logs/**".into(),
            ],
            write_paths: vec![".agent/results/**".into()],
            required_capability: "code.review.diff".into(),
            default_backend_id: "mock".into(),
        }
    }

    /// Create the standard runtime_status tool specification.
    pub fn runtime_status() -> Self {
        Self {
            name: "coagent.runtime_status".into(),
            version: "1.0.0".into(),
            input_schema: "runtime_status_input_v1".into(),
            output_schema: "runtime_status_v1".into(),
            permission_level: PermissionLevel::L0Readonly,
            read_paths: vec![],
            write_paths: vec![],
            required_capability: "runtime.status".into(),
            default_backend_id: "mock".into(),
        }
    }
}

/// Registry of all registered ToolSpecs.
#[derive(Default)]
pub struct ToolSpecRegistry {
    specs: Vec<ToolSpec>,
}

impl ToolSpecRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, spec: ToolSpec) {
        self.specs.push(spec);
    }

    pub fn get(&self, name: &str) -> Option<&ToolSpec> {
        self.specs.iter().find(|s| s.name == name)
    }

    #[allow(dead_code)]
    pub fn list_names(&self) -> Vec<String> {
        self.specs.iter().map(|s| s.name.clone()).collect()
    }

    /// Create the default registry with all built-in tools.
    pub fn default_registry() -> Self {
        let mut registry = Self::new();
        registry.register(ToolSpec::review_diff());
        registry.register(ToolSpec::runtime_status());
        // Future tools:
        // registry.register(ToolSpec::review_architecture());
        // registry.register(ToolSpec::security_audit());
        registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_diff_spec_has_correct_defaults() {
        let spec = ToolSpec::review_diff();
        assert_eq!(spec.name, "coagent.review_diff");
        assert_eq!(spec.input_schema, "review_diff_input_v1");
        assert_eq!(spec.output_schema, "review_result_v1");
        assert_eq!(spec.permission_level, PermissionLevel::L1DiffReview);
        assert_eq!(spec.required_capability, "code.review.diff");
        assert!(!spec.read_paths.is_empty());
        assert!(!spec.write_paths.is_empty());
    }

    #[test]
    fn registry_register_and_retrieve() {
        let mut registry = ToolSpecRegistry::new();
        registry.register(ToolSpec::review_diff());
        let spec = registry.get("coagent.review_diff").expect("found");
        assert_eq!(spec.version, "1.0.0");
    }

    #[test]
    fn default_registry_contains_review_diff() {
        let registry = ToolSpecRegistry::default_registry();
        let names = registry.list_names();
        assert!(names.contains(&"coagent.review_diff".to_string()));
        assert!(names.contains(&"coagent.runtime_status".to_string()));
        assert_eq!(names.len(), 2);
    }

    #[test]
    fn runtime_status_spec_is_read_only() {
        let spec = ToolSpec::runtime_status();
        assert_eq!(spec.name, "coagent.runtime_status");
        assert_eq!(spec.input_schema, "runtime_status_input_v1");
        assert_eq!(spec.output_schema, "runtime_status_v1");
        assert_eq!(spec.permission_level, PermissionLevel::L0Readonly);
        assert!(spec.read_paths.is_empty());
        assert!(spec.write_paths.is_empty());
        assert_eq!(spec.required_capability, "runtime.status");
    }
}
