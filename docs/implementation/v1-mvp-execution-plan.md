# v1 MVP Execution Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the Coasonix v1 MVP from `docs/coasonix/06-roadmap/07-v1-implementation-blueprint.md`, starting with the Rust-gated runtime core and ending at the mock `reasonix.review_diff` vertical slice.

**Architecture:** Rust owns enforceable schema, canonicalization, state, policy, audit, locks, and runtime decisions. TypeScript owns the MCP adapter and process supervision only, and it invokes Reasonix only after Rust returns `allow`.

**Tech Stack:** Rust 2024, Cargo workspace, Bun, TypeScript ESM, JSON-RPC 2.0 over stdio, JSON Schema 2020-12, SQLite.

**Current Status:** M0, M1, M2, M3, M4, M5, and M6 are implemented, reviewed, verified, and committed in separate local phases. Continue with M7 in a later execution pass.

---

## Source Boundaries

Project specifications live under:

```text
docs/coasonix/
```

Implementation plans and code-progress notes live under:

```text
docs/implementation/
```

Runtime source lives under:

```text
crates/
packages/
tests/
```

## Current Execution Scope

This execution pass covers:

```text
M0: repository scaffold
M1: Rust schema and canonicalization foundation
M2: Rust state, artifact path, shell, and minimum policy foundation
M3: Rust SQLite persistence and audit foundation
M4: RuntimeKernel decision merge and persistence composition
M5: Rust JSON-RPC worker
M6: TypeScript worker client
```

It intentionally does not implement MCP, real Reasonix invocation, patch
application, approval UI, remote HTTP, or post-v1 `reasonix.*` tools.

## Task 1: Scaffold Workspaces

**Files:**
- Create: `Cargo.toml`
- Create: `Cargo.lock`
- Create: `.gitignore`
- Create: `crates/coasonix-runtime-core/Cargo.toml`
- Create: `crates/coasonix-runtime-core/src/lib.rs`
- Create: `crates/coasonix-runtime-core/src/schema/mod.rs`
- Create: `crates/coasonix-runtime-core/src/canonical/mod.rs`
- Create: `crates/coasonix-runtime-worker/Cargo.toml`
- Create: `crates/coasonix-runtime-worker/src/main.rs`
- Create: `package.json`
- Create: `bun.lock`
- Create: `packages/reasonix-expert-mcp/package.json`
- Create: `packages/reasonix-expert-mcp/src/index.ts`

- [x] **Step 1: Write scaffold smoke tests**

Add minimal Rust and TypeScript tests that fail because the workspaces do not
exist yet:

```text
cargo test --workspace
bun test
```

Expected before scaffold: Cargo cannot find `Cargo.toml`; Bun has no test
workspace.

- [x] **Step 2: Create minimal workspace files**

Create a Rust workspace with `coasonix-runtime-core` and
`coasonix-runtime-worker`, plus a Bun workspace with `reasonix-expert-mcp`.

- [x] **Step 3: Run scaffold verification**

Run:

```text
cargo test --workspace
bun test
python -m json.tool schemas/coasonix-v1.schema.json
```

Expected: all commands exit 0.

## Task 2: Schema Registry and Duplicate-Key Rejection

**Files:**
- Modify: `crates/coasonix-runtime-core/src/lib.rs`
- Modify: `crates/coasonix-runtime-core/src/schema/mod.rs`
- Test: `crates/coasonix-runtime-core/tests/schema_registry.rs`

- [x] **Step 1: Write failing schema tests**

Test behaviors:

```text
schema registry loads schemas/coasonix-v1.schema.json
valid review_diff_input_v1 validates
valid review_result_v1 validates
valid error_result_v1 validates
runtime_decision_v1 validates
schema_validation_result_v1 validates
wrong schema_version fails
unknown expected schema fails closed
output_schema mismatch fails
unexpected top-level field fails
duplicate JSON key fails before schema validation
malformed JSON returns an error without panic
```

