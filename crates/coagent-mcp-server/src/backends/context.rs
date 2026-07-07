/// Structured context projected from the MCP input to the Reasonix prompt.
///
/// Every field from ReviewDiffInput that is meaningful for the review backend
/// is captured here, so the prompt template can reference them.
#[derive(Debug, Clone, Default)]
pub struct ContextProjection {
    pub goal: String,
    pub diff_path: String,
    pub context_path: Option<String>,
    pub test_log_path: Option<String>,
    pub build_log_path: Option<String>,
    pub focus: Vec<String>,
    pub constraints: Vec<String>,
    pub base_branch: Option<String>,
    pub working_branch: Option<String>,
}

impl ContextProjection {

    /// Build from a ReviewDiffInput (convenience for the review_diff tool).
    pub fn from_review_diff_input(input: &crate::tools::review_diff::ReviewDiffInput) -> Self {
        Self {
            goal: input.goal.clone(),
            diff_path: input.artifacts.diff_path.clone(),
            context_path: input.artifacts.context_path.clone(),
            test_log_path: input.artifacts.test_log_path.clone(),
            build_log_path: input.artifacts.build_log_path.clone(),
            focus: input.focus.clone(),
            constraints: input.constraints.clone(),
            base_branch: input.repo.base_branch.clone(),
            working_branch: input.repo.working_branch.clone(),
        }
    }

    /// Build a BackendRequest from this context projection.
    pub fn to_backend_request(&self, operation: &str, output_schema: &str) -> crate::backends::BackendRequest {
        crate::backends::BackendRequest {
            goal: self.goal.clone(),
            operation: operation.to_string(),
            output_schema: output_schema.to_string(),
            read_paths: vec![self.diff_path.clone()],
            context: serde_json::json!({
                "diff_path": self.diff_path,
                "context_path": self.context_path,
                "test_log_path": self.test_log_path,
                "build_log_path": self.build_log_path,
                "focus": self.focus,
                "constraints": self.constraints,
                "base_branch": self.base_branch,
                "working_branch": self.working_branch,
            }),
        }
    }
    /// Build from the raw input fields (called by the tool handler).
    #[allow(clippy::too_many_arguments)]
    pub fn from_input(
        goal: String,
        diff_path: String,
        context_path: Option<String>,
        test_log_path: Option<String>,
        build_log_path: Option<String>,
        focus: Vec<String>,
        constraints: Vec<String>,
        base_branch: Option<String>,
        working_branch: Option<String>,
    ) -> Self {
        Self {
            goal,
            diff_path,
            context_path,
            test_log_path,
            build_log_path,
            focus,
            constraints,
            base_branch,
            working_branch,
        }
    }

    /// Render the context as a prompt section.
    pub fn render_context_section(&self) -> String {
        let mut section = String::new();

        section.push_str("AVAILABLE FILES:\n");
        section.push_str(&format!("  - diff: {}\n", self.diff_path));
        if let Some(ref p) = self.context_path {
            section.push_str(&format!("  - context: {p}\n"));
        }
        if let Some(ref p) = self.test_log_path {
            section.push_str(&format!("  - test log: {p}\n"));
        }
        if let Some(ref p) = self.build_log_path {
            section.push_str(&format!("  - build log: {p}\n"));
        }

        if let Some(ref base) = self.base_branch {
            section.push_str(&format!("\nBASE BRANCH: {base}\n"));
        }
        if let Some(ref working) = self.working_branch {
            section.push_str(&format!("WORKING BRANCH: {working}\n"));
        }

        if !self.focus.is_empty() {
            section.push_str(&format!(
                "\nFOCUS AREAS:\n{}",
                self.focus
                    .iter()
                    .map(|f| format!("  - {f}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
            section.push('\n');
        }

        if !self.constraints.is_empty() {
            section.push_str(&format!(
                "\nCONSTRAINTS:\n{}",
                self.constraints
                    .iter()
                    .map(|c| format!("  - {c}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
            section.push('\n');
        }

        section
    }
}
