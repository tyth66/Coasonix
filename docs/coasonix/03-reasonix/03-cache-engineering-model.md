# Cache Engineering Model

> **设计规格（Design Specification）**：此文档描述的是 post-v1 KV cache 复用策略。
> 当前 v1 实现中 SQLite 有 `cache_entries` 表但 `reuse_enabled` 始终为 0，
> cache hit 功能被禁用（见 `02-runtime/06-executable-runtime-details.md` §6）。
> 代码中无 cache reuse 的实现。

Reasonix cache performance depends on stable prompt prefix, schema stability, and deterministic context projection. This file defines cache keys, invalidation, and reuse rules.

## 1. Cache Layers

```text
L0 no cache: unsafe / disabled path
L1 global static prefix cache: project-neutral system rules + generic tool definitions + schema summary
L2 project static prefix cache: repo schema summary + project constraints + project policy summary
L3 session lane cache: project_key + lane + static_prefix_hash
L4 projection cache: project_key + context_projection_v1 hash
L5 result cache: project_key + tool_name + schema_version + projection_hash + delta_hash
```

## 2. Cache Key

```text
cache_key =
  project_key
  + session_key
  hash(static_base_prefix)
  + tool_name
  + schema_version
  + coasonix_policy_hash
  + context_projection_hash
  + delta_context_hash
  + reasonix_runtime_version
```

Reasonix routing keys:

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

`project_key` preserves Reasonix project continuity and project isolation. `session_key` preserves cache-stable lane continuity. Neither key is an authoritative memory source.

## 3. Static Prefix Requirements

Static prefix must include:

```text
Reasonix role rules
security rules
tool definitions
schema summary
output format requirements
forbidden behaviors
lane identity
permission boundary
```

Global static prefix may include only project-neutral content:

```text
Reasonix role rules
generic safety rules
generic tool definitions
schema summary
output format requirements
forbidden behaviors
```

Project static prefix must include project-specific content:

```text
repo schema summary
project constraints
project policy summary
project path rules
project-specific AGENTS / REASONIX rules
```

Static prefix must not include:

```text
task_id
request_id
diff excerpt
log excerpt
test output
user secrets
cross-tool hidden memory
```

## 4. Invalidation Rules

Invalidate result cache when any of the following changes:

```text
schema_version
tool definition
coasonix_policy_hash
context_projection_hash
delta_context_hash
reasonix runtime version
Wrapper prompt template
redaction policy
permission level
lane
model
reasonix_config_hash
repo_root_realpath
worktree_id
```

Invalidate prefix cache when:

```text
security rules change
tool schema summary changes
static prompt text changes byte-for-byte
Reasonix role contract changes
lane role contract changes
```

## 5. Partial Reuse

Allowed:

```text
reuse static prefix cache across tasks
reuse global static prefix across projects only when byte-identical and project-neutral
reuse project static prefix only inside the same project_key
reuse context projection cache only for identical source artifact hashes
reuse result cache only for read-only tools
reuse session lane only when session_key components are compatible
```

Forbidden:

```text
reuse patch proposal across different base_revision
reuse result when coasonix_policy_hash changes
reuse result when schema_version differs
reuse result for L2_PATCH_ONLY or L3_ISOLATED_WORKTREE side-effect-bearing flows
reuse one session lane across different role/model/permission/policy boundaries
depend on hidden Reasonix session history for correctness
reuse project static prefix across different Project Controllers
reuse context projection cache across different Project Controllers
reuse result cache across different Project Controllers
reuse patch proposal cache across different Project Controllers
```

## 6. Cache Safety

```text
1. Cache hit must still emit audit event.
2. Cached Reasonix output must still pass output schema validation.
3. Cache hit must record cache_key and source request_id.
4. Cache must not store secrets after redaction.
5. Cache failure must degrade to live Reasonix call, not skip validation.
6. Session lane reuse must still emit routing and audit metadata.
7. Patch lane result reuse is disabled unless exact base_revision, coasonix_policy_hash, schema_family, and static_prefix_hash match.
8. Cross-project cache hits are denied except for byte-identical global static prefix.
```

## 7. Cache Observability

Metrics:

```text
cache_lookup_total
cache_hit_total
cache_miss_total
cache_invalidation_total
cache_prefix_reuse_total
cache_result_reuse_total
```

Trace fields:

```text
cache_layer
cache_key_hash
project_key_hash
session_key_hash
lane
hit
invalidation_reason
```