- [x] **Step 2: Verify tests fail**

Run:

```text
cargo test -p coasonix-runtime-core schema_registry -- --nocapture
```

Expected: tests fail because `SchemaRegistry` does not exist.

- [x] **Step 3: Implement minimal schema registry**

Implement:

```text
SchemaRegistry::load_from_path
SchemaRegistry::validate
parse_json_no_duplicate_keys
SchemaValidationResult
SchemaValidationError
```

- [x] **Step 4: Verify schema tests pass**

Run:

```text
cargo test -p coasonix-runtime-core schema_registry -- --nocapture
```

Expected: all schema registry tests pass.

## Task 3: Canonical JSON and Hashing

**Files:**
- Modify: `crates/coasonix-runtime-core/src/canonical/mod.rs`
- Test: `crates/coasonix-runtime-core/tests/canonical_json.rs`

- [x] **Step 1: Write failing canonicalization tests**

Test behaviors:

```text
object keys are sorted deterministically
equivalent object key order produces identical canonical_hash
different payload content produces different canonical_hash
arrays preserve order
non-finite numbers do not enter serde_json::Value
```

- [x] **Step 2: Verify tests fail**

Run:

```text
cargo test -p coasonix-runtime-core canonical_json -- --nocapture
```

Expected: tests fail because canonicalization functions do not exist.

- [x] **Step 3: Implement minimal canonicalization**

Implement:

```text
canonical_json
canonical_hash
```

Use SHA-256 and prefix hashes as `sha256:<hex>`.

- [x] **Step 4: Verify canonicalization tests pass**

Run:

```text
cargo test -p coasonix-runtime-core canonical_json -- --nocapture
```

Expected: all canonical JSON tests pass.

## Task 4: M0/M1 Review and Documentation Update

**Files:**
- Modify: `docs/implementation/v1-mvp-execution-plan.md`
- Modify if needed: `docs/coasonix/README.md`

- [x] **Step 1: Run full verification**

Run:

```text
cargo test --workspace
bun test
python -m json.tool schemas/coasonix-v1.schema.json
git status --short
```

- [x] **Step 2: Review M0/M1 against blueprint**

Check:

```text
M0 scaffold exists
M1 schema tests cover duplicate keys
M1 canonical tests cover stable hashes
no MCP or Reasonix integration was added early
project docs and implementation docs remain separated
```

- [x] **Step 3: Fix any review findings**

Do not proceed to M2 while Critical or Important review issues remain.

- [x] **Step 4: Update implementation plan checkboxes**

Mark only completed steps. Do not mark future milestones complete.

### M0/M1 Completion Record

Fresh verification after review fixes:

```text
cargo test --workspace
  coasonix-runtime-core: 1 smoke, 5 canonical, 13 schema registry tests passed
  coasonix-runtime-worker: 0 tests, binary scaffold compiled

bun test
  packages/reasonix-expert-mcp/src/index.test.ts passed

python -m json.tool schemas/coasonix-v1.schema.json > $null
  exited 0

cargo fmt --all -- --check
  exited 0
```

Review outcome:

```text
M0/M1 independent review initially requested changes for repository hygiene and
missing M1 schema coverage. Fixes added .gitignore, Cargo.lock, bun.lock,
expanded schema/canonical tests, and SchemaValidationResult::to_payload.
Re-review approved M0/M1 for documentation update and local commit.
```

Non-blocking notes:

```text
Worker rpc/lifecycle/dispatch source modules are deferred to M5.
Rust 2024 is selected by the blueprint; a rust-toolchain.toml can be added when
CI/MSRV policy is introduced.
```

## Task 5: State, Path, Shell, and Minimum Policy

