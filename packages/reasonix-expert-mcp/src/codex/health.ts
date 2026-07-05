import { existsSync } from "node:fs";
import { resolve } from "node:path";

import { buildCodexMcpAddCommand, type BackendProfile, type CommandResult } from "./setup";
import { ERROR_CODES, errorLayerForCode, type ErrorLayer } from "../agent/error-taxonomy";
import { EXTERNAL_REVIEW_DIFF_TOOL_NAME } from "../agent/naming";

type CheckStatus = "pass" | "fail";

export interface HealthCheck {
  name: string;
  status: CheckStatus;
  code?: string;
  layer?: ErrorLayer;
  message?: string;
  diagnostics?: Record<string, unknown>;
}

export interface HealthReport {
  schema_version: "codex_mcp_health_v1";
  status: CheckStatus;
  checks: HealthCheck[];
}

export interface HealthOptions {
  repoRoot: string;
  targetRepo: string;
  codexCommand?: string;
  bunCommand?: string;
  profile?: BackendProfile;
  runtimeWorker?: string;
  agentCommand?: string[];
  buildRuntimeWorker?: boolean;
  skipGatewaySmoke?: boolean;
  run?: (command: string, args: string[], options?: { cwd?: string }) => Promise<CommandResult>;
}

interface ServerLaunch {
  command: string;
  args: string[];
  env: Record<string, string>;
}

export async function healthCodexMcp(options: HealthOptions): Promise<HealthReport> {
  const checks: HealthCheck[] = [];
  const run = options.run ?? runCommand;

  checks.push(await checkCodexRegistration(options.codexCommand ?? "codex", run));

  if (!options.skipGatewaySmoke) {
    checks.push(...(await checkGatewaySmoke(options)));
  }

  return {
    schema_version: "codex_mcp_health_v1",
    status: checks.every((check) => check.status === "pass") ? "pass" : "fail",
    checks,
  };
}

export function formatHealthReport(report: HealthReport): string {
  const lines = [`Coasonix Codex MCP health: ${report.status}`];
  for (const check of report.checks) {
    const layer = check.layer ? `${check.layer}:` : "";
    const suffix = check.code ? ` (${layer}${check.code})` : "";
    const message = check.message ? ` - ${check.message}` : "";
    lines.push(`[${check.status}] ${check.name}${suffix}${message}`);
  }
  return `${lines.join("\n")}\n`;
}

async function checkCodexRegistration(
  codexCommand: string,
  run: (command: string, args: string[], options?: { cwd?: string }) => Promise<CommandResult>,
): Promise<HealthCheck> {
  let getResult: CommandResult;
  try {
    getResult = await run(codexCommand, ["mcp", "get", "coasonix"]);
  } catch (error) {
    return fail("codex_registration", ERROR_CODES.CODEX_MCP_NOT_REGISTERED, formatError(error));
  }
  if (getResult.exitCode !== 0) {
    return fail("codex_registration", ERROR_CODES.CODEX_MCP_NOT_REGISTERED, getResult.stderr || getResult.stdout);
  }

  let listResult: CommandResult;
  try {
    listResult = await run(codexCommand, ["mcp", "list"]);
  } catch (error) {
    return fail("codex_registration", ERROR_CODES.CODEX_MCP_NOT_REGISTERED, formatError(error));
  }
  if (listResult.exitCode !== 0) {
    return fail("codex_registration", ERROR_CODES.CODEX_MCP_NOT_REGISTERED, listResult.stderr || listResult.stdout);
  }
  if (!listResult.stdout.includes("coasonix")) {
    return fail("codex_registration", ERROR_CODES.CODEX_MCP_NOT_REGISTERED, "codex mcp list did not include coasonix");
  }

  return pass("codex_registration", "coasonix is registered and listed");
}

