# Runtime Enforcement Layer

Current Coagent documentation defines rules, schemas, state machines, policy,
transactions, caching, and observability. The Runtime Enforcement Layer turns
those rules into non-bypassable runtime gates.

This file defines the Coagent execution kernel.

When translating gate rules into executable implementations, also obey
[06-executable-runtime-details.md](06-executable-runtime-details.md). That file defines
canonicalization, path matcher, shell argv parser, network exceptions, cache key,
audit storage, verification runner, human approval lifecycle, and patch dry-run
deterministic details.

## 1. Runtime Positioning

Current v1 implementation status:

```text
Rust RuntimeKernel:                    implemented (kernel/mod.rs)
  - State engine:                      implemented (state/mod.rs)
  - Policy engine:                     implemented (policy/mod.rs)
  - Artifact policy:                   implemented (artifact/mod.rs)
  - SQLite audit storage:              implemented (storage/mod.rs, 10 tables)
  - JSON Schema validation:            implemented (schema/mod.rs)
  - Canonical JSON/path normalization: implemented (canonical/mod.rs)
Rust JSON-RPC stdio Worker:            implemented (coagent-runtime-worker/main.rs)
TypeScript Runtime Worker client:      implemented (RuntimeWorkerClient.ts)
MCP Adapter integration:               implemented (adapter.ts calls runtime.evaluate_operation)
```

MVP deployment:

```text
TypeScript reasonix-expert MCP Adapter is launched by Codex as a local STDIO
MCP server.
Rust Runtime Worker is a managed child process of the adapter (Bun.spawn).
Rust Runtime Core owns the enforceable safety kernel.
Remote Runtime Service / Reasonix worker pool is not part of v1.
```

Architecture impact:

```text
No architecture change. This aligns older wording with the selected Rust Core +
TypeScript Adapter + managed worker boundary.
```

## 2. Runtime Boundary

Runtime Enforcement Layer sits before every high-risk action:

```text
Codex Orchestrator (MCP tools/call)
  -> TypeScript Adapter
      -> Rust Runtime Worker (JSON-RPC 2.0 stdio)
          -> Runtime Enforcement Layer
              -> State Machine (state/mod.rs)
              -> Policy Engine (policy/mod.rs)
              -> Audit Writer (storage/mod.rs)
          -> Allow/Deny decision
  -> Reasonix invocation (only on allow)
```

v1 enforced operations:

```text
reasonix.review_diff (through evaluate_operation)
```

Post-v1 operations (design only):

```text
patch safety check
patch transaction apply
verification completion
human approval unblock
shell command execution
network request
cache result reuse
task completion
```

## 3. Core Principle

```text
Rules are not advisory.
Runtime gates enforce rules.
Fail closed on uncertainty.
```

No module may execute a high-risk action and record it afterward. Audit is
evidence chain, not authorization mechanism.

## 4. Runtime Kernel (Implemented)

The Runtime Kernel combines three engines:

```text
State Machine Engine (state/mod.rs)  -> TaskStateValue: Created -> Running -> Completed/Failed
Policy Engine (policy/mod.rs)        -> PolicyEngine with operation registry + path/argv/network checks
Artifact Policy (artifact/mod.rs)    -> ArtifactPolicy with allowlist/denylist + glob matching
```

The kernel receives an operation request, runs checks, and returns
allow / deny / require_approval / retryable_error / fatal_error.

References:
- `crates/coagent-runtime-core/src/kernel/mod.rs` — RuntimeKernel.evaluate_operation
- `crates/coagent-runtime-core/src/state/mod.rs` — TaskState, TaskStateValue
- `crates/coagent-runtime-core/src/policy/mod.rs` — PolicyEngine.evaluate
- `crates/coagent-runtime-core/src/storage/mod.rs` — RuntimeStore with SQLite

## 5. Runtime Operation Flow (Implemented)

