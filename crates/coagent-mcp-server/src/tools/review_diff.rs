use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::backends::mock::PureReviewResult;

// ── MCP Input schema ──

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[schemars(rename = "review_diff_input_v1")]
pub struct ReviewDiffInput {
    pub schema_version: String,
    pub task_id: Option<String>,
    pub request_id: Option<String>,
    pub mode: Option<String>,
    pub goal: String,
    pub repo: RepoInfo,
    pub artifacts: Artifacts,
    #[serde(default)]
    pub focus: Vec<String>,
    #[serde(default)]
    pub constraints: Vec<String>,
    pub budget: Option<Budget>,
    pub permission_level: String,
    pub output_schema: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct RepoInfo {
    pub root: String,
    pub base_branch: Option<String>,
    pub working_branch: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct Artifacts {
    pub diff_path: String,
    pub context_path: Option<String>,
    pub test_log_path: Option<String>,
    pub build_log_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct Budget {
    pub max_minutes: Option<i64>,
    pub max_output_chars: Option<i64>,
    pub max_steps: Option<i64>,
}

// ── Validation ──

#[derive(Debug)]
pub struct ValidationError {
    pub path: String,
    pub message: String,
}

impl ReviewDiffInput {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.schema_version != "review_diff_input_v1" {
            return Err(ValidationError {
                path: "/schema_version".into(),
                message: "schema_version must be review_diff_input_v1".into(),
            });
        }
        if self.goal.is_empty() {
            return Err(ValidationError {
                path: "/goal".into(),
                message: "goal is required".into(),
            });
        }
        if self.repo.root.is_empty() {
            return Err(ValidationError {
                path: "/repo/root".into(),
                message: "repo.root is required".into(),
            });
        }
        if self.artifacts.diff_path.is_empty() {
            return Err(ValidationError {
                path: "/artifacts/diff_path".into(),
                message: "artifacts.diff_path is required".into(),
            });
        }
        if self.permission_level != "L1_DIFF_REVIEW" {
            return Err(ValidationError {
                path: "/permission_level".into(),
                message: "permission_level must be L1_DIFF_REVIEW".into(),
            });
        }
        if self.output_schema != "review_result_v1" {
            return Err(ValidationError {
                path: "/output_schema".into(),
                message: "output_schema must be review_result_v1".into(),
            });
        }
        Ok(())
    }
}

// ── Output validation ──

impl PureReviewResult {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if !matches!(
            self.verdict.as_str(),
            "pass" | "needs_fix" | "risky" | "unknown" | "not_applicable"
        ) {
            return Err(ValidationError {
                path: "/verdict".into(),
                message: "verdict must be a valid review verdict".into(),
            });
        }
        if self.summary.is_empty() {
            return Err(ValidationError {
                path: "/summary".into(),
                message: "summary must be a non-empty string".into(),
            });
        }
        if !(0.0..=1.0).contains(&self.confidence) {
            return Err(ValidationError {
                path: "/confidence".into(),
                message: "confidence must be between 0 and 1".into(),
            });
        }
        Ok(())
    }
}

// ── Coagent wrapper (metadata attached by server, not by backend) ──

#[derive(Debug, Serialize)]
pub struct CoagentReviewWrapper {
    pub review: PureReviewResult,
    pub metadata: ReviewMetadata,
}

#[derive(Debug, Serialize)]
pub struct ReviewMetadata {
    pub schema_version: String,
    pub task_id: String,
    pub request_id: String,
    pub status: String,
    pub operation: String,
    pub runtime_decision: String,
}
#[cfg(test)]
mod tests {
    use super::*;

    // ── Input validation ──

    fn valid_input() -> ReviewDiffInput {
        ReviewDiffInput {
            schema_version: "review_diff_input_v1".into(),
            task_id: Some("TASK-test".into()),
            request_id: Some("REQ-test".into()),
            mode: Some("review_diff".into()),
            goal: "Test review".into(),
            repo: RepoInfo {
                root: "/test".into(),
                base_branch: None,
                working_branch: None,
            },
            artifacts: Artifacts {
                diff_path: "/test/diff".into(),
                context_path: None,
                test_log_path: None,
                build_log_path: None,
            },
            focus: vec![],
            constraints: vec![],
            budget: None,
            permission_level: "L1_DIFF_REVIEW".into(),
            output_schema: "review_result_v1".into(),
        }
    }