async function checkGatewaySmoke(options: HealthOptions): Promise<HealthCheck[]> {
  const checks: HealthCheck[] = [];
  const runtimeReady = await ensureRuntimeWorkerForHealth(options);
  if (!runtimeReady.ok) {
    checks.push(pass("server_startup", "server launch was not attempted"));
    checks.push(fail("runtime_initialize", ERROR_CODES.RUNTIME_UNAVAILABLE, runtimeReady.message));
    return checks;
  }

  let launch: ServerLaunch;
  try {
    launch = buildServerLaunch(options, runtimeReady.workerPath);
  } catch (error) {
    return [fail("server_startup", ERROR_CODES.SERVER_STARTUP_FAILED, formatError(error))];
  }

  let child: ReturnType<typeof Bun.spawn>;
  try {
    child = Bun.spawn([launch.command, ...launch.args], {
      cwd: resolve(options.repoRoot),
      env: { ...process.env, ...launch.env },
      stdin: "pipe",
      stdout: "pipe",
      stderr: "pipe",
    });
  } catch (error) {
    return [fail("server_startup", ERROR_CODES.SERVER_STARTUP_FAILED, formatError(error))];
  }
  checks.push(pass("server_startup", "server process launched"));

  const client = createJsonRpcClient(child);
  try {
    const initialize = await client.request("initialize", {
      protocolVersion: "2025-06-18",
      capabilities: {},
      clientInfo: { name: "coasonix-healthcheck", version: "0.0.0" },
    });
    if (initialize.result?.serverInfo?.name !== "reasonix-expert-mcp") {
      checks.push(fail("runtime_initialize", ERROR_CODES.SERVER_STARTUP_FAILED, "initialize returned unexpected serverInfo"));
      return await finishServer(child, checks);
    }
    checks.push(pass("runtime_initialize", "runtime.initialize completed before MCP initialize response"));

    const listed = await client.request("tools/list", {});
    const toolNames = listed.result?.tools?.map((tool: { name: string }) => tool.name) ?? [];
    if (JSON.stringify(toolNames) !== JSON.stringify([EXTERNAL_REVIEW_DIFF_TOOL_NAME])) {
      checks.push(fail("tools_list", ERROR_CODES.SERVER_STARTUP_FAILED, `unexpected tools/list result: ${toolNames.join(",")}`));
      return await finishServer(child, checks);
    }
    checks.push(pass("tools_list", "tools/list returned the expected v1 tool set"));
    checks.push(pass("stdout_protocol", "stdout contained parseable JSON-RPC response frames"));

    const result = await client.request("tools/call", {
      name: EXTERNAL_REVIEW_DIFF_TOOL_NAME,
      arguments: reviewDiffInput(options.targetRepo),
    });
    if (result.result?.isError) {
      checks.push(classifyBackendFailure(result.result));
      return await finishServer(child, checks);
    }
    if (result.result?.structuredContent?.schema_version !== "review_result_v1") {
      checks.push(fail("mock_review_diff", ERROR_CODES.WORKER_SCHEMA_INVALID, "tools/call did not return review_result_v1"));
      return await finishServer(child, checks);
    }
    checks.push(pass("mock_review_diff", "mock worker returned valid review_result_v1"));
    return await finishServer(child, checks);
  } catch (error) {
    const stderr = await stopServer(child);
    checks.push(classifyStartupFailure(formatError(error), stderr));
    return checks;
  }
}

function buildServerLaunch(options: HealthOptions, runtimeWorker: string): ServerLaunch {
  const invocation = buildCodexMcpAddCommand({
    repoRoot: options.repoRoot,
    targetRepo: options.targetRepo,
    codexCommand: options.codexCommand ?? "codex",
    bunCommand: options.bunCommand ?? process.execPath,
    profile: options.profile ?? "mock",
  });
  const separator = invocation.args.indexOf("--");
  if (separator < 0 || !invocation.args[separator + 1]) {
    throw new Error("setup command did not contain MCP server launch args");
  }

  const env = parseEnvArgs(invocation.args.slice(0, separator));
  env.COASONIX_RUNTIME_WORKER = runtimeWorker;
  if (options.agentCommand) {
    env.COASONIX_AGENT_COMMAND_JSON = JSON.stringify(options.agentCommand);
  }

  return {
    command: invocation.args[separator + 1],
    args: invocation.args.slice(separator + 2),
    env,
  };
}

