use serde::{Deserialize, Serialize};

/// Severity of a review finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Blocker,
    Major,
    Minor,
    Note,
}

/// A single finding from a code review. Strongly-typed to ensure
/// Reasonix output is structurally valid before reaching callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub severity: Severity,
    pub category: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<i64>,
    pub issue: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommendation: Option<String>,
    pub confidence: f64,
}

/// Pure review result returned by backends (no system envelope fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PureReviewResult {
    pub verdict: String,
    pub summary: String,
    pub findings: Vec<Finding>,
    pub tests_to_run: Vec<String>,
    pub risks: Vec<String>,
    pub assumptions: Vec<String>,
    pub confidence: f64,
}

impl PureReviewResult {
    pub fn mock_pass() -> Self {
        Self {
            verdict: "pass".into(),
            summary: "Mock runner completed review.".into(),
            findings: vec![],
            tests_to_run: vec![],
            risks: vec![],
            assumptions: vec![],
            confidence: 0.9,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_pass_has_valid_fields() {
        let result = PureReviewResult::mock_pass();
        assert_eq!(result.verdict, "pass");
        assert!(!result.summary.is_empty());
        assert!(result.findings.is_empty());
        assert!((0.0..=1.0).contains(&result.confidence));
    }

    #[test]
    fn mock_pass_is_deterministic() {
        let a = PureReviewResult::mock_pass();
        let b = PureReviewResult::mock_pass();
        assert_eq!(a.verdict, b.verdict);
        assert_eq!(a.summary, b.summary);
        assert_eq!(a.confidence, b.confidence);
    }

    #[test]
    fn mock_pass_serializes_to_json() {
        let result = PureReviewResult::mock_pass();
        let json = serde_json::to_string(&result).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["verdict"], "pass");
        assert!(!parsed["summary"].as_str().unwrap().is_empty());
    }
}
