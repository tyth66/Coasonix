# reasonix.review_diff -- Full Schema Reference

## Input Schema (review_diff_input_v1)

`json
{
  "schema_version": "review_diff_input_v1",
  "task_id": "TASK-optional-uuid",
  "request_id": "REQ-optional-uuid",
  "mode": "review_diff",
  "goal": "Review this authentication refactor for security issues",
  "repo": {
    "root": "D:/my-repo",
    "base_branch": "main",
    "working_branch": "feature/auth-refactor"
  },
  "artifacts": {
    "diff_path": ".agent/diffs/current.diff",
    "context_path": ".agent/context/README.md",
    "test_log_path": ".agent/logs/test-output.log",
    "build_log_path": ".agent/logs/build-output.log"
  },
  "focus": ["authentication", "session management"],
  "constraints": ["ignore whitespace changes", "focus on security"],
  "budget": {
    "max_minutes": 5,
    "max_output_chars": 5000,
    "max_steps": 10
  },
  "permission_level": "L1_DIFF_REVIEW",
  "output_schema": "review_result_v1"
}
`

### Field Details

| field | type | required | notes |
|-------|------|----------|-------|
| schema_version | const "review_diff_input_v1" | yes | Must be exact |
| task_id | string | no | Auto-generated UUID if omitted |
| request_id | string | no | Auto-generated UUID if omitted |
| mode | const "review_diff" | no | Only valid mode |
| goal | string (minLength 1) | yes | What to review |
| repo.root | string (minLength 1) | yes | Absolute path |
| repo.base_branch | string | no | For context |
| repo.working_branch | string | no | For context |
| artifacts.diff_path | string (minLength 1) | yes | Under .agent/diffs/ |
| artifacts.context_path | string | no | Additional context |
| artifacts.test_log_path | string | no | Test output |
| artifacts.build_log_path | string | no | Build output |
| focus | string[] | no | Areas to emphasize |
| constraints | string[] | no | Review constraints |
| budget.max_minutes | integer | no | Time limit |
| budget.max_output_chars | integer | no | Output limit |
| budget.max_steps | integer | no | Step limit |
| permission_level | const "L1_DIFF_REVIEW" | yes | Only valid level |
| output_schema | const "review_result_v1" | yes | Must be exact |

## Output Schema (coagent_review_wrapper_v1)

`json
{
  "review": {
    "verdict": "needs_fix",
    "summary": "Two security issues found in session handling",
    "findings": [
      {
        "id": "F-001",
        "severity": "blocker",
        "category": "security",
        "file": "src/auth/session.rs",
        "line": 142,
        "issue": "Session token not invalidated on logout",
        "evidence": "The logout handler calls session.clear() but does not revoke the JWT.",
        "recommendation": "Add token to a revocation list or use short-lived tokens with refresh.",
        "confidence": 0.95
      }
    ],
    "tests_to_run": [
      "cargo test -p auth --test session_lifecycle",
      "cargo test -p auth --test token_revocation"
    ],
    "risks": [
      "Token reuse possible within expiry window if revocation not added"
    ],
    "assumptions": [
      "JWT implementation uses standard claims",
      "No external session store involved"
    ],
    "confidence": 0.88
  },
  "metadata": {
    "schema_version": "review_result_v1",
    "task_id": "TASK-abc123",
    "request_id": "REQ-xyz789",
    "status": "ok",
    "operation": "reasonix.review_diff",
    "runtime_decision": "allow"
  }
}
`

### Finding Severity Levels

| severity | meaning | action |
|----------|---------|--------|
| blocker | Must fix before merge | Stop, fix immediately |
| major | Should fix before merge | Fix in current PR |
| minor | Nice to fix | Can defer to follow-up |
| note | Observation | No action required |

### Finding Categories

Common categories: security, correctness, performance, style, maintainability, testing, documentation, architecture.

### Confidence Range

0.0 (completely uncertain) to 1.0 (completely certain). Findings with confidence below 0.5 should be treated as suggestions rather than requirements.

## Complete Example: End-to-End Review Session

`powershell
# 1. Prepare
mkdir -p .agent/diffs
git diff origin/main...HEAD > .agent/diffs/my-changes.diff

# 2. Call (via Codex MCP)
# reasonix.review_diff with:
#   goal: "Review my-changes.diff for correctness and style"
#   artifacts.diff_path: ".agent/diffs/my-changes.diff"

# 3. Interpret response
# Check metadata.status == "ok"
# Then review.review.verdict for action
# Address findings by severity: blockers first

# 4. Verify fixes
cargo test --workspace
# Re-run review if substantial changes made

# 5. Clean up
# Audit records remain in .agent/coagent.sqlite
# Results logged in .agent/results/
`