    #[test]
    fn valid_input_passes() {
        assert!(valid_input().validate().is_ok());
    }

    #[test]
    fn rejects_wrong_schema_version() {
        let mut input = valid_input();
        input.schema_version = "wrong".into();
        let err = input.validate().unwrap_err();
        assert!(err.message.contains("schema_version"));
    }

    #[test]
    fn rejects_empty_goal() {
        let mut input = valid_input();
        input.goal = "".into();
        assert!(input.validate().is_err());
    }

    #[test]
    fn rejects_empty_repo_root() {
        let mut input = valid_input();
        input.repo.root = "".into();
        assert!(input.validate().is_err());
    }

    #[test]
    fn rejects_empty_diff_path() {
        let mut input = valid_input();
        input.artifacts.diff_path = "".into();
        assert!(input.validate().is_err());
    }

    #[test]
    fn rejects_wrong_permission_level() {
        let mut input = valid_input();
        input.permission_level = "L0_READONLY".into();
        assert!(input.validate().is_err());
    }

    #[test]
    fn rejects_wrong_output_schema() {
        let mut input = valid_input();
        input.output_schema = "wrong".into();
        assert!(input.validate().is_err());
    }

    #[test]
    fn auto_generates_task_id() {
        let input = ReviewDiffInput {
            task_id: None,
            ..valid_input()
        };
        // Should still validate (task_id is optional in schema)
        assert!(input.validate().is_ok());
    }

    // ── Output validation ──

    #[test]
    fn valid_pure_review_passes() {
        let review = PureReviewResult {
            verdict: "pass".into(),
            summary: "No issues.".into(),
            findings: vec![],
            tests_to_run: vec![],
            risks: vec![],
            assumptions: vec![],
            confidence: 0.95,
        };
        assert!(review.validate().is_ok());
    }

    #[test]
    fn rejects_invalid_verdict() {
        let review = PureReviewResult {
            verdict: "invalid".into(),
            summary: "ok".into(),
            findings: vec![],
            tests_to_run: vec![],
            risks: vec![],
            assumptions: vec![],
            confidence: 0.5,
        };
        assert!(review.validate().is_err());
    }

    #[test]
    fn rejects_empty_summary() {
        let review = PureReviewResult {
            verdict: "pass".into(),
            summary: "".into(),
            findings: vec![],
            tests_to_run: vec![],
            risks: vec![],
            assumptions: vec![],
            confidence: 0.5,
        };
        assert!(review.validate().is_err());
    }

    #[test]
    fn rejects_confidence_out_of_range() {
        let review = PureReviewResult {
            verdict: "pass".into(),
            summary: "ok".into(),
            findings: vec![],
            tests_to_run: vec![],
            risks: vec![],
            assumptions: vec![],
            confidence: 2.0,
        };
        assert!(review.validate().is_err());
    }

    #[test]
    fn all_valid_verdicts_accepted() {
        for verdict in &["pass", "needs_fix", "risky", "unknown", "not_applicable"] {
            let review = PureReviewResult {
                verdict: verdict.to_string(),
                summary: "ok".into(),
                findings: vec![],
                tests_to_run: vec![],
                risks: vec![],
                assumptions: vec![],
                confidence: 0.5,
            };
            assert!(review.validate().is_ok(), "verdict '{}' should be valid", verdict);
        }
    }

    // ── Wrapper serialization ──

    #[test]
    fn wrapper_serializes_correct_structure() {
        let review = PureReviewResult::mock_pass();
        let wrapper = CoagentReviewWrapper {
            review: review.clone(),
            metadata: ReviewMetadata {
                schema_version: "review_result_v1".into(),
                task_id: "TASK-1".into(),
                request_id: "REQ-1".into(),
                status: "ok".into(),
                operation: "reasonix.review_diff".into(),
                runtime_decision: "allow".into(),
            },
        };
        let json = serde_json::to_string(&wrapper).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["review"]["verdict"], "pass");
        assert_eq!(parsed["metadata"]["task_id"], "TASK-1");
        assert_eq!(parsed["metadata"]["status"], "ok");
    }
}
