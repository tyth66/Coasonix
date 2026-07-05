# v1 MVP Implementation History

This file is a historical milestone reference. The active forward plan is:

```text
review-diff-agent-collaboration-plan.md
```

## Current Product Model

```text
Codex   = assigns the task and makes the final decision
Coasonix = safely gates and translates the call
Reasonix = performs the delegated expert review task
Codex   = evaluates the returned review
```

## Historical Milestones (Implemented)

```text
M12 setup:codex-mcp installer
M13 health:codex-mcp diagnostics
M14 conformance:agent-worker checks
M15 internal naming constants and reserved alias
M16 error taxonomy
M17 backend profiles
M18 tools/call error metadata
M19 CLI --help and expanded test coverage
```

These remain useful infrastructure. They no longer define what Reasonix review
result should contain.

## Verification Baseline

```powershell
cargo test --workspace
bun test
python -m json.tool schemas/coasonix-v1.schema.json > $null
cargo fmt --all -- --check
git diff --check
```
