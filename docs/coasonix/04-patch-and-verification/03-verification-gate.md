# Verification Gate

Verification Gate upgrades a claim from advisory or inferred status to verified status using tests, lint, build, benchmark, profiling, static analysis, or approved human evidence.

## 1. Verification Types

```text
unit_test
integration_test
lint
typecheck
build
security_scan
benchmark
profiling
manual_approval
static_review
```

## 2. Claim Mapping

| Claim Type | Minimum Evidence |
|---|---|
| bug fixed | relevant failing test now passes or repro no longer fails |
| security risk addressed | targeted security test or human approval |
| performance improved | benchmark/profiling before-after evidence |
| patch safe to apply | patch_safety_report_v1 pass |
| architecture acceptable | Codex decision record plus explicit constraints |
| test plan adequate | tests mapped to risk areas |

## 3. Verification Gap Semantics

```text
allowed_terminal_with_required_verification_gap: false
allowed_terminal_with_optional_verification_gap: true
```

Rules:

```text
1. complete is forbidden if any required verification gap remains open.
2. optional verification gaps may exist in complete only if explicitly recorded as non-blocking.
3. performance claims always require benchmark/profiling evidence or remain unverified.
4. security-sensitive claims require targeted evidence or human approval.
5. Reasonix memory/history cannot satisfy a required verification gap.
```

## 4. Hard Requirements

```text
1. Verification must reference command, artifact, timestamp, and result.
2. Passing generic tests cannot verify a specific performance claim.
3. If verification cannot run, Codex must record a verification gap.
4. Final completion requires no unresolved required verification gaps.
5. Reasonix pass verdict is not sufficient completion evidence.
```

## 5. Output Contract

Verification artifacts validate against `verification_result_v1` in `../schemas/coasonix-v1.schema.json`.