function parseEnvArgs(args: string[]): Record<string, string> {
  const env: Record<string, string> = {};
  for (let index = 0; index < args.length; index += 1) {
    if (args[index] !== "--env") {
      continue;
    }
    const entry = args[index + 1] ?? "";
    const separator = entry.indexOf("=");
    if (separator > 0) {
      env[entry.slice(0, separator)] = entry.slice(separator + 1);
    }
    index += 1;
  }
  return env;
}

async function ensureRuntimeWorkerForHealth(
  options: HealthOptions,
): Promise<{ ok: true; workerPath: string } | { ok: false; workerPath: string; message: string }> {
  const workerPath =
    options.runtimeWorker ??
    resolve(
      options.repoRoot,
      process.platform === "win32"
        ? "target/debug/coasonix-runtime-worker.exe"
        : "target/debug/coasonix-runtime-worker",
    );
  if (options.runtimeWorker || existsSync(workerPath) || options.buildRuntimeWorker === false) {
    return existsSync(workerPath)
      ? { ok: true, workerPath }
      : { ok: false, workerPath, message: `runtime worker not found: ${workerPath}` };
  }

  let build: CommandResult;
  try {
    build = await runCommand("cargo", ["build", "-p", "coasonix-runtime-worker"], {
      cwd: resolve(options.repoRoot),
    });
  } catch (error) {
    return { ok: false, workerPath, message: formatError(error) };
  }
  if (build.exitCode !== 0) {
    return { ok: false, workerPath, message: build.stderr || build.stdout };
  }
  return { ok: true, workerPath };
}

function createJsonRpcClient(child: ReturnType<typeof Bun.spawn>) {
  const reader = child.stdout.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  let nextId = 1;

  async function readResponse(id: number): Promise<any> {
    const deadlineMs = 5_000;
    while (true) {
      const newlineIndex = buffer.indexOf("\n");
      if (newlineIndex >= 0) {
        const line = buffer.slice(0, newlineIndex);
        buffer = buffer.slice(newlineIndex + 1);
        if (!line.trim()) {
          continue;
        }
        const parsed = JSON.parse(line);
        if (parsed.id === id) {
          return parsed;
        }
      }

      const read = await withTimeout(reader.read(), deadlineMs, "server stdout timed out");
      if (read.done) {
        throw new Error("server stdout closed before response");
      }
      buffer += decoder.decode(read.value, { stream: true });
    }
  }

  return {
    async request(method: string, params: unknown) {
      const id = nextId++;
      child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
      child.stdin.flush();
      return readResponse(id);
    },
  };
}

async function finishServer(child: ReturnType<typeof Bun.spawn>, checks: HealthCheck[]): Promise<HealthCheck[]> {
  const stderr = await stopServer(child);
  const exitCode = await child.exited.catch(() => -1);
  if (exitCode === 0) {
    checks.push(pass("runtime_shutdown", stderr ? `shutdown succeeded with diagnostics: ${stderr}` : "shutdown succeeded"));
  } else {
    checks.push(fail("runtime_shutdown", ERROR_CODES.SERVER_STARTUP_FAILED, stderr || `server exited with ${exitCode}`));
  }
  return checks;
}

async function stopServer(child: ReturnType<typeof Bun.spawn>): Promise<string> {
  try {
    child.stdin.end();
  } catch {
    // Process may already have exited.
  }
  const stderr = new Response(child.stderr).text();
  const exit = withTimeout(child.exited, 3_000, "server shutdown timed out").catch(() => {
    try {
      child.kill();
    } catch {
      // Process may already have exited.
    }
    return -1;
  });
  await exit;
  return stderr;
}

function reviewDiffInput(repoRoot: string) {
  return {
    schema_version: "review_diff_input_v1",
    task_id: "TASK-health-review-diff",
    request_id: "REQ-health-review-diff",
    mode: "review_diff",
    goal: "Run Coasonix healthcheck review_diff.",
    repo: { root: repoRoot },
    artifacts: { diff_path: ".agent/diffs/current.diff" },
    permission_level: "L1_DIFF_REVIEW",
    output_schema: "review_result_v1",
  };
}

