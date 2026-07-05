# SQLite Persistence

> **实现状态**：此文档描述的 SQLite 持久化层已在 `crates/coasonix-runtime-core/src/storage/mod.rs`
> 中实现。10 张表、WAL 模式、append-only 触发器、FK 均已在代码中。
> 部分 post-v1 表（`policy_evaluation_results`、`artifacts`）已创建但写入路径
> 未完全使用。

Coasonix v1 uses a repo-local SQLite database for runtime state. SQLite is part
of the safety boundary because task state, audit ordering, runtime decisions,
locks, and cache metadata must survive Rust worker restarts.

Database location:

```text
.agent/coasonix.sqlite
```

Artifacts remain ordinary files under `.agent/`. SQLite stores their metadata,
hashes, policy decisions, and audit references.

## 1. Persistence Boundary

Stored in SQLite:

```text
task state snapshots
audit events
runtime decisions
policy decisions
schema validation results
lock records
cache metadata
artifact metadata
```

Post-v1 tables may also store:

```text
approval records
verification records
patch transaction metadata
```

Stored as files:

```text
.agent/context/**
.agent/diffs/**
.agent/results/**
.agent/logs/**
large command outputs
raw Reasonix outputs
patch text artifacts
```

SQLite is the source of truth for runtime state. Files are the source of truth
for artifact bytes.

## 2. Open Settings

The Rust Runtime Worker must open SQLite with deterministic settings:

```text
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;
PRAGMA synchronous = FULL;
PRAGMA busy_timeout = 5000;
```

Rules:

```text
1. The database lives under the repo-local .agent directory.
2. The worker must create parent directories before opening the database.
3. Startup must run schema migration before accepting runtime requests.
4. Failed migration blocks side effects.
5. Database corruption blocks side effects and returns runtime_storage_error.
```

## 3. Minimum Tables

Minimum v1-core tables:

```text
runtime_metadata
tasks
task_state
audit_events
runtime_decisions
schema_validation_results
policy_evaluation_results
locks
artifacts
cache_entries
```

Post-v1 tables:

```text
approvals
verification_results
patch_transactions
```

Each row that stores a schema-governed object should keep:

```text
schema_version
task_id
request_id where applicable
canonical_json
canonical_hash
created_at
```

v1 may create post-v1 tables only if they are inert and covered by migrations,
but v1 correctness must not depend on approval UI, verification runner, or patch
transaction tables.

Required v1 DDL shape, excluding `audit_events` which is defined in section 4:

```sql
create table runtime_metadata (
  key text primary key,
  value text not null,
  updated_at text not null
);

create table tasks (
  task_id text primary key,
  goal text,
  repo_root text not null,
  base_revision text,
  created_at text not null,
  updated_at text not null
);

create table task_state (
  task_id text primary key references tasks(task_id),
  state text not null,
  round integer not null default 0,
  reasonix_calls integer not null default 0,
  patch_attempts integer not null default 0,
  test_failure_rounds integer not null default 0,
  canonical_json text not null,
  canonical_hash text not null,
  updated_at text not null
);

create table runtime_decisions (
  id integer primary key autoincrement,
  task_id text not null references tasks(task_id),
  request_id text,
  operation text not null,
  decision text not null,
  canonical_json text not null,
  canonical_hash text not null unique,
  audit_event_id integer references audit_events(id),
  created_at text not null
);

create table schema_validation_results (
  id integer primary key autoincrement,
  task_id text not null references tasks(task_id),
  request_id text,
  expected_schema text not null,
  valid integer not null,
  canonical_json text not null,
  canonical_hash text not null unique,
  audit_event_id integer references audit_events(id),
  created_at text not null
);

create table policy_evaluation_results (
  id integer primary key autoincrement,
  task_id text not null references tasks(task_id),
  request_id text,
  operation text not null,
  decision text not null,
  canonical_json text not null,
  canonical_hash text not null unique,
  audit_event_id integer references audit_events(id),
  created_at text not null
);

create table locks (
  lock_key text primary key,
  lock_kind text not null,
  task_id text references tasks(task_id),
  request_id text,
  owner_worker_id text not null,
  status text not null,
  acquired_at text not null,
  expires_at text,
  released_at text
);

create table artifacts (
  artifact_path text primary key,
  artifact_kind text not null,
  task_id text not null references tasks(task_id),
  request_id text,
  content_hash text not null,
  size_bytes integer not null,
  policy_decision_id integer references policy_evaluation_results(id),
  created_at text not null
);

create table cache_entries (
  cache_key_hash text primary key,
  cache_family text not null,
  schema_family text not null,
  policy_hash text not null,
  snapshot_id text,
  base_revision text,
  artifact_hashes_json text not null,
  payload_artifact_path text references artifacts(artifact_path),
  created_at text not null,
  expires_at text
);
```

Architecture impact:

```text
No architecture change. This makes the selected SQLite boundary executable by
giving the Rust migration layer concrete v1 tables, keys, and audit references.
```