```text
adapter.ts: tools/call received
-> adapter calls runtime.evaluate_operation via RuntimeWorkerClient
-> Rust Worker dispatches to RuntimeKernel.evaluate_operation
-> evaluate_state: checks task terminal state (Completed/Failed -> Deny)
-> policy_engine.evaluate: checks operation registration, permission level,
   path allowlist/denylist, network default-deny
-> merge_decisions: Deny > FatalError > RequireApproval > RetryableError > Allow
-> persist_runtime_decision_with_audit: SQLite transaction (decision + audit event)
-> persist_running_state: Created -> Running transition
-> return RuntimeDecision payload
-> adapter checks decision.decision; only "allow" proceeds to Reasonix
```

If any step fails:

```text
decision = deny or fatal_error
side_effect = not_executed
audit_event = runtime_decision_deny or runtime_decision_fatal_error
```

## 6. Runtime Decision Values (Implemented)

```text
allow               (only this permits side effects)
deny                (final for the requested operation)
require_approval    (post-v1, not yet exercised)
retryable_error
fatal_error
```

See `crates/coagent-runtime-core/src/policy/mod.rs`: `RuntimeDecisionValue`.

## 7. Runtime Operation Types

v1 implemented:

```text
reasonix.review_diff (via evaluate_operation)
```

Post-v1 design:

```text
transition_state
call_reasonix_tool
accept_reasonix_result
freeze_snapshot
apply_patch_transaction
run_verification
complete_task
request_human_approval
resolve_human_approval
read_artifact
write_artifact
run_shell
open_network
reuse_cache_result
```

## 8. State Machine Runtime Engine (Implemented)

### 8.1 Responsibility

State Machine Runtime Engine enforces the task lifecycle.

Implemented in `crates/coagent-runtime-core/src/state/mod.rs`:

```text
TaskStateValue: Created | Running | Completed | Failed
Transitions: Created->Running, Running->Completed, Running->Failed
Terminal: Completed and Failed reject all further transitions
```

`RuntimeKernel.evaluate_state()` checks if a task is in terminal state.
Terminal tasks get `Deny` decisions. Non-existent tasks get a fresh
`Created` state and proceed.

### 8.2 API (Worker-Level)

The Rust Worker exposes this through `runtime.evaluate_operation`:

```json
{
  "method": "runtime.evaluate_operation",
  "params": {
    "task_id": "TASK-001",
    "operation": "reasonix.review_diff",
    "permission_level": "L1_DIFF_REVIEW",
    "resources": {
      "read_paths": [".agent/diffs/current.diff"],
      "write_paths": [],
      "network": false
    }
  }
}
```

Result: runtime_decision_v1 payload with `allow`/`deny` + reasons.

### 8.3 Hard Requirements (Implemented)

```text
1. Transition validation happens before side effects.  -> evaluate_state runs first
2. Illegal transition fails closed.                      -> terminal state -> Deny
3. Terminal states reject mutation operations.           -> Completed/Failed -> Deny
```

Post-v1: `waiting_for_approval` blocking, `complete` verification gaps.

## 9. Schema Enforcement (Current State)

v1 does not run a Runtime Schema Enforcement Layer in the live call path.
The schema file is a **test contract fixture**:

```text
schemas/coagent-v1.schema.json
```

The MCP adapter performs a narrow local `review_result_v1` contract check
in `worker-contract.ts:reviewResultSchemaError()` before returning
`structuredContent`. The Rust `schema/mod.rs` module exists for standalone
validation + duplicate-key detection tests.

Architecture impact:

```text
The architecture path intentionally stays schema-free at Runtime startup;
the fixture is for regression tests and contract documentation.
```

## 10. Policy Execution Engine (Implemented)

### 10.1 Responsibility

Policy Execution Engine turns safety rules into runtime gates.

Implemented in `crates/coagent-runtime-core/src/policy/mod.rs`:

