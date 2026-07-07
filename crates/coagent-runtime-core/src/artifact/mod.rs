use std::{
    fs,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct ArtifactPolicy {
    repo_root: PathBuf,
    repo_root_real: PathBuf,
    read_allow: Vec<String>,
    write_allow: Vec<String>,
    deny: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceAccess {
    Read,
    Write,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactPolicyError {
    PathTraversal,
    OutsideRepo,
    SymlinkEscape,
    DeniedByDenylist,
    NotAllowed,
    InvalidRepoRoot,
}

impl ArtifactPolicy {
    pub fn new(repo_root: impl AsRef<Path>) -> Result<Self, ArtifactPolicyError> {
        let repo_root = repo_root.as_ref().to_path_buf();
        let repo_root_real =
            fs::canonicalize(&repo_root).map_err(|_| ArtifactPolicyError::InvalidRepoRoot)?;
        Ok(Self {
            repo_root,
            repo_root_real,
            read_allow: Vec::new(),
            write_allow: Vec::new(),
            deny: Vec::new(),
        })
    }

    pub fn allow_read<I, S>(mut self, patterns: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.read_allow.extend(
            patterns
                .into_iter()
                .map(|pattern| normalize_pattern(pattern.as_ref())),
        );
        self
    }

    pub fn allow_write<I, S>(mut self, patterns: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.write_allow.extend(
            patterns
                .into_iter()
                .map(|pattern| normalize_pattern(pattern.as_ref())),
        );
        self
    }

    pub fn deny<I, S>(mut self, patterns: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.deny.extend(
            patterns
                .into_iter()
                .map(|pattern| normalize_pattern(pattern.as_ref())),
        );
        self
    }

    pub fn authorize(
        &self,
        access: ResourceAccess,
        requested: impl AsRef<Path>,
    ) -> Result<PathBuf, ArtifactPolicyError> {
        let requested = requested.as_ref();
        reject_traversal(requested)?;

        let candidate = if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            self.repo_root.join(requested)
        };

        if !path_starts_with(&candidate, &self.repo_root) {
            return Err(ArtifactPolicyError::OutsideRepo);
        }

        let relative = relative_under_repo(&candidate, &self.repo_root)?;
        let relative_key = normalize_path_key(&relative);

        if self
            .deny
            .iter()
            .any(|pattern| pattern_matches(pattern, &relative_key))
        {
            return Err(ArtifactPolicyError::DeniedByDenylist);
        }

        let allowlist = match access {
            ResourceAccess::Read => &self.read_allow,
            ResourceAccess::Write => &self.write_allow,
        };
        if !allowlist
            .iter()
            .any(|pattern| pattern_matches(pattern, &relative_key))
        {
            return Err(ArtifactPolicyError::NotAllowed);
        }

        self.reject_symlink_escape(&candidate)?;
        Ok(self.repo_root.join(&relative))
    }

    fn reject_symlink_escape(&self, candidate: &Path) -> Result<(), ArtifactPolicyError> {
        let existing = deepest_existing_path(candidate);
        if let Some(existing) = existing {
            let real = fs::canonicalize(existing).map_err(|_| ArtifactPolicyError::OutsideRepo)?;
            if !path_starts_with(&real, &self.repo_root_real) {
                return Err(ArtifactPolicyError::SymlinkEscape);
            }
        }
        Ok(())
    }
}

fn reject_traversal(path: &Path) -> Result<(), ArtifactPolicyError> {
    for component in path.components() {
        if matches!(component, Component::ParentDir) {
            return Err(ArtifactPolicyError::PathTraversal);
        }
    }
    Ok(())
}

fn deepest_existing_path(path: &Path) -> Option<&Path> {
    let mut current = path;
    loop {
        if current.exists() {
            return Some(current);
        }
        current = current.parent()?;
    }
}

fn normalize_pattern(pattern: &str) -> String {
    normalize_case_key(pattern.replace('\\', "/").trim_start_matches("./"))
}

fn normalize_path_key(path: &Path) -> String {
    normalize_case_key(
        &path
            .components()
            .filter_map(|component| match component {
                Component::Normal(value) => Some(value.to_string_lossy().into_owned()),
                Component::CurDir => None,
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/"),
    )
}

fn pattern_matches(pattern: &str, value: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix("/**") {
        value == prefix || value.starts_with(&format!("{prefix}/"))
    } else {
        value == pattern
    }
}

#[cfg(windows)]
fn path_starts_with(path: &Path, base: &Path) -> bool {
    let path = path.to_string_lossy().replace('\\', "/").to_lowercase();
    let base = base.to_string_lossy().replace('\\', "/").to_lowercase();
    path == base || path.starts_with(&format!("{base}/"))
}

#[cfg(not(windows))]
fn path_starts_with(path: &Path, base: &Path) -> bool {
    path.starts_with(base)
}

#[cfg(windows)]
fn relative_under_repo(path: &Path, base: &Path) -> Result<PathBuf, ArtifactPolicyError> {
    let path_components: Vec<_> = path.components().collect();
    let base_components: Vec<_> = base.components().collect();

    if path_components.len() < base_components.len() {
        return Err(ArtifactPolicyError::OutsideRepo);
    }

    for (path_component, base_component) in path_components.iter().zip(base_components.iter()) {
        if path_component.as_os_str().to_string_lossy().to_lowercase()
            != base_component.as_os_str().to_string_lossy().to_lowercase()
        {
            return Err(ArtifactPolicyError::OutsideRepo);
        }
    }

    Ok(path_components[base_components.len()..]
        .iter()
        .collect::<PathBuf>())
}

#[cfg(not(windows))]
fn relative_under_repo<'a>(path: &'a Path, base: &Path) -> Result<&'a Path, ArtifactPolicyError> {
    path.strip_prefix(base)
        .map_err(|_| ArtifactPolicyError::OutsideRepo)
}

#[cfg(windows)]
fn normalize_case_key(value: &str) -> String {
    value.to_lowercase()
}

#[cfg(not(windows))]
fn normalize_case_key(value: &str) -> String {
    value.to_string()
}
