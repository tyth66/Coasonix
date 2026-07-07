---
name: coagent
description: Use Coagent to review code diffs through a gated MCP tool with audit and policy enforcement. Use when preparing diffs for review, running reasoned code reviews, interpreting Coagent review results, setting up Coagent MCP server registration, or debugging Coagent review failures.
---

# Coagent -- Gated Code Diff Review

Coagent is an MCP server that provides reasonix.review_diff -- a gated, audited code review tool. It sits between Codex and the Reasonix review backend, enforcing policy, recording audit trails, and ensuring safe execution.

## Quick Start

### Register Coagent with Codex

`powershell
# Build first
cargo build -p coagent-mcp-server

# Register (mock backend for fast local reviews)
codex mcp add coagent   --env COAGENT_REPO_ROOT=D:/your-repo   --env COAGENT_BACKEND=mock   -- D:/Coagent/target/debug/coagent-mcp-server.exe

# Or with real Reasonix backend
codex mcp add coagent   --env COAGENT_REPO_ROOT=D:/your-repo   --env COAGENT_BACKEND=reasonix   --env COAGENT_REASONIX_MODEL=deepseek-v4-flash   -- D:/Coagent/target/debug/coagent-mcp-server.exe
`

### Verify Registration

`powershell
codex mcp list coagent
`

## Review Workflow

### Step 1: Prepare the Diff

Before calling reasonix.review_diff, prepare the diff artifact:

`powershell
# Generate diff of staged + unstaged changes
git diff HEAD > .agent/diffs/current.diff

# Or diff against a specific base
git diff main...HEAD > .agent/diffs/feature.diff
`

The diff file must be under .agent/diffs/ (within the repo root) to pass Coagent path policy.

### Step 2: Call reasonix.review_diff

Use the reasonix.review_diff MCP tool with these parameters:

| Parameter | Required | Description |
|-----------|----------|-------------|
| schema_version | yes | "review_diff_input_v1" |
| goal | yes | What to review |
| repo.root | yes | Absolute path to repo root |
| artifacts.diff_path | yes | Path to diff file (under .agent/diffs/) |
| permission_level | yes | "L1_DIFF_REVIEW" |
| output_schema | yes | "review_result_v1" |
| focus | no | Specific areas to focus on |
| constraints | no | E.g. "ignore formatting changes" |
| budget | no | { max_minutes, max_output_chars, max_steps } |

### Step 3: Interpret Results

The response is a CoagentReviewWrapper with verdict, findings, risks, and metadata.

**Verdicts:**

| verdict | action |
|---------|--------|
| pass | No changes needed |
| needs_fix | Address findings before proceeding |
| risky | Proceed with caution |
| unknown | Manual review needed |
| not_applicable | Diff cannot be reviewed |

**Response statuses:**

| status | meaning |
|--------|---------|
| ok | Review completed successfully |
| approval_required | Tool requires approval before execution |
| runtime_policy_denied | Diff path or permissions blocked by policy |
| worker_schema_invalid | Review output did not match expected schema |
| worker_unavailable | Reasonix backend failed or timed out |

Run tests_to_run from the response to validate changes before merging.

## Path Policy

Coagent enforces a filesystem sandbox. Diffs and context files must be under:

- .agent/diffs/** -- diff files
- .agent/context/** -- additional context files
- .agent/logs/** -- test/build logs
- docs/**, crates/**, packages/**, schemas/** -- readable source

Results write to .agent/results/**.

Blocked: .agent/secrets/**, .git/**.

## Audit Trail

Every review creates an audit record in .agent/coagent.sqlite (SQLite, WAL mode). The audit is append-only; records cannot be modified or deleted.

## Environment Variables

| variable | default | description |
|----------|---------|-------------|
| COAGENT_REPO_ROOT | (required) | repo root for path resolution |
| COAGENT_BACKEND | (tool default) | mock or reasonix |
| COAGENT_REASONIX_MODEL | deepseek-v4-flash | model for Reasonix backend |
| COAGENT_REASONIX_PATH | reasonix | path to Reasonix CLI |
| COAGENT_AGENT_TIMEOUT_MS | 120000 | backend timeout in ms |

## Troubleshooting

**runtime_policy_denied**: Check that diff paths are under .agent/diffs/ and relative to COAGENT_REPO_ROOT.

**worker_unavailable**: Reasonix backend not reachable. Verify COAGENT_REASONIX_PATH and model credentials. Use COAGENT_BACKEND=mock for testing.

**worker_schema_invalid**: The review backend returned malformed JSON. Check backend logs.

**Empty result**: Ensure git diff actually produced output. Use git diff HEAD for staged + unstaged, or git diff --staged for staged only.

See [REFERENCE.md](references/reference.md) for the full reasonix.review_diff input/output schema.
