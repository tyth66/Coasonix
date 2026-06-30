# Repository Guidelines

## Project Structure & Module Organization

This repository is currently a documentation-first specification for Coasonix, a Codex-Orchestrated Reasonix Runtime. The root `README.md` gives the high-level entry point. The canonical documentation lives under `docs/coasonix/`:

- `00-executive-summary.md` summarizes current conclusions and implementation status.
- `01-architecture/` defines roles, context ownership, MCP communication, and project/session routing.
- `02-runtime/` describes task state, runtime enforcement, policy, schema checks, and observability.
- `03-reasonix/` covers Reasonix tool contracts, concurrency, cache behavior, and context projection risks.
- `04-patch-and-verification/` documents patch transactions, safety checks, verification, and approval gates.
- `05-versioning/` defines schema and compatibility policy.
- `06-roadmap/` records reassessment, defaults, and implementation planning.
- `schemas/coasonix-v1.schema.json` is the canonical v1 schema registry.

## Build, Test, and Development Commands

There is no application build system yet. Use lightweight checks:

- `git status --short` checks pending edits before and after changes.
- `python -m json.tool docs/coasonix/schemas/coasonix-v1.schema.json > $null` verifies the schema file is valid JSON.
- `git diff -- docs/coasonix` reviews documentation-only changes before commit.

When runtime code is added, document its build and test commands here before relying on them in PRs.

## Coding Style & Naming Conventions

Markdown files should use ATX headings (`#`, `##`), concise paragraphs, and fenced code blocks for command or contract examples. Preserve the numbered directory prefixes because they encode the intended reading order. New documentation files should use lowercase kebab-case names, for example `07-conformance-tests/01-test-runner.md`.

JSON schema edits should keep two-space indentation, stable key ordering where practical, and explicit `additionalProperties` decisions for object contracts.

## Testing Guidelines

For documentation changes, verify links and cross-references manually against `docs/coasonix/README.md`. For schema changes, run the JSON validation command above and inspect affected `$defs` references. When implementation code appears, add focused tests beside the new module or in a clearly named `tests/` tree, and update this guide with the exact command.

## Commit & Pull Request Guidelines

Current history uses concise, imperative summary commits such as `Establish Coasonix documentation baseline`. Keep commit subjects short and outcome-focused.

Pull requests should include a summary, changed documentation areas, schema impact if any, verification performed, and open risks or follow-up work. Link related issues when available. Include screenshots only if generated diagrams or rendered documentation are part of the change.

## Agent-Specific Instructions

Treat `docs/coasonix/README.md` and `schemas/coasonix-v1.schema.json` as source-of-truth entry points. Keep documentation, schema names, and roadmap status aligned in the same change.
