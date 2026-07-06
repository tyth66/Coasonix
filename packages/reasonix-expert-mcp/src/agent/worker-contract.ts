import { resolve } from "node:path";

import { ERROR_CODES, errorLayerForCode, type ErrorLayer } from "./error-taxonomy";

export interface ReviewDiffContractInput {
  schema_version: "review_diff_input_v1";
  task_id: string;
  request_id: string;
  mode: "review_diff";
  goal: string;
  repo: { root: string };
  artifacts: { diff_path: string };
  permission_level: "L1_DIFF_REVIEW";
  output_schema: "review_result_v1";
}

export interface AgentWorkerRunResult {
  stdout: string;
  stderr: string;
  exitCode: number;
  timedOut?: boolean;
}

export type AgentWorkerValidationResult =
  | {
      ok: true;
      value: Record<string, unknown>;
    }
  | {
      ok: false;
      code: string;
      layer: ErrorLayer;
      message: string;
    };

export interface AgentWorkerConformanceCheck {
  name: string;
  status: "pass" | "fail";
  code?: string;
  layer?: ErrorLayer;
  message: string;
}

export interface AgentWorkerConformanceReport {
  schema_version: "agent_worker_conformance_v1";
  status: "pass" | "fail";
  checks: AgentWorkerConformanceCheck[];
}

export interface AgentWorkerConformanceOptions {
  command: string[];
  timeoutMs?: number;
}

export async function runAgentWorkerConformance(
  options: AgentWorkerConformanceOptions,
): Promise<AgentWorkerConformanceReport> {
  const input = contractInput();
  let run: AgentWorkerRunResult;
  try {
    run = await runAgentWorkerCommand(options.command, input, options.timeoutMs ?? 1_000);
  } catch (error) {
    const check = {
      name: "success",
      status: "fail" as const,
      code: ERROR_CODES.WORKER_UNAVAILABLE,
      layer: errorLayerForCode(ERROR_CODES.WORKER_UNAVAILABLE),
      message: formatError(error),
    };
    return {
      schema_version: "agent_worker_conformance_v1",
      status: "fail",
      checks: [check],
    };
  }
  const validation = validateAgentWorkerReviewResult(input, run);
  const check: AgentWorkerConformanceCheck = validation.ok
    ? {
        name: "success",
        status: "pass",
        message: "worker emitted one valid review_result_v1 JSON object",
      }
    : {
        name: "success",
        status: "fail",
        code: validation.code,
        layer: validation.layer,
        message: validation.message,
      };

  return {
    schema_version: "agent_worker_conformance_v1",
    status: check.status,
    checks: [check],
  };
}

export function validateAgentWorkerReviewResult(
  input: ReviewDiffContractInput,
  run: AgentWorkerRunResult,
): AgentWorkerValidationResult {
  if (run.timedOut) {
    return invalid(ERROR_CODES.WORKER_TIMEOUT, "worker did not exit before timeout");
  }
  if (run.exitCode !== 0) {
    return invalid(ERROR_CODES.WORKER_NONZERO_EXIT, `worker exited with ${run.exitCode}`);
  }

  const trimmed = run.stdout.trim();
  if (!trimmed) {
    return invalid(ERROR_CODES.WORKER_EMPTY_STDOUT, "worker stdout was empty");
  }

  let parsed: unknown;
  try {
    parsed = JSON.parse(trimmed);
  } catch {
    return invalid(ERROR_CODES.WORKER_MALFORMED_JSON, "worker stdout must be exactly one JSON object");
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    return invalid(ERROR_CODES.WORKER_MALFORMED_JSON, "worker stdout JSON must be an object");
  }

  const output = parsed as Record<string, unknown>;
  if (output.task_id !== input.task_id || output.request_id !== input.request_id) {
    return invalid(ERROR_CODES.WORKER_IDENTITY_MISMATCH, "worker output task_id/request_id must match input");
  }

  const schemaError = reviewResultSchemaError(output);
  if (schemaError) {
    return invalid(ERROR_CODES.WORKER_SCHEMA_INVALID, schemaError);
  }

  return { ok: true, value: output };
}

export function formatAgentWorkerConformanceReport(report: AgentWorkerConformanceReport): string {
  const lines = [`Agent Worker Contract conformance: ${report.status}`];
  for (const check of report.checks) {
    const layer = check.layer ? `${check.layer}:` : "";
    const suffix = check.code ? ` (${layer}${check.code})` : "";
    lines.push(`[${check.status}] ${check.name}${suffix} - ${check.message}`);
  }
  return `${lines.join("\n")}\n`;
}

