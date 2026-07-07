use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Mutex;

use super::reasonix::{AcpSession, ReasonixRunner};

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// AgentProfile — backend configuration
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Session scoping policy for ACP backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionPolicy {
    /// One session per backend globally.
    PerBackend,
    /// One session per project directory.
    PerProject,
    /// One session per task.
    PerTask,
    /// One session per project+task combination (default).
    PerProjectTask,
}

/// Trust level for a backend — constrains what it is allowed to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustLevel {
    /// Read-only review, no workspace writes.
    ReviewOnly,
    /// Can propose patches but not apply.
    PatchProposal,
    /// Can execute in isolated sandbox.
    IsolatedExecution,
}

/// Configuration profile for an ACP-compatible backend agent.
#[derive(Debug, Clone)]
pub struct AgentProfile {
    /// Unique identifier for this backend instance.
    pub backend_id: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Command to spawn (e.g. "reasonix").
    pub command: String,
    /// Arguments passed to the command (e.g. ["acp", "--model", "deepseek-v4-flash"]).
    pub args: Vec<String>,
    /// Working directory for the spawned process.
    pub cwd: PathBuf,
    /// Capability tags this backend provides.
    pub capabilities: Vec<String>,
    /// Session scoping policy.
    pub session_policy: SessionPolicy,
    /// Trust level for permission gating.
    pub trust_level: TrustLevel,
    /// Timeout for backend invocations in milliseconds.
    pub timeout_ms: u64,
    /// Maximum retry count for recoverable errors.
    pub max_retries: u32,
}

impl AgentProfile {
    /// Create the default Reasonix profile.
    pub fn reasonix(cwd: PathBuf, model: &str) -> Self {
        Self {
            backend_id: "reasonix".into(),
            display_name: "Reasonix (DeepSeek)".into(),
            command: "reasonix".into(),
            args: vec!["acp".into(), "--model".into(), model.into()],
            cwd,
            capabilities: vec!["code.review.diff".into()],
            session_policy: SessionPolicy::PerProjectTask,
            trust_level: TrustLevel::ReviewOnly,
            timeout_ms: 180_000,
            max_retries: 1,
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// AcpSessionPool — multi-key session management
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Key for session pool lookup: backend_id + scope identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SessionKey {
    backend_id: String,
    scope: String, // project path or task_id depending on policy
}

/// Pool of ACP sessions, keyed by backend + scope.
pub struct AcpSessionPool {
    sessions: HashMap<SessionKey, AcpSession>,
}

impl AcpSessionPool {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    /// Get or create a session for the given profile and scope.
    pub async fn get_or_create(
        &mut self,
        profile: &AgentProfile,
        scope: &str,
        model: &str,
    ) -> Result<&mut AcpSession, String> {
        let key = SessionKey {
            backend_id: profile.backend_id.clone(),
            scope: scope.to_string(),
        };

        if !self.sessions.contains_key(&key) {
            let session = AcpSession::connect(model, &profile.cwd)
                .await
                .map_err(|e| format!("session connect: {e}"))?;
            self.sessions.insert(key.clone(), session);
        }

        self.sessions
            .get_mut(&key)
            .ok_or_else(|| "session not found after insert".to_string())
    }

    /// Remove and drop a session (e.g. on fatal error).
    pub fn invalidate(&mut self, backend_id: &str, scope: &str) {
        let key = SessionKey {
            backend_id: backend_id.to_string(),
            scope: scope.to_string(),
        };
        self.sessions.remove(&key);
    }

    /// Number of active sessions.
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }
}

impl Default for AcpSessionPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasonix_profile_has_expected_defaults() {
        let profile = AgentProfile::reasonix(PathBuf::from("."), "deepseek-v4-flash");
        assert_eq!(profile.backend_id, "reasonix");
        assert!(profile.capabilities.contains(&"code.review.diff".to_string()));
        assert_eq!(profile.trust_level, TrustLevel::ReviewOnly);
        assert_eq!(profile.session_policy, SessionPolicy::PerProjectTask);
    }

    #[test]
    fn session_pool_starts_empty() {
        let pool = AcpSessionPool::new();
        assert_eq!(pool.session_count(), 0);
    }
}