**Files:**
- Create: `crates/coasonix-runtime-core/src/state/mod.rs`
- Create: `crates/coasonix-runtime-core/src/artifact/mod.rs`
- Create: `crates/coasonix-runtime-core/src/policy/mod.rs`
- Modify: `crates/coasonix-runtime-core/src/lib.rs`
- Test: `crates/coasonix-runtime-core/tests/state_machine.rs`
- Test: `crates/coasonix-runtime-core/tests/artifact_policy.rs`
- Test: `crates/coasonix-runtime-core/tests/policy_engine.rs`

- [x] **Step 1: Write failing M2 tests**

Test behaviors:

```text
illegal state transition denied
terminal state rejects mutation
completion blocked while required verification gaps exist
reasonix_calls increments only through runtime-owned decisions
denied path blocks before read
absolute path outside repo denied
.. traversal denied
symlink escape denied
Windows case-folded repo path remains repo-local
denylist beats allowlist
shell string rejected
argv substring bypass rejected
argv extra-argument bypass rejected
permission mismatch denied
network request denied by default
allowed review_diff policy records command hash
M2 minimum owned types are constructible
```

- [x] **Step 2: Verify tests fail**

Run:

```text
cargo test -p coasonix-runtime-core --tests -- --nocapture
```

Expected before implementation: tests fail because `state`, `artifact`, and
`policy` modules do not exist.

- [x] **Step 3: Implement minimal M2 runtime gates**

Implemented:

```text
TaskState
TaskStateValue
RuntimeOperationRequest
RuntimeDecision
PolicyEvaluationRequest
PolicyEvaluationResult
ResourceSet
PermissionLevel
RuntimeDecisionValue
RoutingMetadata
ArtifactPolicy
CommandInvocation
PolicyEngine::review_diff
```

The M2 implementation remains in memory only. SQLite persistence, audit rows,
RuntimeKernel composition, worker RPC, MCP adapter behavior, and Reasonix
invocation remain out of scope until later milestones.

- [x] **Step 4: Verify M2 tests pass**

Run:

```text
cargo test -p coasonix-runtime-core --tests -- --nocapture
```

Expected: all state, artifact, and policy tests pass.

- [x] **Step 5: Review M2 against blueprint**

Review checks:

```text
state machine blocks illegal and terminal transitions
required completion gaps block completion
reasonix call counter cannot be advanced by adapter-observed attempts
path policy rejects traversal, absolute outside paths, and symlink escapes
denylist is evaluated before allowlist
Windows case-folding bypass is covered
shell strings are rejected
argv[0], argv args, and extra argv bypasses are rejected structurally
network is denied by default
permission mismatch is denied
command hash is recorded for allowed argv
no M3+ SQLite/audit, worker, MCP, or Reasonix integration was added
```

- [x] **Step 6: Fix review findings**

Local review found and fixed:

```text
argv extra arguments were initially allowed after matching argv[0] and argv[1]
Windows case-folded absolute repo paths were authorized but returned an
un-normalized path, and case-sensitive relative extraction rejected them
```

An attempted code-review subagent run failed with an external `402 Payment
Required` provider error, so M2 review was completed locally against the
blueprint and tests above.

- [x] **Step 7: Run full verification and update implementation docs**

Fresh verification after review fixes:

```text
cargo test --workspace
  coasonix-runtime-core: 1 smoke, 7 artifact, 5 canonical, 8 policy,
  13 schema registry, and 4 state tests passed
  coasonix-runtime-worker: 0 tests, binary scaffold compiled

bun test
  packages/reasonix-expert-mcp/src/index.test.ts passed

python -m json.tool schemas/coasonix-v1.schema.json > $null
  exited 0

cargo fmt --all -- --check
  exited 0
```

## Task 6: SQLite Store and Audit

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `crates/coasonix-runtime-core/Cargo.toml`
- Modify: `crates/coasonix-runtime-core/src/lib.rs`
- Modify: `crates/coasonix-runtime-core/src/state/mod.rs`
- Create: `crates/coasonix-runtime-core/src/storage/mod.rs`
- Test: `crates/coasonix-runtime-core/tests/sqlite_store.rs`

