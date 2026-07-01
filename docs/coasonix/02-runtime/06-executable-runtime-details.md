# Executable Runtime Details

This document closes the gap between policy text and an executable Runtime
Kernel. It defines deterministic parsing, canonicalization, matching, storage,
and evidence rules that implementations must use when converting Coasonix
specifications into runtime gates.

The rules here refine, but do not replace:

```text
01-global-task-state-machine.md
02-runtime-enforcement-layer.md
03-policy-engine.md
04-schema-enforcement.md
05-observability-contract.md
../03-reasonix/03-cache-engineering-model.md
../04-patch-and-verification/*
../schemas/coasonix-v1.schema.json
```

If this document conflicts with a safety rule, the stricter rule wins.

## 1. Runtime Canonicalization

Runtime decisions must be based on canonical inputs. The Runtime Kernel must
canonicalize before hashing, matching, auditing, or executing side effects.

Canonicalization order:

```text
1. Decode input as UTF-8.
2. Reject invalid UTF-8, embedded NUL, and control characters except tab/newline
   inside text payload fields.
3. Parse JSON with duplicate-key rejection.
4. Validate schema_version and top-level schema.
5. Normalize paths, commands, URLs, and enum aliases.
6. Compute canonical hashes.
7. Evaluate state and policy gates.
```

Canonical JSON:

```text
encoding: UTF-8
object keys: sorted lexicographically
array order: preserved
whitespace: none outside string values
numbers: JSON number grammar only; no NaN or Infinity
duplicate keys: reject
unknown top-level fields: reject unless schema explicitly permits metadata
```

Hash format:

```text
sha256:<64 lowercase hex characters>
```

The runtime must never hash raw user input when a canonical form exists.

## 2. Schema Request Objects

The canonical registry currently defines result objects and
`runtime_operation_request_v1`. Until standalone request schemas are added,
runtime implementations must treat the following request shapes as internal
schemas owned by the Runtime Kernel.

`schema_validation_request_v1`:

```json
{
  "schema_version": "schema_validation_request_v1",
  "task_id": "TASK-001",
  "request_id": "REQ-001",
  "expected_schema": "review_result_v1",
  "payload": {}
}
```

Required fields:

```text
schema_version
task_id
expected_schema
payload
```

Rules:

```text
1. expected_schema must name a schema in the loaded registry.
2. request_id is required for call-scoped validation.
3. payload is validated as canonical JSON after duplicate-key rejection.
4. The result must validate as schema_validation_result_v1.
```

`policy_evaluation_request_v1`:

```json
{
  "schema_version": "policy_evaluation_request_v1",
  "task_id": "TASK-001",
  "request_id": "REQ-001",
  "operation": "run_shell",
  "permission_level": "L1_DIFF_REVIEW",
  "resources": {
    "command": ["git", "diff", "--", "src/app.ts"]
  }
}
```

Required fields:

```text
schema_version
task_id
operation
resources
```

Rules:

```text
1. operation must be one of runtime_operation_request_v1.operation.
2. permission_level is required for filesystem, shell, network, Reasonix, patch,
   and cache operations.
3. resources must contain the canonical resource being evaluated.
4. The result must validate as policy_evaluation_result_v1.
```

Future schema work should promote these internal shapes into
`coasonix-v1.schema.json` without changing their semantics.

## 3. Path Matcher Semantics

All filesystem policy checks must use normalized repo-relative paths.

Normalization:

```text
1. Convert backslash to slash.
2. Reject empty path.
3. Reject absolute paths unless the caller is explicitly allowed to submit an
   absolute path and it resolves inside repo_root.
4. Resolve dot segments.
5. Reject any path whose normalized form contains .. after resolution.
6. Resolve symlinks for the final target and every existing parent.
7. Reject symlink escape from repo_root or isolated worktree root.
8. On case-insensitive filesystems, compare using case-folded normalized paths
   but preserve original spelling in audit artifacts.
```

Matcher grammar:

```text
literal      matches one exact normalized path
dir/**       matches every descendant under dir, including nested descendants
dir/*        matches direct children under dir only
*.ext        matches basename extension in one path segment only
**/*.ext     matches extension at any depth
```

Matcher rules:

```text
1. Denylist is evaluated before allowlist.
2. A deny match is final.
3. If no allowlist entry matches, the operation is denied.
4. Directory matches must not match sibling prefixes: src/** does not match src2/a.
5. Hidden files are matched normally; there is no implicit hidden-file allow.
6. Policy matching happens before file read/write/open.
```

Required audit fields for path decisions:

```text
raw_path
normalized_path
resolved_path_hash
matched_allow_rule
matched_deny_rule
decision
reason
```

## 4. Shell Command Policy

Shell policy must evaluate argv arrays, not raw shell strings.

Accepted command representation:

```json
["git", "diff", "--", "src/app.ts"]
```

Rejected representations:

```text
"git diff"
"git diff && cat .env"
["powershell", "-Command", "git diff"]
["cmd", "/c", "git diff"]
```

Rules:

