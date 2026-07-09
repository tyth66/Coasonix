# /coagent:review

Run a gated code review on local git changes through the Coagent runtime. The review is read-only -- Coagent enforces policy, records an audit trail, and returns findings with severity levels.

## Arguments

- `--base <ref>`: branch or commit to diff against (default: `HEAD` for uncommitted changes, or use `main...HEAD` for branch review)
- `--background`: run review in background; check progress with `/coagent:status`
- `--scope`: `working-tree` (default) or `branch`

## Workflow

1. Determine the review target:
   - Working-tree (default): `git diff HEAD` captures staged + unstaged changes. Also check `git status --short --untracked-files=all` for untracked files.
   - Branch (`--base <ref>`): `git diff <ref>...HEAD`
2. Verify there is content to review. If both `git diff --shortstat` and untracked files are empty, tell the user there is nothing to review.
3. Write the diff to `.agent/diffs/current.diff` (create `.agent/diffs/` if needed).
4. Call the `coagent.review_diff` MCP tool with:
   - `schema_version`: `"review_diff_input_v1"`
   - `goal`: a short description of what changed (inferred from the diff and branch context)
   - `repo.root`: absolute path to the repo root
   - `artifacts.diff_path`: `".agent/diffs/current.diff"`
   - `permission_level`: `"L1_DIFF_REVIEW"`
   - `output_schema`: `"review_result_v1"`
5. Return the review output verbatim. Do not paraphrase, summarize, or add commentary.
6. Do not fix any issues mentioned in the review output. This tool is read-only.