```text
PolicyEngine::review_diff(ArtifactPolicy)    -> registers reasonix.review_diff at L1_DIFF_REVIEW
PolicyEngine::evaluate(RuntimeOperationRequest) -> checks operation, permission, paths, network
ArtifactPolicy                              -> path allowlist/denylist with glob matching
```

### 10.2 Default Policy (Code-Level)

From `kernel/mod.rs` `RuntimeKernel::initialize()`:

```rust
ArtifactPolicy::new(&repo_root)
    .allow_read([
        ".agent/context/**",
        ".agent/diffs/**",
        ".agent/logs/**",
        "docs/**",
        "crates/**",
        "packages/**",
        "schemas/**",
    ])
    .allow_write([".agent/results/**", ".agent/logs/**"])
    .deny([".agent/secrets/**", ".git/**"])
```

### 10.3 Hard Requirements (Implemented)

```text
1. Denylist wins over allowlist.                   -> ArtifactPolicy evaluates deny first
2. Path normalization happens before policy match.  -> canonical/mod.rs
3. Network default is deny.                         -> network=true -> reason added
4. L1_DIFF_REVIEW is the only active level.         -> review_diff factory
```

## 11. Runtime Composition Rules (Implemented)

### 11.1 Ordering

From `RuntimeKernel.evaluate_operation()`:

```text
1. evaluate_state (task_id)          -> allow/deny + reasons
2. policy_engine.evaluate (request)  -> allow/deny + reasons
3. merge_decisions (state, policy)   -> final RuntimeDecisionValue
4. persist_runtime_decision_with_audit  -> SQLite transaction
5. persist_running_state (if allow)  -> Created->Running transition
```

### 11.2 Decision Merge (Implemented)

From `RuntimeKernel::merge_decisions()`:

```text
policy=Deny       -> Deny
any=FatalError    -> FatalError
any=Deny          -> Deny
any=RequireApproval -> RequireApproval
any=RetryableError  -> RetryableError
otherwise         -> Allow
```

### 11.3 Audit (Implemented)

Every runtime decision emits one `audit_event` row with type
`runtime_decision_{allow|deny|fatal_error|...}`.
The `audit_events` table has triggers that reject UPDATE and DELETE.

## 12. Runtime Decision Payload

Actual shape from `kernel/mod.rs` `RuntimeDecision::to_payload()`:

```json
{
  "schema_version": "runtime_decision_v1",
  "task_id": "TASK-001",
  "operation": "reasonix.review_diff",
  "decision": "allow",
  "engine_results": {
    "state": "allow",
    "policy": "allow"
  },
  "reasons": []
}
```

## 13. Runtime Error Codes

Implemented in the Rust worker (`main.rs`):

```text
-32700  Parse error
-32600  Invalid Request
-32601  Method not found
-32602  Invalid params
-32008  runtime_unavailable
-32010  runtime_internal_error
```

Adapter-level error codes (14 codes across 6 layers) are defined in
`error-taxonomy.ts`.

## 14. Framework Status After This Layer

```text
Design direction:              complete
Deterministic runtime spec:    complete (06-executable-runtime-details.md)
Runtime enforcement design:    complete (this document)
v1 runtime implementation:     complete (Rust RuntimeKernel + JSON-RPC Worker + TS client)
Autonomous patch operation:    blocked (requires patch safety, approval, transaction, verification gates)
```

The v1 implementation exercises this runtime contract through:
- `RuntimeKernel::evaluate_operation` (kernel/mod.rs)
- JSON-RPC 2.0 stdio Worker (coagent-runtime-worker/main.rs)
- TypeScript RuntimeWorkerClient (RuntimeWorkerClient.ts)
- MCP adapter integration (adapter.ts)
- Mock `reasonix.review_diff` vertical slice (MockRunner)

It should not be operated in autonomous patch-generating mode until the
post-v1 patch gates and conformance tests pass.
