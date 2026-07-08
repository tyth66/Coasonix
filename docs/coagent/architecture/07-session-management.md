# Session Management

This document describes the intended session-management direction. It is not a
claim that multi-session execution is implemented today.

## Current Runtime

Current production runtime is a single lane:

```text
coagent.review_diff -> AcpBackend -> ReasonixRunner -> single persistent AcpSession
```

`ReasonixRunner` is Reasonix-specific. It drives `reasonix acp --model ...`
through one child process and one ACP session. Calls against the same runner are
serial because the session mutex is held across the full `send_prompt().await`.

Current recovery behavior:

```text
Ok                 -> keep session
Io | Protocol      -> drop session, reconnect once, retry same prompt
Timeout            -> drop session, return timeout without retry
Spawn              -> return immediately
tool_call after valid review JSON  -> return collected review immediately
`n≥5 consecutive denied tool_calls -> max tool calls protocol error, drop session without retry`ntool_call before valid review JSON -> deny (TOOL_UNSUPPORTED), increment counters, continue collecting
```

Current observability is exposed through `ReasonixRunnerStats` and
`coagent.runtime_status`. The status view is single-runner scoped and reports
whether a session exists, how many sessions were created, prompt attempts,
reconnects, timeout/protocol/I/O/spawn error counts, and the last error string.
Stats also include 	ool_call_count and denied_tool_call_count.

The current runner does not execute Reasonix-requested ACP tool calls. That
capability is intentionally left out of the single-lane baseline.

## Next Session Manager

The next multi-session step should wrap the existing runner rather than replace
it:

```text
ReasonixSessionManager<HashMap<SessionKey, ReasonixRunner>>
```

Proposed key:

```text
SessionKey = repo_root + task_id + lane + backend_id + model
```

Serial and parallel rules:

```text
same SessionKey      -> same ReasonixRunner -> serial prompts
different SessionKey -> different ReasonixRunner -> parallel lanes allowed
```

This keeps ACP frame ordering local to one session while allowing unrelated
lanes to progress independently.

## Future Runtime Status Shape

`coagent.runtime_status` is the stable debugging entry point. When session
management is introduced, extend the response rather than adding a separate
debug tool:

```json
{
  "backend": "reasonix",
  "repo_root": "D:/repo",
  "sessions": [
    {
      "session_key": "repo/task/lane/reasonix/model",
      "lane": "code_review",
      "task_id": "TASK-123",
      "has_session": true,
      "session_created_count": 1,
      "prompt_count": 3,
      "reconnect_count": 0,
      "timeout_count": 0,
      "last_used_at": "2026-07-09T00:00:00Z"
    }
  ]
}
```

Do not implement the session manager as one global session for all tools, and do
not create a fresh session per tool call. The intended model is stable keyed
reuse with explicit per-key observability.