- [x] **Step 1: Write failing M3 tests**

Test behaviors:

```text
database created under .agent/coasonix.sqlite
foreign keys enabled
journal_mode WAL, synchronous FULL, busy_timeout 5000
migrations run in required blueprint order
failed migration blocks store initialization and removes database file
audit update rejected
audit delete rejected
audit id globally monotonic
audit task_sequence monotonic per task
deny decision persisted through decision+audit transaction
runtime decision and audit commit atomically
failed audit insert rolls back runtime decision
state and audit commit atomically
rollback leaves no partial state transition
worker restart recovers task state
stale lock detected on startup
cache metadata can be recorded while cache reuse remains disabled
cache corruption denies reuse only
```

- [x] **Step 2: Verify tests fail**

Run:

```text
cargo test -p coasonix-runtime-core --test sqlite_store -- --nocapture
```

Expected before implementation: tests fail because `storage` module does not
exist.

- [x] **Step 3: Implement minimal SQLite store and audit writer**

Implemented:

```text
RuntimeStore::initialize
RuntimeStore::initialize_with_extra_migration
RuntimeStore::write_audit_event
RuntimeStore::commit_runtime_decision_with_audit
RuntimeStore::transition_state_with_audit
RuntimeStore::upsert_task_state
RuntimeStore::load_task_state
RuntimeStore::insert_lock
RuntimeStore::stale_locks
RuntimeStore::record_cache_metadata
RuntimeStore::cache_reuse_allowed
append-only audit update/delete triggers
required migration table order
SQLite PRAGMAs required by the blueprint
```

`rusqlite` is used with the bundled SQLite feature. Store transactions use
`TransactionBehavior::Immediate`, so `unchecked_transaction()` begins as
`BEGIN IMMEDIATE`.

- [x] **Step 4: Verify M3 tests pass**

Run:

```text
cargo test -p coasonix-runtime-core --test sqlite_store -- --nocapture
```

Expected: all SQLite store and audit tests pass.

- [x] **Step 5: Review M3 against blueprint**

Review checks:

```text
SQLite is under .agent/coasonix.sqlite
required PRAGMAs are applied per opened connection
all blueprint migration tables are created in order
audit_events are append-only by trigger
task_sequence is per-task monotonic and id is global monotonic
runtime decision and audit commit together
failed audit insert rolls back runtime decision
state and audit commit together
failed audit insert rolls back state update
deny decisions are persisted
task state survives store reopen
stale locks are detected
cache metadata is recorded but cache-hit reuse stays disabled
cache corruption denies reuse without breaking the store
no M4 RuntimeKernel, worker RPC, MCP adapter, or Reasonix integration was added
```

- [x] **Step 6: Fix review findings**

Local review found and fixed:

```text
RuntimeStore initially exposed insert_runtime_decision, which could persist a
decision without an audit event. The public API now requires
commit_runtime_decision_with_audit for decision persistence.

Transactions initially used rusqlite's default deferred behavior. Store opening
now sets TransactionBehavior::Immediate so runtime transactions begin as
BEGIN IMMEDIATE.
```

An attempted code-review subagent run failed with an external `402 Payment
Required` provider error, so M3 review was completed locally against the
blueprint and tests above.

- [x] **Step 7: Run full verification and update implementation docs**

Fresh verification after review fixes:

```text
cargo test --workspace
  coasonix-runtime-core: 1 smoke, 7 artifact, 5 canonical, 8 policy,
  13 schema registry, 12 sqlite store, and 4 state tests passed
  coasonix-runtime-worker: 0 tests, binary scaffold compiled

bun test
  packages/reasonix-expert-mcp/src/index.test.ts passed

python -m json.tool schemas/coasonix-v1.schema.json > $null
  exited 0

cargo fmt --all -- --check
  exited 0
```

