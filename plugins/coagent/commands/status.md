# /coagent:status

Show active and recent Coagent jobs for the current repository.

## Arguments

- `[job-id]`: show detailed status for a specific job
- `--all`: include all historical jobs, not just recent ones

## Workflow

1. If a job ID is provided, call `coagent.runtime_status` MCP tool and filter for that job's details.
2. If no job ID is provided, call `coagent.list_jobs` MCP tool.
3. Render the result as a Markdown table:
   - Active jobs: job ID, kind (review/rescue), status (queued/running), phase, elapsed time, summary
   - Latest finished job: job ID, kind, status (completed/failed/cancelled), duration, summary
4. Keep output compact. Do not include progress blocks or extra prose outside the table.
5. Preserve actionable follow-up commands from the output (e.g. `/coagent:cancel <job-id>`, `/coagent:result <job-id>`).