Migration order:

```text
1. runtime_metadata
2. tasks
3. audit_events and append-only triggers
4. task_state
5. runtime_decisions
6. schema_validation_results
7. policy_evaluation_results
8. locks
9. artifacts
10. cache_entries
```

## 4. Audit Append-Only Rule

Audit is append-only inside SQLite. Coasonix uses a hybrid ordering model:

```text
id            = global database order
task_sequence = per-task audit chain order
```

This gives restart/replay code a global fact order while keeping each task's
timeline directly readable.

`audit_events` must include:

```text
id integer primary key autoincrement
task_id text not null
request_id text
task_sequence integer not null
event text not null
actor text not null
status text not null
canonical_json text not null
canonical_hash text not null
created_at text not null
```

Constraints:

```text
unique(task_id, task_sequence)
unique(canonical_hash)
```

Required DDL shape:

```sql
create table audit_events (
  id integer primary key autoincrement,
  task_id text not null,
  request_id text,
  task_sequence integer not null,
  event text not null,
  actor text not null,
  status text not null,
  canonical_json text not null,
  canonical_hash text not null,
  created_at text not null,
  unique(task_id, task_sequence),
  unique(canonical_hash)
);

create trigger audit_events_no_update
before update on audit_events
begin
  select raise(abort, 'audit_events is append-only');
end;

create trigger audit_events_no_delete
before delete on audit_events
begin
  select raise(abort, 'audit_events is append-only');
end;
```

The runtime migration must install triggers that reject update and delete on
`audit_events`.

Required behavior:

```text
1. id is globally monotonic in commit order.
2. task_sequence is monotonic per task.
3. Audit rows are never updated or deleted by runtime code.
4. A failed audit insert makes the enclosing runtime operation fail.
5. Runtime decisions are not complete until their audit event is committed.
6. Audit export to JSONL is allowed for debugging, but JSONL is not the v1 source
   of truth.
```

## 5. Transaction Rules

Runtime operations that change state must use SQLite transactions.

Required transaction shape:

```text
BEGIN IMMEDIATE;
validate current state
evaluate policy
insert runtime_decision
insert audit_event with next task_sequence
update task_state when applicable
COMMIT;
```

Rules:

```text
1. No side effect may run before the allow decision transaction commits.
2. Deny decisions are also recorded and committed.
3. State and audit for one runtime decision are committed atomically.
4. Cache metadata updates never replace required audit writes.
5. Rollback leaves no partial state transition.
6. The adapter may perform a side effect only after the allow transaction commits.
```

## 6. Lock Records

The v1 worker is one process per repo root, but locks are still persisted so
restart recovery can detect interrupted operations.

Lock table minimum fields:

```text
lock_key
lock_kind
task_id
request_id
owner_worker_id
status
acquired_at
expires_at
released_at
```

Rules:

```text
1. Worktree write locks are exclusive.
2. Task state locks are exclusive per task.
3. Read-only operations do not take worktree write locks.
4. Startup must detect stale held locks and mark the related task recoverable or
   failed according to policy.
```

## 7. Artifact Metadata

SQLite stores artifact metadata, not large artifact bytes.

Artifact metadata:

```text
artifact_path
artifact_kind
task_id
request_id
content_hash
size_bytes
created_at
policy_decision_id
```

Rules:

```text
1. Artifact paths are normalized before insert.
2. Artifact hashes are computed from bytes after write.
3. Missing artifact files invalidate dependent cached results.
4. Artifact metadata without a matching file is a recoverable integrity error
   unless policy marks the artifact required.
```

## 8. Cache Metadata

Cache entries are disposable. State and audit are not.

Cache entries store:

```text
cache_family
cache_key_hash
schema_family
policy_hash
snapshot_id
base_revision
artifact_hashes
payload_artifact_path
created_at
expires_at
```

Rules:

```text
1. Cache metadata may be deleted to recover.
2. v1 may record cache metadata but should not reuse cached review_diff results
   until cache-hit conformance tests exist.
3. Cache hits still require schema validation.
4. Cache hits still emit audit events.
5. Cache corruption denies reuse, not the whole task.
```

## 9. Migration Rules

SQLite schema migrations are runtime compatibility gates.

Rules:

```text
1. Each migration has a monotonic integer version.
2. v1 migrations are Rust-owned SQL applied by RuntimeDatabase at startup.
3. Migrations run at worker startup before runtime.initialize returns success.
4. Failed migration blocks side effects.
5. Destructive migrations are forbidden in v1.
6. Unknown future database version fails closed.
```

## 10. Testing Requirements

v1 conformance must test:

```text
database created under .agent/coasonix.sqlite
foreign keys enabled
audit update rejected
audit delete rejected
audit id globally monotonic
audit task_sequence monotonic per task
task state and audit commit atomically
deny decision persisted
worker restart recovers task state
stale lock detected on startup
cache metadata can be recorded without enabling v1 cache-hit reuse
cache corruption denies reuse only
```
