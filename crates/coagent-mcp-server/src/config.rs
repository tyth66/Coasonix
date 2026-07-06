use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub repo_root: PathBuf,
    pub backend: BackendId,
    pub reasonix_model: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendId {
    Mock,
    Reasonix,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let repo_root = required("COAGENT_REPO_ROOT")?;
        let backend = match std::env::var("COAGENT_BACKEND").as_deref() {
            Ok("reasonix") | Ok("Reasonix") => BackendId::Reasonix,
            _ => BackendId::Mock,
        };
        let reasonix_model = std::env::var("COAGENT_REASONIX_MODEL")
            .unwrap_or_else(|_| "deepseek-v4-flash".into());

        Ok(Self {
            repo_root: PathBuf::from(repo_root),
            backend,
            reasonix_model,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("missing required configuration: {0}")]
    Missing(String),
}

fn required(key: &str) -> Result<String, ConfigError> {
    std::env::var(key).map_err(|_| ConfigError::Missing(key.into()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn backend_id_mock_is_not_reasonix() {
        // Pure unit test: no env mutation needed
        assert_ne!(super::BackendId::Mock, super::BackendId::Reasonix);
    }

    #[test]
    fn backend_id_clone_copy() {
        let a = super::BackendId::Mock;
        let b = a;
        assert_eq!(a, b);
    }
}