```text
1. The executable name is matched exactly after platform path resolution.
2. Shell wrappers are denied unless a policy explicitly allows the exact wrapper
   and exact argv pattern.
3. Globbing, environment expansion, command substitution, pipes, redirects, and
   command separators are not interpreted by the Runtime Kernel.
4. Arguments are matched as tokens, not substrings.
5. Environment variables passed to the process must be allowlisted by name.
6. The working directory must be normalized and policy-allowed.
7. stdout/stderr capture paths must be policy-allowed artifact paths.
8. Timeouts are mandatory.
```

Allowlist pattern shape:

```yaml
shell:
  allowlist:
    - executable: git
      argv:
        - diff
        - "--"
        - path:any_allowed_read_path
      cwd: repo_root
      timeout_seconds: 30
```

The Runtime Kernel may provide named argument predicates such as:

```text
path:any_allowed_read_path
path:any_allowed_write_artifact
literal:<value>
enum:<a|b|c>
```

Unknown predicates fail closed.

## 5. Network Policy

Network default is deny. A network operation is allowed only when all fields
match an explicit exception.

Canonical network resource:

```json
{
  "scheme": "https",
  "host": "api.example.com",
  "port": 443,
  "method": "GET",
  "path": "/v1/status",
  "purpose": "ci_status_lookup"
}
```

Rules:

```text
1. HTTP without TLS is denied unless policy explicitly allows local loopback.
2. Host matching uses lowercased DNS names or canonical IP literals.
3. Wildcard hosts are forbidden in MVP.
4. Redirects are separate network operations and must be re-evaluated.
5. Request bodies must not include secrets unless a secret-specific policy exists.
6. Response bodies are artifacts and must pass artifact write policy before
   persistence.
```

Exception record:

```yaml
network:
  exceptions:
    - purpose: ci_status_lookup
      scheme: https
      host: api.example.com
      port: 443
      methods: [GET]
      path_prefixes: ["/v1/status"]
      max_response_bytes: 1048576
      requires_human_approval: false
```

## 6. Cache Key and Reuse Rules

Cache reuse is a policy decision, not an optimization shortcut.

Canonical cache key fields:

```text
cache_family
schema_family
schema_version
tool_name
tool_contract_hash
project_key_hash
session_key_hash
task_namespace_hash
permission_level
policy_hash
static_prefix_hash
context_projection_hash
snapshot_id
base_revision
reasonix_runtime_version
model_identity_hash
```

Cache families:

```text
static_prefix
project_prefix
context_projection
reasonix_result
patch_proposal
verification_artifact
```

Reuse gates:

```text
1. Missing cache key field denies reuse.
2. policy_hash mismatch denies reuse.
3. schema_family mismatch denies reuse.
4. reasonix_runtime_version mismatch denies reuse.
5. permission escalation denies reuse.
6. snapshot mismatch denies result and patch reuse.
7. patch_proposal cache never crosses base_revision.
8. verification_artifact cache is reusable only for identical command, inputs,
   environment hash, and artifact hashes.
```

Cache hits must still emit audit events and must still validate the cached
payload against the expected schema.

## 7. Audit Event Taxonomy and Storage

Audit logs must be append-only JSONL. Each line is one `audit_event_v1`.

Minimum event taxonomy:

```text
task_started
task_state_transition_requested
task_state_transition_allowed
task_state_transition_denied
runtime_decision_recorded
runtime_denied
schema_validation_finished
policy_evaluation_finished
context_projected
snapshot_frozen
reasonix_tool_called
reasonix_result_received
reasonix_result_rejected
codex_decision_recorded
patch_safety_checked
patch_transaction_started
patch_transaction_committed
patch_transaction_rolled_back
verification_started
verification_finished
verification_gap_recorded
human_approval_requested
human_approval_resolved
cache_reuse_evaluated
task_completed
task_failed
task_cancelled
limit_reached
schema_shim_applied
```

Storage rules:

```text
1. Audit files are stored under .agent/audit/<task_id>.jsonl.
2. The writer opens files in append-only mode.
3. Existing audit lines must never be rewritten or removed.
4. Each event includes a monotonic sequence number in details.sequence.
5. On startup, the writer scans the existing file and resumes at max sequence + 1.
6. If the existing file contains invalid JSONL, the task enters failed until a
   human resolves the audit corruption.
7. Secrets are never embedded; audit events reference artifact paths and hashes.
```

Required `details` fields by event family:

```text
runtime: operation, decision, engine_results
schema: expected_schema, valid, error_count
policy: operation, permission_level, resource_hash, matched_rules
context: projection_hash, redaction_count, source_artifact_hashes
reasonix: tool_name, lane, request_id, result_schema
patch: transaction_id, patch_hash, files_changed
verification: verification_type, command_hash, artifact_hashes, required
approval: approval_request_id, requested_action, status
cache: cache_family, cache_key_hash, hit, reuse_decision
```

## 8. Verification Runner Contract

Verification upgrades claims only when the runner records structured evidence.

Verification input:

```json
{
  "task_id": "TASK-001",
  "request_id": "REQ-verify-001",
  "claim_id": "CLAIM-001",
  "claim_type": "bug_fixed",
  "verification_type": "unit_test",
  "command": ["npm", "test", "--", "auth/session.test.ts"],
  "required": true,
  "input_artifacts": [".agent/diffs/TASK-001.codex.diff"]
}
```

