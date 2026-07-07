use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

/// Execution sandbox configuration: controls what a task can access.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(Default)]
pub struct SandboxConfig {
    /// Working directory for the task (relative to repo root or absolute).
    pub working_directory: Option<PathBuf>,

    /// Allowed environment variable names (empty = deny all).
    pub env_allowlist: Vec<String>,

    /// Forbidden environment variable names (checked before allowlist).
    pub env_denylist: Vec<String>,

    /// Resource budgets. None = no limit.
    pub budgets: ResourceBudgets,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceBudgets {
    /// Maximum wall-clock duration for a single backend invocation.
    pub max_wall_clock: Option<Duration>,

    /// Maximum stdout/stderr bytes from backend.
    pub max_output_bytes: Option<u64>,

    /// Maximum token budget (if applicable; advisory only).
    pub max_tokens: Option<u64>,

    /// Maximum CPU time (advisory, not always enforceable).
    pub max_cpu_time: Option<Duration>,
}

impl Default for ResourceBudgets {
    fn default() -> Self {
        Self {
            max_wall_clock: Some(Duration::from_secs(120)),
            max_output_bytes: Some(10 * 1024 * 1024), // 10 MB
            max_tokens: None,
            max_cpu_time: None,
        }
    }
}


impl SandboxConfig {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the working directory for backend execution.
    pub fn with_working_directory(mut self, dir: impl Into<PathBuf>) -> Self {
        self.working_directory = Some(dir.into());
        self
    }

    /// Allow specific environment variables to pass through to backend.
    pub fn with_env_allowlist(mut self, vars: Vec<String>) -> Self {
        self.env_allowlist = vars;
        self
    }

    /// Block specific environment variables.
    pub fn with_env_denylist(mut self, vars: Vec<String>) -> Self {
        self.env_denylist = vars;
        self
    }

    /// Set resource budgets.
    pub fn with_budgets(mut self, budgets: ResourceBudgets) -> Self {
        self.budgets = budgets;
        self
    }

    /// Check if an environment variable is allowed.
    pub fn env_allowed(&self, name: &str) -> bool {
        if self
            .env_denylist
            .iter()
            .any(|d| d.eq_ignore_ascii_case(name))
        {
            return false;
        }
        if self.env_allowlist.is_empty() {
            return false; // empty allowlist = deny all
        }
        self.env_allowlist
            .iter()
            .any(|a| a.eq_ignore_ascii_case(name))
    }

    /// Produce the set of environment variables to pass to the backend.
    /// Applies the allowlist/denylist filter over the actual current env.
    pub fn filtered_env(&self) -> Vec<(String, String)> {
        let allow_set: HashSet<String> = self
            .env_allowlist
            .iter()
            .map(|s| s.to_lowercase())
            .collect();
        let deny_set: HashSet<String> =
            self.env_denylist.iter().map(|s| s.to_lowercase()).collect();

        std::env::vars()
            .filter(|(k, _)| {
                let lower = k.to_lowercase();
                if deny_set.contains(&lower) {
                    return false;
                }
                if allow_set.is_empty() {
                    return false;
                }
                allow_set.contains(&lower)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_allowlist_denies_everything() {
        let config = SandboxConfig::default();
        assert!(!config.env_allowed("PATH"));
        assert!(!config.env_allowed("HOME"));
    }

    #[test]
    fn denylist_overrides_allowlist() {
        let config = SandboxConfig::new()
            .with_env_allowlist(vec!["PATH".into(), "HOME".into()])
            .with_env_denylist(vec!["HOME".into()]);
        assert!(config.env_allowed("PATH"));
        assert!(!config.env_allowed("HOME"));
    }

    #[test]
    fn default_budgets_have_sane_limits() {
        let budgets = ResourceBudgets::default();
        assert_eq!(budgets.max_wall_clock, Some(Duration::from_secs(120)));
        assert_eq!(budgets.max_output_bytes, Some(10 * 1024 * 1024));
        assert_eq!(budgets.max_tokens, None);
    }

    #[test]
    fn filtered_env_respects_allowlist() {
        unsafe { std::env::set_var("COAGENT_TEST_VAR", "value") };
        let config =
            SandboxConfig::new().with_env_allowlist(vec!["COAGENT_TEST_VAR".into(), "PATH".into()]);
        let env = config.filtered_env();
        assert!(env.iter().any(|(k, _)| k == "COAGENT_TEST_VAR"));
        assert!(!env.iter().any(|(k, _)| k == "HOME"));
    }
}
