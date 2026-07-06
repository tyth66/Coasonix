use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use coagent_runtime_core::artifact::{ArtifactPolicy, ArtifactPolicyError, ResourceAccess};

fn temp_repo(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("coagent-{name}-{unique}"));
    fs::create_dir_all(root.join(".agent/diffs")).expect("create agent dirs");
    fs::create_dir_all(root.join(".agent/context")).expect("create context dirs");
    fs::create_dir_all(root.join("docs/public")).expect("create docs dirs");
    fs::create_dir_all(root.join("docs/secrets")).expect("create denied docs dirs");
    root
}

fn create_dir_symlink(link: &Path, target: &Path) {
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(target, link).expect("create symlink");
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).expect("create symlink");
}

#[test]
fn denied_path_blocks_before_read() {
    let repo = temp_repo("deny-before-read");
    let policy = ArtifactPolicy::new(&repo)
        .expect("policy")
        .allow_read([".agent/diffs/**"])
        .deny([".agent/diffs/private/**"]);

    let error = policy
        .authorize(ResourceAccess::Read, ".agent/diffs/private/current.diff")
        .expect_err("denylist wins before read");

    assert_eq!(error, ArtifactPolicyError::DeniedByDenylist);
}

#[test]
fn absolute_path_outside_repo_is_denied() {
    let repo = temp_repo("absolute-outside");
    let outside = temp_repo("outside");
    let policy = ArtifactPolicy::new(&repo)
        .expect("policy")
        .allow_read([".agent/diffs/**"]);

    let error = policy
        .authorize(ResourceAccess::Read, outside.join("leak.txt"))
        .expect_err("outside absolute path denied");

    assert_eq!(error, ArtifactPolicyError::OutsideRepo);
}

#[test]
fn traversal_is_denied() {
    let repo = temp_repo("traversal");
    let policy = ArtifactPolicy::new(&repo)
        .expect("policy")
        .allow_read([".agent/diffs/**"]);

    let error = policy
        .authorize(ResourceAccess::Read, ".agent/diffs/../../secrets.txt")
        .expect_err("path traversal denied");

    assert_eq!(error, ArtifactPolicyError::PathTraversal);
}

#[test]
fn symlink_escape_is_denied() {
    let repo = temp_repo("symlink");
    let outside = temp_repo("symlink-outside");
    fs::write(outside.join("secret.txt"), "secret").expect("write outside file");
    create_dir_symlink(&repo.join(".agent/diffs/link"), &outside);

    let policy = ArtifactPolicy::new(&repo)
        .expect("policy")
        .allow_read([".agent/diffs/**"]);

    let error = policy
        .authorize(ResourceAccess::Read, ".agent/diffs/link/secret.txt")
        .expect_err("symlink escape denied");

    assert_eq!(error, ArtifactPolicyError::SymlinkEscape);
}

#[test]
fn denylist_beats_allowlist() {
    let repo = temp_repo("deny-beats-allow");
    let policy = ArtifactPolicy::new(&repo)
        .expect("policy")
        .allow_read(["docs/**"])
        .deny(["docs/secrets/**"]);

    let error = policy
        .authorize(ResourceAccess::Read, "docs/secrets/plan.md")
        .expect_err("denylist has precedence");

    assert_eq!(error, ArtifactPolicyError::DeniedByDenylist);
}

#[test]
fn allowed_repo_local_path_is_normalized() {
    let repo = temp_repo("allowed");
    let policy = ArtifactPolicy::new(&repo)
        .expect("policy")
        .allow_read([".agent/diffs/**"]);

    let normalized = policy
        .authorize(ResourceAccess::Read, ".agent/diffs/current.diff")
        .expect("allowed repo-local path");

    assert!(normalized.starts_with(&repo));
}

#[cfg(windows)]
#[test]
fn windows_case_folded_repo_path_is_still_repo_local() {
    let repo = temp_repo("windows-case");
    let policy = ArtifactPolicy::new(&repo)
        .expect("policy")
        .allow_read([".agent/diffs/**"]);
    let folded = repo
        .join(".agent/diffs/current.diff")
        .to_string_lossy()
        .to_uppercase();

    let normalized = policy
        .authorize(ResourceAccess::Read, folded)
        .expect("case-folded repo-local path should remain allowed");

    assert!(normalized.starts_with(&repo));
}
