# Reasonix Project / Session / Tool Mapping

本文件定义多个 Codex 会话调用 `reasonix.*` MCP tools 时，Wrapper / Gateway 如何映射到 Reasonix 的 project controller、task namespace、session pool、session lane 和单次 tool call。

核心结论：

```text
Same repo/worktree/config/policy/runtime -> same Reasonix Project Controller
Same task -> isolated task namespace
Same role lane -> cache-stable Reasonix Session
Same tool call -> explicit snapshot input and schema-gated output
```

Coasonix 不应把每次 `reasonix.*` 调用都变成全新的 Reasonix 项目，也不应把所有专家任务塞进一个共享巨型会话。推荐模型是：同一个 Reasonix Project Controller 下挂多个 task namespace 和 cache-stable session lanes。

Final shape:

```text
Codex Session A
Codex Session B
Codex Session C
  -> reasonix-expert MCP Gateway
    -> Runtime Enforcement Layer
      -> Project Router
        -> Reasonix Project Controller A
        -> Reasonix Project Controller B
        -> Reasonix Project Controller C
```

Inside each project controller:

```text
Reasonix Project Controller
  -> Task Namespace
  -> Snapshot Registry
  -> Artifact Registry
  -> Policy Runtime
  -> Session Pool
    -> review lane
    -> security lane
    -> debug lane
    -> performance lane
    -> architecture lane
    -> patch lane
```

Global rule:

```text
Same project boundary:
  shared Reasonix Project Controller
  isolated task namespaces
  lane-based sessions

Different project boundary:
  isolated Reasonix Project Controller
  isolated session pool
  isolated task registry
  isolated artifact registry
  isolated policy/cache boundary
```

## 1. External Basis

Reasonix 的公开文档显示两个工程事实会影响 Coasonix 映射：

```text
1. Reasonix is optimized around DeepSeek prefix-cache and long append-only sessions.
2. Reasonix separates some collaboration roles into separate sessions to preserve cache-stable prefixes.
```

因此 Coasonix 的 Wrapper 必须同时满足：

```text
1. preserve project-level continuity
2. preserve lane-level cache stability
3. avoid hidden cross-tool memory
4. keep Codex as the authoritative state owner
```

References:

```text
https://reasonix.cn/
https://github.com/esengine/DeepSeek-Reasonix/blob/main-v2/docs/SPEC.md
https://github.com/esengine/DeepSeek-Reasonix/blob/main-v2/docs/GUIDE.md
```

## 2. Three-Layer Mapping

### 2.0 Global Runtime / Project Registry

`reasonix-expert` Gateway may be global. Reasonix Project Controller must be project-scoped.

Recommended layering:

```text
Global Reasonix Runtime
  +-- Global Static Prefix Cache
  +-- Model Provider Pool
  +-- Tool Schema Registry
  +-- Project Registry
        +-- Project Controller A
        |     +-- Project Static Prefix
        |     +-- Session Pool
        |     +-- Task Registry
        |     +-- Artifact Registry
        |     +-- Policy Runtime
        |
        +-- Project Controller B
              +-- Project Static Prefix
              +-- Session Pool
              +-- Task Registry
              +-- Artifact Registry
              +-- Policy Runtime
```

Allowed global sharing:

```text
Reasonix binary / runtime
model provider pool
tool schema registry
global static prefix for byte-identical generic rules
generic safety rules
```

Forbidden cross-project sharing:

```text
Reasonix session
task_state
artifact paths
result cache
patch proposals
context_projection
audit namespace
permission profile
project policy decisions
```

### 2.1 Reasonix Project Controller

Reasonix Project Controller is the repo/worktree/config/memory/plugin/sandbox/runtime boundary.

It includes:

```text
repo_root
worktree_id
reasonix.toml
AGENTS.md / REASONIX.md
project memory
MCP/plugin registry
sandbox policy
reasonix_runtime_version
session_pool
task_registry
snapshot_registry
artifact_registry
lock_manager
```

Rule:

```text
Same tenant/user boundary + repo_root_realpath + worktree_id + base_branch + reasonix_config_hash + coasonix_policy_hash + schema_family + reasonix_runtime_version MUST route to the same Reasonix Project Controller, even when calls originate from different Codex sessions.
```

Different repo_root, worktree_id, Reasonix config, schema family, runtime version, or policy boundary must route to a different Project Controller or fail closed during compatibility negotiation.