async function runAgentWorkerCommand(
  command: string[],
  input: ReviewDiffContractInput,
  timeoutMs: number,
): Promise<AgentWorkerRunResult> {
  const child = Bun.spawn(command, {
    stdin: "pipe",
    stdout: "pipe",
    stderr: "pipe",
  });
  const stdout = new Response(child.stdout).text();
  const stderr = new Response(child.stderr).text();

  child.stdin.write(JSON.stringify(input));
  child.stdin.end();

  let timedOut = false;
  let timeoutId: ReturnType<typeof setTimeout>;
  const timeout = new Promise<null>((resolve) => {
    timeoutId = setTimeout(() => {
      timedOut = true;
      try {
        child.kill();
      } catch {
        // Process may already have exited.
      }
      resolve(null);
    }, timeoutMs);
  });

  const exitCode = await Promise.race([child.exited, timeout]);
  clearTimeout(timeoutId!);
  const [stdoutText, stderrText] = await Promise.all([stdout, stderr]);
  return {
    stdout: stdoutText,
    stderr: stderrText,
    exitCode: exitCode ?? -1,
    timedOut,
  };
}

function reviewResultSchemaError(output: Record<string, unknown>): string | null {
  if (output.schema_version !== "review_result_v1") {
    return "schema_version must be review_result_v1";
  }
  if (
    typeof output.status !== "string" ||
    ![
      "ok",
      "partial",
      "error",
      "timeout",
      "invalid_input",
      "permission_denied",
      "schema_validation_failed",
      "reasonix_failed",
      "artifact_not_found",
    ].includes(output.status)
  ) {
    return "status must be a valid result status";
  }
  if (
    typeof output.verdict !== "string" ||
    !["pass", "needs_fix", "risky", "unknown", "not_applicable"].includes(output.verdict)
  ) {
    return "verdict must be a valid result verdict";
  }
  if (typeof output.summary !== "string" || !output.summary) {
    return "summary must be a non-empty string";
  }
  if (typeof output.confidence !== "number" || output.confidence < 0 || output.confidence > 1) {
    return "confidence must be a number between 0 and 1";
  }
  return null;
}

function contractInput(): ReviewDiffContractInput {
  return {
    schema_version: "review_diff_input_v1",
    task_id: "TASK-agent-contract",
    request_id: "REQ-agent-contract",
    mode: "review_diff",
    goal: "Validate the Agent Worker Contract.",
    repo: { root: process.cwd() },
    artifacts: { diff_path: ".agent/diffs/current.diff" },
    permission_level: "L1_DIFF_REVIEW",
    output_schema: "review_result_v1",
  };
}

function invalid(code: string, message: string): AgentWorkerValidationResult {
  return { ok: false, code, layer: errorLayerForCode(code), message };
}

function formatError(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function defaultMockWorkerCommand(repoRoot: string): string[] {
  return [
    resolve(
      repoRoot,
      process.platform === "win32" ? "bin/coasonix-mock-worker.cmd" : "bin/coasonix-mock-worker",
    ),
    "review-diff",
  ];
}

function parseArgs(argv: string[]) {
  const parsed: Record<string, string | boolean> = {};
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (!arg.startsWith("--")) {
      continue;
    }
    const raw = arg.slice(2);
    const separator = raw.indexOf("=");
    const rawKey = separator >= 0 ? raw.slice(0, separator) : raw;
    const inlineValue = separator >= 0 ? raw.slice(separator + 1) : undefined;
    const key = rawKey.replace(/-([a-z])/g, (_, letter: string) => letter.toUpperCase());
    if (inlineValue !== undefined) {
      parsed[key] = inlineValue;
    } else if (argv[index + 1] && !argv[index + 1].startsWith("--")) {
      parsed[key] = argv[index + 1];
      index += 1;
    } else {
      parsed[key] = true;
    }
  }
  return parsed;
}

if (import.meta.main) {
  const argv = process.argv.slice(2);
  if (argv.includes("--help") || argv.includes("-h")) {
    process.stdout.write(`Usage: bun run conformance:agent-worker [options]

Validate a backend agent worker against the review-diff stdio contract.

Options:
  --command-json <json>  Worker command as JSON string array (default: repo-local mock worker)
  --timeout-ms <ms>      Worker timeout in milliseconds (default: 1000)

Examples:
  bun run conformance:agent-worker
  bun run conformance:agent-worker --command-json '["reasonix-cli","review-diff"]'
  bun run conformance:agent-worker --command-json '["mimocode-cli","review-diff"]' --timeout-ms 5000
`);
    process.exit(0);
  }

  const args = parseArgs(argv);
  const repoRoot = String(args.repoRoot ?? resolve(import.meta.dir, "../../../.."));
  const command = args.commandJson
    ? (JSON.parse(String(args.commandJson)) as string[])
    : defaultMockWorkerCommand(repoRoot);
  const timeoutMs = args.timeoutMs ? Number(args.timeoutMs) : 1_000;

  runAgentWorkerConformance({ command, timeoutMs })
    .then((report) => {
      process.stdout.write(formatAgentWorkerConformanceReport(report));
      process.exitCode = report.status === "pass" ? 0 : 1;
    })
    .catch((error) => {
      const message = error instanceof Error ? error.message : String(error);
      process.stderr.write(`${message}\n`);
      process.exit(1);
    });
}

