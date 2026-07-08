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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_context_round_trips_to_rendered_prompt_section() {
        let value = serde_json::json!({
            "diff_path": ".agent/diffs/current.diff",
            "context_path": ".agent/context/review.md",
            "test_log_path": ".agent/logs/test.log",
            "build_log_path": ".agent/logs/build.log",
            "focus": ["correctness", "policy"],
            "constraints": ["avoid new dependencies"],
            "base_branch": "main",
            "working_branch": "feature/coagent"
        });

        let projection = ContextProjection::from_backend_context(&value).expect("context");
        let rendered = projection.render_context_section();

        assert!(rendered.contains("FOCUS AREAS"));
        assert!(rendered.contains("  - correctness"));
        assert!(rendered.contains("CONSTRAINTS"));
        assert!(rendered.contains("avoid new dependencies"));
        assert!(rendered.contains("BASE BRANCH: main"));
        assert!(rendered.contains("WORKING BRANCH: feature/coagent"));
        assert!(rendered.contains(".agent/logs/test.log"));
        assert!(rendered.contains(".agent/logs/build.log"));
    }

    #[test]
    fn backend_request_read_paths_include_optional_context_and_logs() {
        let projection = ContextProjection {
            goal: "review diff".into(),
            diff_path: ".agent/diffs/current.diff".into(),
            context_path: Some(".agent/context/review.md".into()),
            test_log_path: Some(".agent/logs/test.log".into()),
            build_log_path: Some(".agent/logs/build.log".into()),
            focus: vec![],
            constraints: vec![],
            base_branch: None,
            working_branch: None,
        };

        let request = projection.to_backend_request("coagent.review_diff", "pure_review_result_v1");

        assert_eq!(
            request.read_paths,
            vec![
                ".agent/diffs/current.diff",
                ".agent/context/review.md",
                ".agent/logs/test.log",
                ".agent/logs/build.log"
            ]
        );
    }
}

impl ContextProjection {
    pub fn from_backend_context(value: &serde_json::Value) -> Result<Self, String> {
        let diff_path = value
            .get("diff_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "missing diff_path".to_string())?
            .to_string();
        let optional_string = |key: &str| {
            value
                .get(key)
                .and_then(|v| if v.is_null() { None } else { v.as_str() })
                .map(str::to_string)
        };
        let string_array = |key: &str| {
            value
                .get(key)
                .and_then(|v| v.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(str::to_string))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };

        Ok(Self {
            goal: String::new(),
            diff_path,
            context_path: optional_string("context_path"),
            test_log_path: optional_string("test_log_path"),
            build_log_path: optional_string("build_log_path"),
            focus: string_array("focus"),
            constraints: string_array("constraints"),
            base_branch: optional_string("base_branch"),
            working_branch: optional_string("working_branch"),
        })
    }

    /// Build from a ReviewDiffInput (convenience for the review_diff tool).
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub fn to_backend_request(
        &self,
        operation: &str,
        output_schema: &str,
    ) -> crate::backends::BackendRequest {
        crate::backends::BackendRequest {
            goal: self.goal.clone(),
            operation: operation.to_string(),
            output_schema: output_schema.to_string(),
            read_paths: self.read_paths(),
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

    pub fn read_paths(&self) -> Vec<String> {
        let mut paths = vec![self.diff_path.clone()];
        paths.extend(
            [
                self.context_path.as_ref(),
                self.test_log_path.as_ref(),
                self.build_log_path.as_ref(),
            ]
            .into_iter()
            .flatten()
            .cloned(),
        );
        paths
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