function classifyStartupFailure(message: string, stderr: string): HealthCheck {
  const combined = `${message}\n${stderr}`;
  if (/runtime|worker|coasonix-runtime-worker/i.test(combined)) {
    return fail("runtime_initialize", ERROR_CODES.RUNTIME_UNAVAILABLE, combined.trim());
  }
  return fail("server_startup", ERROR_CODES.SERVER_STARTUP_FAILED, combined.trim());
}

function classifyBackendFailure(result: {
  content?: Array<{ text?: string }>;
  _meta?: Record<string, unknown>;
}): HealthCheck {
  const metaCode = result._meta?.code;
  if (typeof metaCode === "string" && errorLayerForCode(metaCode) !== undefined) {
    const text = result.content?.map((item) => item.text ?? "").join("\n") ?? "";
    return fail("mock_review_diff", metaCode, text || `worker failed (${metaCode})`);
  }

  // Fallback: regex classification for backward compatibility with pre-taxonomy errors
  const text = result.content?.map((item) => item.text ?? "").join("\n") ?? "";
  if (/exited with/i.test(text)) {
    return fail("mock_review_diff", ERROR_CODES.WORKER_NONZERO_EXIT, text);
  }
  if (/timed out/i.test(text)) {
    return fail("mock_review_diff", ERROR_CODES.WORKER_TIMEOUT, text);
  }
  if (/schema/i.test(text)) {
    return fail("mock_review_diff", ERROR_CODES.WORKER_SCHEMA_INVALID, text);
  }
  return fail("mock_review_diff", ERROR_CODES.WORKER_UNAVAILABLE, text || "worker failed");
}

function pass(name: string, message?: string): HealthCheck {
  return { name, status: "pass", message };
}

function fail(name: string, code: string, message?: string): HealthCheck {
  return { name, status: "fail", code, layer: errorLayerForCode(code), message };
}

async function runCommand(
  command: string,
  args: string[],
  options: { cwd?: string } = {},
): Promise<CommandResult> {
  const child = Bun.spawn([command, ...args], {
    cwd: options.cwd,
    stdout: "pipe",
    stderr: "pipe",
  });
  const [stdout, stderr, exitCode] = await Promise.all([
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
    child.exited,
  ]);
  return { exitCode, stdout, stderr };
}

async function withTimeout<T>(promise: Promise<T>, timeoutMs: number, message: string): Promise<T> {
  let timeoutId: ReturnType<typeof setTimeout>;
  const timeout = new Promise<never>((_resolve, reject) => {
    timeoutId = setTimeout(() => reject(new Error(message)), timeoutMs);
  });
  try {
    return await Promise.race([promise, timeout]);
  } finally {
    clearTimeout(timeoutId!);
  }
}

function formatError(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
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
    process.stdout.write(`Usage: bun run health:codex-mcp [options]

Check Coasonix Codex MCP gateway health.

Options:
  --repo-root <path>     Coasonix repository root (default: auto-detect)
  --target-repo <path>   Target repository to healthcheck against
  --profile <name>       Backend profile: mock (default), conformance, reasonix-cli, mimocode-cli
  --codex-command <cmd>  Codex CLI command (default: codex)
  --bun-command <cmd>    Bun executable path (default: auto-detect)

Example:
  bun run health:codex-mcp --target-repo D:\\work\\my-project
`);
    process.exit(0);
  }

  const args = parseArgs(argv);
  const repoRoot = String(args.repoRoot ?? resolve(import.meta.dir, "../../../.."));
  const targetRepo = String(args.targetRepo ?? process.cwd());
  const profile = String(args.profile ?? "mock") as BackendProfile;
  const codexCommand = String(args.codexCommand ?? "codex");
  const bunCommand = String(args.bunCommand ?? process.execPath);

  healthCodexMcp({ repoRoot, targetRepo, profile, codexCommand, bunCommand })
    .then((report) => {
      process.stdout.write(formatHealthReport(report));
      process.exitCode = report.status === "pass" ? 0 : 1;
    })
    .catch((error) => {
      process.stderr.write(`${formatError(error)}\n`);
      process.exit(1);
    });
}