Rationale:

```text
Project-level reuse preserves repository structure understanding, project config, plugin registry, sandbox policy, runtime compatibility, and project-scoped memory.
```

Project reuse does not make Reasonix authoritative. Codex still owns global task state, final decisions, execution, verification, and delivery.

Controller shape:

```ts
class ReasonixProjectController {
  projectKey: string
  repoRoot: string
  worktreeId: string
  reasonixConfigHash: string
  coasonixPolicyHash: string
  reasonixRuntimeVersion: string

  sessions: SessionPool
  tasks: Map<TaskId, TaskRuntime>
  snapshots: SnapshotRegistry
  artifacts: ArtifactRegistry
  locks: LockManager
}
```

### 2.1.1 Task Namespace

Multiple Codex sessions may share one Reasonix Project Controller. They must not share task-local state.

```text
task_namespace =
  task_id
  + codex_session_id
  + branch_or_worktree_id
  + base_revision
```

Rules:

```text
1. Different Coasonix tasks under the same project MUST use separate task namespaces.
2. Every Reasonix output MUST be bound to task_id, request_id, snapshot_id, lane, schema_version, and base_revision.
3. No result from one task namespace may be consumed by another task unless Codex explicitly projects it as an artifact.
4. Result artifacts MUST be written under task_id/request_id scoped paths.
```

Recommended result paths:

```text
.agent/results/TASK-001/REQ-001.review_diff.json
.agent/results/TASK-001/REQ-002.security_audit.json
.agent/results/TASK-002/REQ-003.performance_review.json
```

Task runtime shape:

```ts
class TaskRuntime {
  taskId: string
  codexSessionId: string
  baseRevision: string
  state: TaskState
  explicitState: {
    findings: Finding[]
    hypothesisPool: Hypothesis[]
    riskRegister: Risk[]
    acceptedFindings: string[]
    rejectedFindings: string[]
  }
  snapshots: SnapshotId[]
}
```

### 2.2 Reasonix Session Lane

Reasonix Session is the cache-stable inference lane.

It is:

```text
cache boundary
append-only prefix boundary
role/model/permission boundary
```

It is not:

```text
authoritative memory
global task state
implicit cross-tool continuity
approval state
patch transaction state
```

Rule:

```text
Same lane SHOULD reuse a long-lived Reasonix Session when project_key, task_id, model, permission_level, static_prefix_hash, schema_family, policy_hash, and reasonix_runtime_version are compatible.
```

Different role, model, permission, policy, or mutation surface should use a different lane.

Session pool shape:

```ts
class SessionPool {
  getOrCreateLaneSession(routeKey: SessionRouteKey): ReasonixSession
  invalidateByPolicyHash(policyHash: string): void
  invalidateBySchemaFamily(schemaFamily: string): void
  invalidateByRuntimeVersion(reasonixRuntimeVersion: string): void
  closeIdleSessions(ttlMs: number): void
}
```

Session shape:

```ts
class ReasonixSession {
  lane: Lane
  staticPrefixHash: string
  permissionLevel: PermissionLevel
  appendOnlyHistory: Message[]
  reasonixRuntimeVersion: string
  lastUsedAt: Date
}
```

### 2.3 Reasonix Tool Call

Reasonix Tool Call is the Codex-visible MCP invocation exposed by `reasonix-expert`.

Examples:

```text
reasonix.review_diff
reasonix.security_audit
reasonix.debug_hypothesis
reasonix.performance_review
reasonix.test_plan
reasonix.propose_patch
```

These tools are Wrapper-owned contracts. They do not need to correspond one-to-one with Reasonix internal tools. The Wrapper may map one MCP tool call to one or more Reasonix turns inside the selected lane, but the result must still validate against the declared Coasonix schema.

## 3. Recommended Lane Layout

```text
Reasonix Project Controller
  |
  +-- SessionLane: review
  |     tools: reasonix.review_diff, reasonix.test_plan
  |
  +-- SessionLane: security
  |     tools: reasonix.security_audit
  |
  +-- SessionLane: debug
  |     tools: reasonix.debug_hypothesis
  |
  +-- SessionLane: performance
  |     tools: reasonix.performance_review
  |
  +-- SessionLane: architecture
  |     tools: reasonix.architecture_options
  |
  +-- SessionLane: patch
        tools: reasonix.propose_patch
```

Default routing:

| MCP tool | Default lane | Notes |
|---|---|---|
| `reasonix.review_diff` | `review` | Read-only diff review |
| `reasonix.test_plan` | `review` | May share review lane because it uses similar read-only diff/test context |
| `reasonix.security_audit` | `security` | Keep security reasoning isolated from non-security assumptions |
| `reasonix.debug_hypothesis` | `debug` | May depend on failure logs and prior attempts |
| `reasonix.performance_review` | `performance` | Keep benchmark/profiling assumptions isolated |
| `reasonix.architecture_options` | `architecture` | Usually single-lane due to direction-setting role |
| `reasonix.propose_patch` | `patch` | Isolated mutation-adjacent lane |

## 4. Routing Keys

```text
project_key =
  tenant_id_or_user_id
  repo_root
  + repo_root_realpath
  + worktree_id
  + base_branch
  + reasonix_config_hash
  + coasonix_policy_hash
  + schema_family
  + reasonix_runtime_version
```

```text
session_key =
  project_key
  + coasonix_task_id
  + lane
  + model
  + permission_level
  + static_prefix_hash
  + schema_family
  + coasonix_policy_hash
  + reasonix_runtime_version
```

```text
call_key =
  session_key
  + request_id
  + tool_name
  + output_schema
  + snapshot_id
  + base_revision
  + context_projection_hash
  + delta_context_hash
```

`project_key` controls Reasonix project reuse. `session_key` controls prefix-cache lane reuse. `call_key` controls request identity, result cache eligibility, and audit correlation.

Minimum project key components:

```text
repo_root_realpath
worktree_id
reasonix_config_hash
coasonix_policy_hash
schema_family
reasonix_runtime_version
```

Project boundary rules:

```text
different repo_root_realpath -> different Project Controller
different worktree_id -> different Project Runtime
different coasonix_policy_hash -> different Project Controller or hard invalidation
different reasonix_config_hash -> different Project Controller
different schema_family -> different Project Controller or explicit compatibility lane
different reasonix_runtime_version -> different Project Controller or hard invalidation
```

Route key:

```ts
type SessionRouteKey = {
  project_key: string
  task_id: string
  lane: "review" | "security" | "debug" | "performance" | "architecture" | "patch"
  permission_level: "L0_READONLY" | "L1_DIFF_REVIEW" | "L2_PATCH_ONLY" | "L3_ISOLATED_WORKTREE"
  schema_family: "coasonix.v1"
  static_prefix_hash: string
  policy_hash: string
  reasonix_runtime_version: string
}
```

Session hit condition:

```text
same project_key
same task_id
same lane
same permission_level
same schema_family
same static_prefix_hash
same policy_hash
same reasonix_runtime_version
```

Any change opens a new compatible lane session or invalidates the old one. Permission escalation, schema change, policy change, and Reasonix runtime version change must not reuse an existing session.

## 5. MVP Engineering Defaults

Coasonix MVP defaults are conservative. Safety and auditability take precedence over maximum cache reuse.

```text
1. Each Coasonix task SHOULD use an isolated git worktree by default.
2. Multiple Codex sessions MAY run read-only analysis against the same project/worktree.
3. Writes to the same worktree MUST be serialized by Runtime locks.
4. Session lanes are task-scoped by default: session_key includes task_id.
5. Reasonix project memory/history may inform hypotheses only; it is not verification evidence.
6. `reasonix.propose_patch` returns patch_proposal_v1 only and never writes the Codex worktree.
7. MVP Runtime Kernel is embedded inside `reasonix-expert` Wrapper.
8. MVP transport is local STDIO; remote Reasonix worker / multi-project Gateway is a later deployment profile.
```

### 5.1 Worktree Default

Default:

```text
one Coasonix task -> one isolated git worktree
```

If multiple tasks share one worktree, Runtime must treat writes as a serialized critical section and must rely on task namespace, immutable snapshots, and worktree write lock to prevent state mixing.

Recommended rule:

```text
read-only lanes may run concurrently
patch/edit/apply/verify operations are serial per worktree
```

### 5.2 Session Lane Scope

MVP uses task-scoped lane sessions:

```text
session_key =
  project_key
  + task_id
  + lane
  + permission_level
  + schema_family
  + static_prefix_hash
  + coasonix_policy_hash
  + reasonix_runtime_version
```

Project-level shared lane sessions are a future optimization. Enabling them requires additional conformance tests proving that hidden Reasonix session history cannot affect another task's result semantics.