## Task 7: RuntimeKernel Decision Merge

**Files:**
- Modify: `crates/coasonix-runtime-core/src/lib.rs`
- Modify: `crates/coasonix-runtime-core/src/storage/mod.rs`
- Create: `crates/coasonix-runtime-core/src/kernel/mod.rs`
- Test: `crates/coasonix-runtime-core/tests/runtime_kernel.rs`

- [x] **Step 1: Write failing M4 tests**

Test behaviors:

```text
allow decision contains schema/state/policy engine results
policy denial beats state allow and is persisted
state denial beats policy allow
unknown operation is denied by the schema gate
runtime_decision_v1 validates against schema registry
audit event id is attached to persisted runtime decision
write_audit is centralized through RuntimeKernel
decision merge precedence matches blueprint
validate_schema routes through kernel registry
```

- [x] **Step 2: Verify tests fail**

Run:

```text
cargo test -p coasonix-runtime-core --test runtime_kernel -- --nocapture
```

Expected before implementation: tests fail because `kernel` does not exist and
runtime decisions do not expose their persisted audit event id.

- [x] **Step 3: Implement minimal RuntimeKernel composition**

Implemented:

```text
RuntimeConfig
RuntimeKernel::initialize
RuntimeKernel::validate_schema
RuntimeKernel::evaluate_operation
RuntimeKernel::write_audit
RuntimeKernel::merge_decisions
RuntimeDecision::to_payload
EngineResults
AuditEvent
AuditWriteResult
RuntimeStore::runtime_decision_audit_event_id
runtime_decisions.audit_event_id
```

`evaluate_operation` now validates the runtime request shape, loads or creates
task state, evaluates local policy, merges schema/state/policy decisions,
persists the runtime decision and audit event in one store transaction, attaches
the audit event id to the returned decision, and advances newly created allowed
tasks to `running`.

- [x] **Step 4: Verify M4 tests pass**

Run:

```text
cargo test -p coasonix-runtime-core --test runtime_kernel -- --nocapture
```

Expected: all RuntimeKernel tests pass.

- [x] **Step 5: Review M4 against blueprint**

Review checks:

```text
RuntimeKernel owns schema, state, policy, audit, and artifact-gate composition
evaluate_operation validates runtime_operation_request_v1 before returning
schema/state/policy engine results are preserved in runtime_decision_v1
decision precedence matches the blueprint
policy deny beats fatal_error from other engines
deny decisions include reasons and are persisted
runtime decision and audit event are committed together
persisted runtime decisions reference their audit_event_id
manual audit writes route through RuntimeKernel
unknown Reasonix operations are not hidden by runtime-operation mapping
no worker JSON-RPC, MCP adapter, or real Reasonix invocation was added
```

- [x] **Step 6: Fix review findings**

Local review found and fixed:

```text
The initial schema request payload always used operation=call_reasonix_tool,
which hid unknown Reasonix operations from the schema gate. A regression test
now requires unknown operations to produce a schema denial; known
reasonix.review_diff requests are mapped to call_reasonix_tool only at the
runtime-operation boundary.
```

- [x] **Step 7: Run full verification and update implementation docs**

Fresh verification after review fixes:

```text
cargo test --workspace
  coasonix-runtime-core: 1 smoke, 7 artifact, 5 canonical, 8 policy,
  8 runtime kernel, 13 schema registry, 12 sqlite store, and 4 state tests
  passed
  coasonix-runtime-worker: 0 tests, binary scaffold compiled

bun test
  packages/reasonix-expert-mcp/src/index.test.ts passed

python -m json.tool schemas/coasonix-v1.schema.json > $null
  exited 0

cargo fmt --all -- --check
  exited 0
```

## Task 8: Rust JSON-RPC Worker

**Files:**
- Modify: `Cargo.lock`
- Modify: `crates/coasonix-runtime-worker/Cargo.toml`
- Modify: `crates/coasonix-runtime-worker/src/main.rs`
- Test: `crates/coasonix-runtime-worker/tests/json_rpc_worker.rs`

