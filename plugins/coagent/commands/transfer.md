# /coagent:transfer

Transfer the current session into a resumable Coagent task, or import an external agent session.

## Arguments

- `--source <path>`: path to an external agent session file (Claude JSONL format)
- `--export <task-id>`: export a Coagent task to JSON format

## Workflow

1. If `--export` is provided, call `coagent.export_session` MCP tool with the task ID.
2. If `--source` is provided, call `coagent.transfer_session` MCP tool with the source path.
3. If neither is provided, call `coagent.resume_task` for the most recent task.
4. Return the result verbatim, including any resume command or exported JSON.