### 5.3 Reasonix Memory Boundary

Reasonix project memory, history, or project knowledge may be used as:

```text
hypothesis source
triage hint
context recall candidate
```

It must not be used as:

```text
verification evidence
security proof
performance proof
completion evidence
patch safety evidence
```

If Reasonix memory affects an output, the output should label it as a memory-derived hypothesis and Codex must verify it through code evidence, tests, build/lint, benchmark/profiling, audit records, or human approval before treating it as fact.

### 5.4 Capability vs Internal Agent

Codex calls Reasonix capabilities, not Reasonix internal agents.

```text
Codex sees: reasonix.review_diff, reasonix.security_audit, reasonix.debug_hypothesis
Reasonix owns: planner/reviewer/security/memory/tool subagent orchestration
Wrapper validates: final schema result only
```

Codex must not select or control Reasonix internal subagents. This preserves the boundary between Coasonix tool contracts and Reasonix internal orchestration.

### 5.5 Patch Permission Boundary

`reasonix.propose_patch` is always advisory in MVP:

```text
Reasonix may output patch_proposal_v1
Reasonix must not write Codex worktree
Reasonix must not run git apply
Reasonix must not commit
Reasonix must not push
```

Future `L3_ISOLATED_WORKTREE` may allow Reasonix to experiment inside an isolated worktree, but final adoption still requires Codex decision, Runtime gates, Patch Safety Checker, Patch Transaction, and verification.

### 5.6 Deployment Default

MVP deployment:

```text
Codex -> reasonix-expert Wrapper(Runtime Kernel + Session Router) -> local Reasonix
transport: STDIO
```

Future deployment:

```text
Codex -> Gateway -> Runtime Service -> Reasonix Project Controller / worker pool
transport: Streamable HTTP
requires: auth, tenant isolation, rate limits, request queue, central audit store
```

## 6. Continuity Rules

Cross-tool continuity MUST use explicit Coasonix state and artifacts.

Allowed:

```text
debug_hypothesis output
-> .agent/results/TASK-001/REQ-003.debug_hypothesis.json
-> task_state.hypothesis_pool
-> explicit previous_findings in propose_patch input
```

Forbidden:

```text
Call debug_hypothesis
-> call propose_patch
-> assume Reasonix remembers hidden reasoning from the earlier session
```

Rules:

```text
1. Reasonix session memory is never an authoritative source of task state.
2. Every cross-tool dependency must appear in task_state or artifact paths.
3. Every result must include request_id and validate against output_schema.
4. Codex performs merge, conflict handling, and final decision.
5. Hidden session continuity may improve cache locality but must not change correctness semantics.
```

## 7. Patch Lane Rules

Patch-capable flows are mutation-adjacent and must be isolated.

```text
1. `reasonix.propose_patch` uses the `patch` lane by default.
2. Patch lane must not share a session with read-only lanes unless policy explicitly allows it.
3. Patch output remains data until Patch Safety Checker and Runtime Enforcement Layer allow application.
4. Patch lane cannot bypass schema validation, task state transitions, approval gates, or verification.
5. Patch lane cache reuse is disabled unless explicitly proven safe for the exact base_revision and policy_hash.
```

## 8. Parallelism Rules

Parallel read-only tool calls may execute as separate lanes under the same project.

```text
review lane
security lane
performance lane
```

Parallel lanes must read the same immutable task snapshot and write separate result artifacts. Codex performs the serial merge after all lanes settle or timeout.

Each Reasonix call must carry snapshot identity:

```json
{
  "snapshot_id": "SNAP-001",
  "task_id": "TASK-001",
  "base_revision": "abc123",
  "artifact_hashes": {
    "context": "sha256:...",
    "diff": "sha256:...",
    "test_log": "sha256:..."
  }
}
```

Reasonix execution must check:

```text
1. snapshot_id exists.
2. base_revision matches.
3. context/diff/log hashes match.
4. call policy allows reading the snapshot.
5. output binds the same snapshot_id and base_revision.
```

Snapshot mismatch invalidates the result. It must not be merged into task_state or accepted by Codex.

Mutation-adjacent operations remain serial:

```text
patch proposal acceptance
patch safety check
patch transaction apply
verification state transition
human approval transition
```

Locks:

```text
Project-level read lock: multiple read-only lanes may run concurrently.
Task-level mutation lock: patch lane and patch transaction are exclusive per task namespace.
Worktree write lock: any write or isolated-worktree promotion is exclusive per worktree.
```

## 9. Cache Rules

Session lane reuse optimizes DeepSeek prefix-cache behavior. It must not weaken correctness.

Cache is layered:

```text
global_static_prefix:
  Reasonix role rules
  generic safety rules
  generic tool definitions
  schema summary
  output format requirements
  forbidden behaviors

project_static_prefix:
  repo schema summary
  project constraints
  project policy summary
  project path rules
  project-specific AGENTS / REASONIX rules

context/result/patch caches:
  project-scoped by default
```

Cross-project cache rules:

```text
1. Global static prefix may be shared only when byte-identical and project-neutral.
2. Project static prefix must not be shared across Project Controllers.
3. Context projection cache is project-scoped and normally cannot be shared across projects.
4. Result cache is project-scoped by default and must not cross policy/schema/runtime/project boundaries.
5. Patch proposal cache must never cross base_revision or Project Controller boundaries.
```

```text
1. Static prefix must be byte-stable within a lane.
2. Lane-specific prompt, schema summary, permission rules, and tool contract are part of static_prefix_hash.
3. Dynamic task material belongs in context_projection_hash and delta_context_hash.
4. Project memory may inform project setup only if surfaced through explicit projection or documented Reasonix project behavior.
5. Result cache reuse still requires schema validation and audit logging.
```

## 10. Runtime Enforcement

Wrapper session routing is a runtime-controlled operation.

Project routing is also runtime-controlled. The Project Router must run before Session Router:

```text
tools/call
-> project_router.resolve(project_key)
-> runtime gates for project policy/state/schema/budget
-> session_router.resolve(session_key)
-> Reasonix lane execution
```

The Runtime Enforcement Layer must be able to audit:

```text
tenant_id_or_user_id
project_key
project_key_hash
session_key
task_namespace
codex_session_id
snapshot_id
base_revision
lane
tool_name
permission_level
coasonix_policy_hash
static_prefix_hash
schema_family
context_projection_hash
delta_context_hash
```

Routing failure is fail-closed:

```text
unknown project -> create or deny according to policy
unknown lane -> deny unless lane is configured
permission mismatch -> deny
policy hash mismatch -> open new compatible lane or deny
schema family mismatch -> open new compatible lane or deny
runtime version mismatch -> open new compatible lane or deny
snapshot mismatch -> deny
patch tool on read-only lane -> deny
project_key mismatch -> deny
cross-project session reuse attempt -> deny
cross-project result cache hit -> deny
```

## 11. Call Flows

### 11.1 Read-Only Tool Call

```text
1. Codex session calls a reasonix.* read-only tool.
2. Wrapper creates request_id.
3. Runtime Gate validates state, schema, policy, budget, and route.
4. Context Projector writes context_projection_v1.
5. SnapshotRegistry freezes snapshot_id and artifact hashes.
6. SessionRouter selects or creates the lane session.
7. Reasonix appends delta input inside that lane.
8. Output Normalizer extracts JSON.
9. Schema Validator validates the declared result schema.
10. Wrapper writes .agent/results/TASK/REQ.tool.json.
11. Wrapper returns MCP structuredContent.
12. Codex records accept / partial_accept / reject.
```

### 11.2 Patch Tool Call

```text
1. Codex decides a patch proposal is needed.
2. Runtime verifies no unresolved read-only fan-out blocks mutation-adjacent work.
3. SessionRouter selects the patch lane.
4. Reasonix returns patch_proposal_v1 only.
5. Reasonix does not write the Codex worktree.
6. Patch Safety Checker validates the diff.
7. Codex decides whether to apply it.
8. Patch Transaction applies, verifies, commits, or rolls back under Runtime control.
```

## 12. Final Rule

```text
Coasonix should call multiple Reasonix tools through one project-level persistent Reasonix Project Controller, multiple task namespaces, and multiple cache-stable session lanes.

Global Runtime owns shared binary/provider/tool-schema surfaces.
Project owns shared repo/config/plugin/sandbox context.
Task namespace owns task-local isolation.
Session lane owns cache-stable inference prefix.
Coasonix task_state and artifacts own cross-tool continuity.
Codex owns final decisions and execution.

Different projects never share Reasonix sessions, task state, artifacts, result cache, patch proposals, context projections, audit namespace, or permission profile.
```