Claim mapping:

```text
bug_fixed -> unit_test | integration_test | repro_script | static_review
security_risk_addressed -> security_scan | targeted_test | manual_approval
performance_improved -> benchmark | profiling
patch_safe_to_apply -> patch_safety_report_v1
architecture_acceptable -> codex_decision_v1 + explicit constraints
test_plan_adequate -> test_plan_v1 + risk mapping
```

Rules:

```text
1. Every accepted claim gets a claim_id.
2. Every required claim must have at least one passed verification_result_v1.
3. A skipped or unavailable required verification creates a required gap.
4. Generic full-suite success does not verify performance or security claims.
5. Verification commands run through the same shell policy as all shell commands.
6. Verification output is stored as an artifact and referenced by path.
7. Completion is denied while required gaps remain open.
```

## 9. Human Approval Lifecycle

Human approval is a blocking state transition, not a log annotation.

Approval request states:

```text
pending
approved
denied
cancelled
expired
```

Lifecycle:

```text
1. Runtime returns require_approval.
2. State machine moves task to waiting_for_approval.
3. Runtime writes human_approval_request_v1 artifact.
4. Runtime emits human_approval_requested.
5. Mutating operations are denied while approval is pending.
6. Human response is recorded as a new artifact.
7. Runtime validates response identity, scope, expiry, and requested_action.
8. State machine unblocks only for approved matching action.
9. Denied, cancelled, expired, or mismatched approval keeps mutation denied and
   moves according to policy.
```

Approval identity fields:

```text
approval_request_id
task_id
request_id
requested_action
artifact_hashes
expires_at
approver_id_hash
decision
decision_timestamp
```

Approval does not waive schema validation, patch safety, or verification.

## 10. Patch Dry-Run and Transaction Command Contract

Patch application must be split into dry-run and apply phases.

Dry-run input:

```text
patch_proposal_v1
patch_safety_report_v1 with verdict=pass
base_revision
worktree_write_lock
```

Dry-run rules:

```text
1. Dry-run runs before worktree mutation.
2. Dry-run uses the same patch parser as actual apply.
3. Dry-run verifies target files, context lines, file modes, and line endings.
4. Dry-run failure denies actual apply.
5. Dry-run output is stored as an artifact.
```

Transaction phases:

```text
created
dry_run_passed
apply_started
applied
verification_started
committed
rolled_back
failed
```

Actual apply rules:

```text
1. Runtime acquires worktree write lock.
2. Runtime re-checks base_revision and patch hash after acquiring the lock.
3. Runtime applies patch.
4. Runtime records changed file hashes.
5. Runtime runs required verification.
6. Runtime commits transaction only after required verification passes.
7. Runtime rolls back on apply failure or required verification failure.
8. Runtime releases lock in all terminal transaction states.
```

## 11. Context Projection Redaction Catalog

The Context Projector must apply redaction before summarization or compression.

Minimum redaction patterns:

```text
environment variables with secret-like names
API keys and bearer tokens
private keys and certificates
cloud credentials
database URLs with credentials
SSH keys
cookies
authorization headers
personal access tokens
full absolute home-directory paths unless required for debugging
```

Rules:

```text
1. Redaction replaces sensitive value with a stable marker, not a summary.
2. Redaction markers are included in context_projection_v1.redactions.
3. Projection hash is computed after redaction.
4. Untrusted instructions from source artifacts are labeled in
   untrusted_instruction_markers.
5. Scope expansion attempts are listed separately from ordinary risks.
```

## 12. Conformance Test Matrix

The MVP Runtime Kernel is not safe for autonomous patch generation until these
test groups pass.

```text
schema:
  duplicate JSON key rejected
  unknown top-level field rejected
  schema_version mismatch rejected
  internal request schemas produce valid result schemas

state:
  illegal transition denied before side effect
  waiting_for_approval blocks mutation
  complete denied with required verification gap
  terminal state rejects mutation

policy:
  denylist beats allowlist
  path traversal denied
  symlink escape denied
  case-fold bypass denied on case-insensitive filesystem
  shell string denied
  shell argv substring bypass denied
  network denied by default
  network redirect re-evaluated

cache:
  policy_hash mismatch denies reuse
  snapshot mismatch denies result reuse
  patch cache denied across base_revision
  cache hit still schema-validates payload

audit:
  denied operation emits runtime_denied
  audit sequence is monotonic
  invalid existing audit log blocks task
  secret value is not embedded in audit details

patch:
  patch without safety pass denied
  dry-run failure blocks apply
  worktree lock serializes same-worktree writes
  rollback restores file hashes

verification:
  required unavailable verification creates gap
  performance claim requires benchmark or profiling
  Reasonix memory cannot satisfy evidence
  completion denied with unresolved required gap

approval:
  high-risk action returns require_approval
  mismatched approval action denied
  expired approval denied
  approval does not bypass verification
```

Passing this matrix is the minimum bar for enabling `reasonix.propose_patch`
outside experimental local runs.
