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
