# Executive Summary

Coasonix is a Codex-centered expert delegation runtime for calling Reasonix as a controlled expert system. The core model is:

```text
Codex = primary controller, orchestrator, executor, and final decision maker
Reasonix = DeepSeek cache-first expert multi-agent system
reasonix-expert Wrapper = MCP Gateway + Runtime Gate + Session Router
same project = shared Project Controller + isolated task namespaces + lane sessions
different project = isolated Project Controller and project-scoped state/cache/policy
```

## Current Status

```text
Deterministic Multi-Agent Runtime Spec: complete
Runtime Enforcement Layer design: complete
Global Runtime / Project Controller isolation / Session Pool / session lane mapping: complete
MVP engineering defaults: complete
Safe autonomous operation: blocked until runtime engines and conformance tests are implemented
```

## MVP Defaults

```text
1. Local single-machine STDIO transport.
2. Runtime Kernel embedded inside `reasonix-expert` Wrapper.
3. One Coasonix task should use one isolated git worktree by default.
4. Same-worktree write operations are serialized.
5. Reasonix session lanes are task-scoped: session_key includes task_id.
6. Reasonix project memory may generate hypotheses, not verification evidence.
7. `reasonix.propose_patch` returns patch_proposal_v1 only and never writes the Codex worktree.
8. Remote Reasonix worker / shared Gateway is deferred to a later deployment profile.
```

## Safety Boundary

Reasonix output is advisory. Every Reasonix result must be schema-valid before Codex can consider it. Patch proposals remain data until Codex decision, Patch Safety Checker, Patch Transaction, and Verification Gate allow progress.

Safe autonomous operation remains blocked until the Runtime Enforcement Layer, schema validator, state machine runner, policy engine, session router, patch checker, audit writer, and conformance tests exist.

## Implementation Entry Points

```text
Wrapper implementer:
  01-architecture/02-communication-and-mcp.md
  03-reasonix/01-tool-contracts-and-wrapper.md
  02-runtime/02-runtime-enforcement-layer.md

Runtime implementer:
  02-runtime/01-global-task-state-machine.md
  02-runtime/02-runtime-enforcement-layer.md
  02-runtime/03-policy-engine.md
  02-runtime/04-schema-enforcement.md

Reasonix integration implementer:
  01-architecture/04-project-session-tool-mapping.md
  03-reasonix/02-reasonix-concurrency-model.md
  03-reasonix/03-cache-engineering-model.md

Patch/verification implementer:
  04-patch-and-verification/*
```
