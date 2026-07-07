use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub repo_root: PathBuf,
    pub backend_override: Option<BackendId>,
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
        let backend_override = match std::env::var("COAGENT_BACKEND") {
            Ok(value) if value.eq_ignore_ascii_case("mock") => Some(BackendId::Mock),
            Ok(value) if value.eq_ignore_ascii_case("reasonix") => Some(BackendId::Reasonix),
            Ok(value) => return Err(ConfigError::InvalidBackend(value)),
            Err(_) => None,
        };
        let reasonix_model =
            std::env::var("COAGENT_REASONIX_MODEL").unwrap_or_else(|_| "deepseek-v4-flash".into());

        Ok(Self {
            repo_root: PathBuf::from(repo_root),
            backend_override,
            reasonix_model,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("missing required configuration: {0}")]
    Missing(String),
    #[error("invalid COAGENT_BACKEND: {0}")]
    InvalidBackend(String),
}

fn required(key: &str) -> Result<String, ConfigError> {
    std::env::var(key).map_err(|_| ConfigError::Missing(key.into()))
}

#[cfg(test)]
mod tests {
    use std::sync::{LazyLock, Mutex};

    static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    struct TestEnv {
        previous: Vec<(&'static str, Option<String>)>,
    }

    impl TestEnv {
        fn set(values: &[(&'static str, &str)]) -> Self {
            let previous = values
                .iter()
                .map(|(key, _)| (*key, std::env::var(key).ok()))
                .collect::<Vec<_>>();
            for (key, value) in values {
                unsafe { std::env::set_var(key, value) };
            }
            Self { previous }
        }
    }

    impl Drop for TestEnv {
        fn drop(&mut self) {
            for (key, value) in &self.previous {
                if let Some(value) = value {
                    unsafe { std::env::set_var(key, value) };
                } else {
                    unsafe { std::env::remove_var(key) };
                }
            }
        }
    }

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

    #[test]
    fn invalid_backend_override_is_rejected() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let _env = TestEnv::set(&[
            ("COAGENT_REPO_ROOT", "."),
            ("COAGENT_BACKEND", "definitely-not-a-backend"),
        ]);

        let error = super::Config::from_env().expect_err("invalid backend must fail closed");

        assert!(matches!(error, super::ConfigError::InvalidBackend(_)));
        assert!(error.to_string().contains("definitely-not-a-backend"));
    }
}