- [x] **Step 1: Write failing M5 tests**

Test behaviors:

```text
valid initialize succeeds after migrations
unknown method rejected
notification rejected
malformed JSON rejected
invalid params rejected
evaluate_operation returns runtime_decision_v1
validate_schema returns schema_validation_result_v1
worker stderr does not pollute stdout
stdout contains JSON-RPC frames only
worker shutdown is explicit
runtime.write_audit returns an audit record after initialize
policy denial still returns a runtime_decision_v1 result, not JSON-RPC success-as-authorization
```

- [x] **Step 2: Verify tests fail**

Run:

```text
cargo test -p coasonix-runtime-worker --test json_rpc_worker -- --nocapture
```

Expected before implementation: tests fail because the worker only prints a
scaffold line to stdout, which is not a JSON-RPC frame.

- [x] **Step 3: Implement minimal JSON-RPC worker**

Implemented:

```text
line-delimited JSON-RPC 2.0 stdin/stdout loop
runtime.initialize
runtime.validate_schema
runtime.evaluate_operation
runtime.write_audit
runtime.shutdown
Parse error, Invalid Request, Method not found, Invalid params, and runtime_unavailable mappings
JSON-RPC id to request_id mapping for REQ-* ids
RuntimeKernel-backed validate/evaluate/audit dispatch
explicit shutdown response and process exit
```

The worker does not expose post-v1 methods and does not implement MCP behavior.
It returns `runtime_decision_v1` as the result for `runtime.evaluate_operation`;
the future adapter must still require `result.decision == "allow"` before
treating any call as authorized.

- [x] **Step 4: Verify M5 tests pass**

Run:

```text
cargo test -p coasonix-runtime-worker --test json_rpc_worker -- --nocapture
```

Expected: all JSON-RPC worker tests pass.

- [x] **Step 5: Review M5 against blueprint**

Review checks:

```text
worker is a JSON-RPC 2.0 stdio process, not an MCP server
stdout contains only JSON-RPC responses
stderr is not used for ordinary responses
one input line maps to one complete JSON-RPC frame
notifications are rejected
unknown methods return Method not found
malformed frames return Parse error
invalid params return Invalid params
only runtime.initialize, runtime.validate_schema, runtime.evaluate_operation, runtime.write_audit, and runtime.shutdown are exposed
request id maps directly to request_id for REQ-* ids
initialize creates the SQLite store through RuntimeKernel migrations
validate_schema returns schema_validation_result_v1
evaluate_operation returns runtime_decision_v1
JSON-RPC success does not by itself authorize side effects
shutdown is explicit
no TypeScript worker client, MCP adapter, or real Reasonix invocation was added
```

- [x] **Step 6: Fix review findings**

Local review found and fixed:

```text
The first implementation carried an unused runtime_decision_error helper that
could imply denied runtime decisions should become JSON-RPC errors. It was
removed so evaluate_operation consistently returns runtime_decision_v1 results,
leaving authorization to the adapter's future result.decision == allow gate.

Review also added explicit coverage for runtime.write_audit and the acceptance
gate that policy denial remains a runtime_decision_v1 result rather than a
JSON-RPC error.
```

- [x] **Step 7: Run full verification and update implementation docs**

Fresh verification after review fixes:

```text
cargo test --workspace
  coasonix-runtime-core: 1 smoke, 7 artifact, 5 canonical, 8 policy,
  8 runtime kernel, 13 schema registry, 12 sqlite store, and 4 state tests
  passed
  coasonix-runtime-worker: 11 json_rpc_worker tests passed; binary compiled

bun test
  packages/reasonix-expert-mcp/src/index.test.ts passed

python -m json.tool schemas/coasonix-v1.schema.json > $null
  exited 0

cargo fmt --all -- --check
  exited 0
```

