# /coagent:adversarial-review

Run a steerable adversarial review that questions chosen implementation and design decisions. Unlike `/coagent:review`, this supports custom focus text to pressure-test specific risk areas.

## Arguments

- `--base <ref>`: branch or commit to diff against
- `--background`: run in background
- `[focus text]`: what to challenge (e.g. "look for race conditions", "question the caching design")

## Workflow

1. Determine the review target (same as `/coagent:review`).
2. Write the diff to `.agent/diffs/current.diff`.
3. Call `coagent.review_diff` MCP tool with the focus text passed as `focus` and `constraints` fields.
4. Return the review output verbatim. Do not fix issues.
