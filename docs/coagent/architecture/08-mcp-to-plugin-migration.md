# Coagent: MCP Server to Codex Plugin Migration Plan

## Motivation

Coagent is currently a pure MCP server. It can only passively wait for Codex to call coagent.review_diff or coagent.runtime_status. It cannot actively intervene in Codex workflows. Compared to openai/codex-plugin-cc (which lets Claude Code invoke Codex), Coagent lacks five key capabilities:

1. **Cross-agent dispatch** -- route tasks to different backends based on type (Reasonix / Codex app-server / Mock)
2. **Slash command UX** -- natural interaction via /coagent:review, /coagent:status, etc.
3. **Background task management** -- queue, track, cancel, and retrieve results for tasks
4. **Stop hook review gate** -- auto-trigger review after model output, block on issues
5. **Session transfer** -- import external agent sessions into Codex, export Coagent sessions for recovery

These capabilities require the hook mechanism, command routing, sub-agents, and state persistence that a plugin framework provides. A pure MCP model cannot deliver them.


## References

- [Codex Plugin API & Console](https://codex-console.com/index) — Codex plugin development console and API reference
- [openai/codex-plugin-cc](https://github.com/openai/codex-plugin-cc) — Reference implementation: Claude Code plugin that delegates reviews and tasks to Codex

## Target Architecture

```
Codex Plugin: coagent/
+-- .codex-plugin/plugin.json      # plugin manifest (name, version, skills, MCP, commands, hooks)
+-- .mcp.json                       # retains Rust MCP server registration
+-- hooks.json                      # review gate, session lifecycle hooks
+-- commands/                       # slash command UX
|   +-- review.md
|   +-- adversarial-review.md
|   +-- rescue.md
|   +-- status.md
|   +-- cancel.md
|   +-- result.md
|   +-- transfer.md
+-- skills/                         # natural language -> command routing
|   +-- coagent/SKILL.md
+-- agents/                         # sub-agents (rescue mode)
|   +-- coagent-rescue.md
+-- scripts/                        # background task management
|   +-- coagent-companion.ps1
|   +-- lib/
|       +-- state.ps1               # JSON file state layer
|       +-- tracked-jobs.ps1        # task lifecycle
|       +-- job-control.ps1         # query/render
|       +-- render.ps1              # Markdown table rendering
+-- assets/                         # plugin icons
+-- ui/                             # (optional) embedded panel
```

The Rust MCP server is **retained**, registered via .mcp.json. The plugin layer (PowerShell scripts) is a thin orchestration layer: parse slash command arguments, manage foreground/background routing, track task state, render output. The actual gating, auditing, and backend invocation remain in the Rust layer.

### Layer Responsibilities

| Layer | Responsibility | Technology |
|---|---|---|
| Plugin UI | Slash command parsing, fg/bg routing, Markdown rendering | PowerShell + Markdown |
| Plugin State | Task tracking, progress updates, workspace config | JSON files (state.json + jobs/*.json) |
| Rust MCP | Gating, policy, audit, schema validation, backend calls | Rust (existing, +7 new tools) |
| Rust Kernel | 9-state FSM, PolicyEngine, SQLite audit, Replay | Rust (unchanged) |

Why keep the Rust MCP layer: Coagent core value (9-state FSM, policy engine, SQLite audit, schema validation, ACP session recovery) is already stable in Rust (176 tests passing). cc-plugin codex-companion.mjs (~900 lines of Node.js) is a thin client for the Codex app-server with no gating or auditing. Rewriting Coagent in Node.js would lose all these capabilities and introduce unnecessary complexity.

## Capability Migration Map

### 1. Cross-Agent Dispatch

**How cc-plugin does it**: Uses Claude Code Agent subagent_type mechanism to invoke a codex:codex-rescue sub-agent, which internally runs codex-companion.mjs task, driving Codex app-server JSON-RPC (thread/start -> turn/start -> notification stream capture).

**Coagent migration**:

The existing AgentBackend trait + BackendRegistry + BackendSelector already provide the infrastructure for cross-backend dispatch. Currently only AcpBackend (Reasonix via ACP) and MockBackend exist. A new backend routable to Codex app-server is needed.

New Rust MCP tool:

```
coagent.rescue
  Input: goal (string), model (optional), effort (optional), resume_thread_id (optional)
  Flow: evaluate_operation -> BackendSelector routes by capability tag ->
        CodexAppServerBackend invokes thread/start + turn/start + notification stream capture ->
        returns finalMessage + threadId
  Backend selection: code.implement.task tag -> CodexAppServerBackend,
                     code.review.diff tag -> AcpBackend (existing)
```

No new backend trait methods needed -- existing AgentBackend::invoke(BackendRequest) -> BackendResponse suffices. BackendRequest.context (currently a Value) carries all routing context.

**Rust changes**: New CodexAppServerBackend (~300 lines, implements AgentBackend trait, wraps Codex app-server JSON-RPC client), registered in BackendRegistry.

### 2. Slash Command UX

**How cc-plugin does it**: Each command is a Markdown file (commands/review.md etc.) with YAML frontmatter declaring argument hints, allowed tools, and whether model invocation is disabled. Claude Code plugin framework parses these as declarative routes.

**Coagent migration**:

Codex plugin framework supports commands/*.md. The Figma plugin uses commands/implement-from-figma.md as the route for /implement-from-figma. Format is plain Markdown body (no YAML frontmatter); Codex interprets it as model instructions.

Example commands/review.md:

```markdown
# /coagent:review

Run a Codex code review through the Coagent runtime gate.

## Arguments

- --base <ref>: branch to diff against (default: HEAD)
- --scope auto|working-tree|branch: review scope
- --wait|--background: execution mode

## Workflow

1. Generate diff with git diff <base>...HEAD > .agent/diffs/current.diff
2. Call coagent.review_diff MCP tool with the diff path
3. Return the review output verbatim
4. Do not fix issues or apply patches
```

Command-to-MCP mapping:

| Command | File | MCP Tool Called |
|---|---|---|
| /coagent:review | commands/review.md | coagent.review_diff |
| /coagent:adversarial-review | commands/adversarial-review.md | coagent.review_diff + focus/constraints |
| /coagent:rescue | commands/rescue.md | coagent.rescue (new) |
| /coagent:status | commands/status.md | coagent.list_jobs (new) |
| /coagent:cancel | commands/cancel.md | coagent.cancel_task (new) |
| /coagent:result | commands/result.md | coagent.task_result (new) |
| /coagent:transfer | commands/transfer.md | coagent.transfer_session (new) |

### 3. Background Task Management

**How cc-plugin does it**: JSON-file-based state layer (state.json + jobs/*.json + jobs/*.log) with max 50 jobs. runTrackedJob() manages state transitions (queued->running->completed|failed|cancelled). createJobProgressUpdater() updates phase/threadId/turnId during notification stream. Background spawn via spawn(..., detached: true, unref()).

**Coagent migration**:

**Dual-layer state strategy**:

1. {B}Rust SQLite{B}: Full audit trail (13 tables, append-only). Every evaluate_operation -> runtime_decisions, every complete_operation -> runtime_steps, every complete_task -> task_state. This is the {B}authoritative source{B} for post-hoc replay and compliance audit.

2. {B}Plugin JSON files{B}: Lightweight job summaries (state.json + jobs/*.json). Only stores active job id/status/phase/summary/logFile, not full audit data. This is the {B}low-latency query source{B} for instant /coagent:status rendering.

Why dual-layer: MCP tool calls have no persistent connection -- each Codex call to coagent.review_diff is an independent MCP request. When Codex calls coagent.list_jobs to query status, JOINing 13 SQLite tables every time has high latency and is unfriendly for concurrent calls. JSON cache provides O(1) active job queries.

State transition flow:

```
Plugin Layer                     Rust MCP Layer
+------------------+          +------------------+
| coagent-companion|          | coagent-mcp-     |
| .ps1             |          | server.exe       |
|                  |          |                  |
| 1. Create job    |          |                  |
|    record (queued)|          |                  |
| 2. Start-Job     |---MCP-->| 3. Receive req   |
|                  |          | 4. evaluate_op   |
|                  |          | 5. Invoke backend |
|                  |<--MCP---| 6. Return result  |
| 7. Write completed|         |                  |
|    Update job     |          |                  |
+------------------+          +------------------+
```

**New MCP tools**:

```
coagent.list_jobs
  Input: (none)
  Output: { running: [...], latestFinished: {...}, recent: [...] }
  Query: tasks + runtime_steps + task_state tables

coagent.cancel_task
  Input: task_id (string)
  Flow: evaluate_state -> transition_to(Cancelled) -> terminate subprocess -> audit event -> return
  Subprocess termination: read pid from operation_attempts table ->
    TerminateProcess (Windows) / SIGTERM (Unix)

coagent.task_result
  Input: task_id (string)
  Output: wrapper JSON (full review + metadata) or rescue final output
  Query: runtime_decisions + runtime_events + stored result payload
```

**Rust changes**: coagent.list_jobs (~80 lines), coagent.cancel_task (~60 lines + RuntimeKernel::cancel_task()), coagent.task_result (~40 lines). Plugin layer scripts/lib/state.ps1 (~100 lines: JSON read/write + job summary cache).

### 4. Stop Hook Review Gate

### 4. Stop Hook Review Gate

**How cc-plugin does it**: Registers a Stop hook in hooks/hooks.json. Claude Code triggers stop-review-gate-hook.mjs after model response and before stop. The script reads last_assistant_message, sends it to Codex via codex-companion.mjs task, parses ALLOW: or BLOCK: prefix in output, and emits a block decision to prevent stop if issues are found.

**Coagent migration**:

**Prerequisite**: Does the Codex plugin framework support Stop hooks? The Figma plugin only uses PostToolUse. cc-plugin SessionStart/SessionEnd/Stop are Claude Code plugin framework capabilities. If Codex supports Stop hooks, the implementation path is nearly identical to cc-plugin. If not, use an alternative approach.

**Plan A: Native Stop hook (if Codex supports it)**

hooks.json:

```json
{
  "hooks": {
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "pwsh -File ./scripts/stop-review-gate-hook.ps1",
            "timeout": 900
          }
        ]
      }
    ]
  }
}
```

scripts/stop-review-gate-hook.ps1:

```powershell
# 1. Read hook input (last_assistant_message, session_id, cwd)
# 2. Call coagent.review_diff MCP tool with last_assistant_message as context
# 3. Parse response: if verdict == needs_fix -> emit block decision with reason
#                    if verdict == pass -> do nothing (allow stop)
```

**Plan B: No Stop hook fallback**

1. Inject pre-stop check instruction in plugin skill: after significant code edits, guide user to run /coagent:review
2. Provide /coagent:review-gate enable/disable command to toggle the review gate
3. Review gate state stored in state.json config.stopReviewGate field
4. Coagent skill (injected into model context) reads this flag; when enabled, suggests review at end of each response

**Relationship with existing ApprovalPolicy**: Coagent already has ApprovalPolicy::Required but lacks a public approve/resume tool to unblock. Stop review gate and ApprovalPolicy are complementary:

- ApprovalPolicy: blocks **before** MCP tool invocation (gate before execution)
- Stop hook: blocks **after** model output (gate after effects)

Both can coexist: Stop hook reviews all model output, ApprovalPolicy controls specific tool execution.

**Rust changes**: coagent.approve_task (~40 lines + RuntimeKernel::approve_task()). Plugin layer scripts/stop-review-gate-hook.ps1 (~80 lines). No changes to existing PolicyEngine or ApprovalPolicy.

### 5. Session Transfer

**How cc-plugin does it**: /codex:transfer command calls codex-companion.mjs transfer, which sends a migration payload containing the Claude session path via Codex app-server externalAgentConfig/import JSON-RPC method. Codex app-server converts JSONL to a Codex thread. Returns codex resume <thread-id> command.

**Coagent migration**:

Coagent session transfer has three directions:

**Direction A: External agent -> Coagent** (similar to cc-plugin /codex:transfer)

```
coagent.transfer_session
  Input: source_path (string), source_type ("claude" | "codex" | "reasonix_acp")
  Flow:
    1. Parse external session file (JSONL / ACP log)
    2. Extract context: goal, diff_path, focus, constraints, base_branch
    3. Call Codex app-server externalAgentConfig/import (for Claude sessions)
       or create new Coagent task with injected context (for Reasonix ACP logs)
    4. Return thread_id and resume command
```

**Direction B: Coagent -> external agent** (Coagent-unique capability)

```
coagent.export_session
  Input: task_id (string), format ("jsonl" | "json")
  Output: full session serialized from SQLite tables, containing:
    - All audit_events (ordered by task_sequence)
    - All runtime_decisions
    - All runtime_events
    - All schema_validation_results
    - task_state transition history
  Use cases: audit compliance, cross-instance migration, downstream analysis
```

**Direction C: Internal Coagent recovery**

```
coagent.resume_task
  Input: task_id (string)
  Flow:
    1. Load TaskState from task_state table
    2. If PartiallyCompleted -> check subtask deps -> try advancing to Completed
    3. If Blocked -> check blocking reason -> try unblocking
    4. If WaitingApproval -> wait for coagent.approve_task
    5. Return current TaskState + actionable next step
```

**Rust changes**: coagent.transfer_session (~120 lines + session parsing logic), coagent.export_session (~80 lines, mostly SQL queries + JSON serialization), coagent.resume_task (~60 lines + RuntimeKernel::resume_task()).

## Implementation Roadmap

### Phase 1: Plugin Skeleton + Two New MCP Tools (est. 4h)

**Goal**: Codex can discover Coagent as a plugin, use /coagent:review and /coagent:status.

1. Create .codex-plugin/plugin.json plugin manifest
2. Create commands/review.md and commands/status.md
3. Create skills/coagent/SKILL.md (migrate from existing .codex/skills/coagent/)
4. Create .mcp.json (register coagent-mcp-server.exe)
5. New Rust MCP tool: coagent.list_jobs
6. New Rust MCP tool: coagent.cancel_task
7. Plugin layer: scripts/coagent-companion.ps1 + scripts/lib/state.ps1 (basic state layer)
8. E2E verify: /coagent:review -> diff generation -> coagent.review_diff -> result rendering
9. E2E verify: /coagent:status -> coagent.list_jobs -> Markdown table

### Phase 2: Background Tasks + Rescue (est. 5h)

**Goal**: Tasks can run in background; users can query progress, cancel, and retrieve results.

1. New Rust MCP tool: coagent.task_result
2. New Rust MCP tool: coagent.rescue
3. New CodexAppServerBackend (implements AgentBackend trait, wraps app-server JSON-RPC client)
4. Plugin layer: scripts/lib/tracked-jobs.ps1 (Start-Job + Receive-Job lifecycle)
5. Plugin layer: scripts/lib/job-control.ps1 (job query, fuzzy match, progress inference)
6. Plugin layer: scripts/lib/render.ps1 (Markdown table rendering)
7. Create commands/rescue.md, commands/cancel.md, commands/result.md
8. Create agents/coagent-rescue.md (sub-agent definition)
9. E2E verify: background review -> status polling -> cancel -> result retrieval

### Phase 3: Review Gate + Approval Flow (est. 3h)

**Goal**: Model output can be auto-reviewed; dangerous operations require approval.

1. Confirm Codex plugin framework Stop hook support
2. New Rust MCP tool: coagent.approve_task
3. If Stop hook available: hooks.json + scripts/stop-review-gate-hook.ps1
4. If Stop hook unavailable: plugin skill injects pre-stop check instruction
5. Create commands/adversarial-review.md and commands/review-gate.md
6. E2E verify: enable review gate -> model edits code -> auto-trigger review -> review blocks/allows

### Phase 4: Session Transfer + Recovery (est. 4h)

**Goal**: Sessions can be transferred between Codex instances; tasks can be recovered from audit logs.

1. New Rust MCP tool: coagent.transfer_session
2. New Rust MCP tool: coagent.export_session
3. New Rust MCP tool: coagent.resume_task
4. Create commands/transfer.md
5. E2E verify: Claude session import -> Codex thread recovery
6. E2E verify: Coagent task export -> new instance import -> resume execution

### Summary

| Phase | New MCP Tools | New Rust Code | New Plugin Code | Cumulative Hours |
|---|---|---|---|---|
| 1. Skeleton + basics | list_jobs, cancel_task | ~200 lines | ~300 lines PS + MD | 4h |
| 2. Background + rescue | task_result, rescue | ~400 lines (incl. new Backend) | ~400 lines PS | 9h |
| 3. Review gate + approval | approve_task | ~100 lines | ~150 lines PS | 12h |
| 4. Session transfer | transfer_session, export_session, resume_task | ~300 lines | ~100 lines PS | 16h |

**Total**: 7 new MCP tools, ~1000 lines Rust, ~950 lines plugin scripts. All changes are incremental -- no breaking changes to existing coagent.review_diff, coagent.runtime_status, 9-state FSM, policy engine, or audit layer.

## Relationship with Existing Architecture

### Unchanged

- coagent-runtime-core crate (lib.rs + state + kernel + policy + storage + schema + sandbox + replay + artifact): {B}zero changes{B}
- coagent-mcp-server pipeline/mod.rs, backends/, tools/review_diff.rs, tools/tool_spec.rs, config.rs: {B}zero or minimal changes{B}
- schemas/coagent-v1.schema.json: {B}zero changes{B}
- All existing tests (176 pass): {B}continue to pass{B}

### Most Changed

- coagent-mcp-server/src/main.rs: register new MCP tools (7 new tool handlers in #[tool_router]), register new Backend
- coagent-mcp-server/src/tools/: new files rescue.rs, list_jobs.rs, cancel_task.rs, task_result.rs, approve_task.rs, transfer_session.rs, export_session.rs, resume_task.rs
- coagent-runtime-core/src/kernel/mod.rs: new methods cancel_task(), approve_task(), resume_task()
- coagent-runtime-core/src/storage/mod.rs: new query methods (list_active_tasks(), load_task_full_history())
- New coagent-mcp-server/src/backends/codex_app_server.rs: CodexAppServerBackend

### v3.1 Roadmap Intersections

Current v3.1 roadmap (06-v3.1-roadmap.md) items relevant to this migration:

- #2 Per-operation backend selection: Partially complete. New CodexAppServerBackend further validates the capability-tag-based BackendSelector design.
- #3 Generic AcpBackend: Not required for this migration. CodexAppServerBackend is an independent Backend implementation; does not block future AcpBackend generalization.
- #7 AgentProfile + SessionPool: coagent.resume_task needs to restore TaskState from task_state table, sharing the underlying motivation with SessionPool key-based session management -- reuse backend sessions by task_id.

## Risks and Notes

1. {B}Codex plugin framework Stop hook{B}: If unavailable, Phase 3 review gate degrades to skill injection approach. This is not a blocker -- cc-plugin Stop hook is Claude Code-specific; Codex may implement equivalent functionality through different mechanisms.

2. {B}Rust MCP server startup overhead{B}: After plugin installation, each tool call launches a new MCP connection. coagent-mcp-server.exe (release ~5 MB) starts in <100ms, acceptable. Long-term consider making the background task manager a daemon process.

3. {B}JSON state file concurrency safety{B}: PowerShell Start-Job is process-level isolation, no concurrent writes to the same state.json. But file locking is needed to prevent races. cc-plugin has no locks -- it relies on Node.js event loop serialization. PowerShell side can use Mutex or simple retry-read-write pattern.

4. {B}Windows-specific background task spawning{B}: Start-Job is PowerShell-native (no detached: true POSIX concept). For cross-platform, pwsh 7.4+ Start-ThreadJob may be more suitable.

5. {B}cc-plugin broker pattern{B}: cc-plugin uses a persistent app-server-broker.mjs process to reuse Codex app-server connections. Coagent initial implementation can skip the broker -- each MCP call independently starts the Rust server. If startup overhead becomes a problem, a similar session pool can be introduced later.

## Comparison with cc-plugin

| Dimension | cc-plugin | Coagent (post-migration) |
|---|---|---|
| Runtime | Claude Code plugin | Codex plugin |
| Backend | Codex CLI (single) | Reasonix ACP / Codex app-server / Mock (multi) |
| Security model | No gating, no audit | Policy engine + 9-state FSM + SQLite audit |
| Review quality | Codex native /review | Backend-dependent: Reasonix (DeepSeek) / Codex / Mock |
| Session transfer | Claude -> Codex | Bidirectional (Claude/Codex/Reasonix <-> Coagent) |
| Implementation | Node.js (~5000 lines) | Rust (~12000 existing + ~1000 new) + PowerShell (~950 new) |
| State persistence | JSON files (50 job limit) | SQLite (full audit) + JSON (active job cache) |