## Task 9: TypeScript Worker Client

**Files:**
- Create: `packages/reasonix-expert-mcp/src/worker/client.ts`
- Create: `packages/reasonix-expert-mcp/src/worker/protocol.ts`
- Create: `packages/reasonix-expert-mcp/src/worker/errors.ts`
- Test: `packages/reasonix-expert-mcp/src/worker/client.test.ts`

- [x] **Step 1: Write failing M6 tests**

Test behaviors:

```text
JSON-RPC request framing writes one complete request per line
non JSON-RPC response frames are rejected
client sends framed requests and receives JSON-RPC results
shutdown is explicit
timeout maps to runtime_unavailable and stops the worker
worker crash maps to runtime_unavailable
worker JSON-RPC -32008 maps to symbolic runtime_unavailable
missing worker executable maps to runtime_unavailable
restart replaces the worker process
```

- [x] **Step 2: Verify tests fail**

Run:

```text
bun test packages/reasonix-expert-mcp/src/worker/client.test.ts
```

Expected before implementation: tests fail because `src/worker/client.ts` and
`src/worker/protocol.ts` do not exist.

- [x] **Step 3: Implement minimal TypeScript worker client**

Implemented:

```text
encodeRequestFrame
parseResponseFrame
RuntimeWorkerError
RuntimeWorkerClient.call
RuntimeWorkerClient.shutdown
RuntimeWorkerClient.restart
RuntimeWorkerClient.isRunning
request timeout cleanup
worker crash and missing executable handling
stderr pipe draining so diagnostics cannot block the worker
JSON-RPC runtime error code mapping to symbolic runtime_* codes
```

The client owns TypeScript-side process supervision and JSON-RPC framing only.
It does not implement MCP tools/list, tools/call, Reasonix invocation, or any
security decision logic.

- [x] **Step 4: Verify M6 tests pass**

Run:

```text
bun test packages/reasonix-expert-mcp/src/worker/client.test.ts
```

Expected: all TypeScript worker client tests pass.

- [x] **Step 5: Review M6 against blueprint**

Review checks:

```text
JSON-RPC frames are one line per request
malformed/non-JSON-RPC stdout is rejected
timeout rejects the pending call and stops the worker
crash rejects pending calls as runtime_unavailable
missing executable maps to runtime_unavailable
runtime JSON-RPC errors map to symbolic runtime_* codes
restart replaces the worker process without resetting request id allocation
shutdown sends runtime.shutdown and leaves the client not running
stderr is drained as diagnostics and never treated as structured output
no MCP adapter, tools/list, tools/call, or Reasonix process invocation was added
```

- [x] **Step 6: Fix review findings**

Local review found and fixed:

```text
The first implementation mapped worker JSON-RPC errors to raw numeric strings.
Runtime error mappings now convert v1 worker codes such as -32008 to symbolic
runtime_unavailable.

RuntimeWorkerError was split into worker/errors.ts to avoid a protocol/client
import cycle. shutdown() now resets internal stopping state in a finally block
if the shutdown RPC fails or times out.
```

- [x] **Step 7: Run full verification and update implementation docs**

Fresh verification after review fixes:

```text
cargo test --workspace
  coasonix-runtime-core and coasonix-runtime-worker Rust tests passed, including
  11 json_rpc_worker tests

bun test
  packages/reasonix-expert-mcp/src/index.test.ts passed
  packages/reasonix-expert-mcp/src/worker/client.test.ts passed

python -m json.tool schemas/coasonix-v1.schema.json > $null
  exited 0

cargo fmt --all -- --check
  exited 0
```

## Full v1 Later Milestones

Future execution passes should continue with:

```text
M7: MCP adapter tools/list and tools/call
M8: mock Reasonix review_diff vertical slice
```

Each milestone requires failing tests first, passing tests after implementation,
review, fixes, and documentation updates before continuing.
