use serde::{Deserialize, Serialize};

/// Pure review result returned by backends (no system envelope fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PureReviewResult {
    pub verdict: String,
    pub summary: String,
    pub findings: Vec<serde_json::Value>,
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
