import { existsSync } from "node:fs";
import { resolve } from "node:path";

export type BackendProfile = "mock";

export interface CommandInvocation {
  command: string;
  args: string[];
}

export interface CommandResult {
  exitCode: number;
  stdout: string;
  stderr: string;
}

export interface SetupOptions {
  repoRoot: string;
  targetRepo: string;
  codexCommand?: string;
  bunCommand?: string;
  profile?: BackendProfile;
  buildRuntimeWorker?: boolean;
  verifyRegistration?: boolean;
  run?: (command: string, args: string[], options?: { cwd?: string }) => Promise<CommandResult>;
}

export interface SetupResult {
  command: CommandInvocation;
  stdout: string;
  stderr: string;
}

export function buildCodexMcpAddCommand(options: SetupOptions): CommandInvocation {
  const repoRoot = resolve(options.repoRoot);
  const packageRoot = resolve(repoRoot, "packages/reasonix-expert-mcp");
  const schemaPath = resolve(repoRoot, "schemas/coasonix-v1.schema.json");
  const runtimeWorker = resolve(
    repoRoot,
    process.platform === "win32"
      ? "target/debug/coasonix-runtime-worker.exe"
      : "target/debug/coasonix-runtime-worker",
  );
  const reasonixCommand = workerCommandForProfile(repoRoot, options.profile ?? "mock");

  return {
    command: options.codexCommand ?? "codex",
    args: [
      "mcp",
      "add",
      "coasonix",
      "--env",
      `COASONIX_REPO_ROOT=${resolve(options.targetRepo)}`,
      "--env",
      `COASONIX_SCHEMA_PATH=${schemaPath}`,
      "--env",
      `COASONIX_RUNTIME_WORKER=${runtimeWorker}`,
      "--env",
      `COASONIX_REASONIX_COMMAND_JSON=${JSON.stringify(reasonixCommand)}`,
      "--",
      options.bunCommand ?? process.execPath,
      "run",
      "--silent",
      `--cwd=${packageRoot}`,
      "start:mcp",
    ],
  };
}

export async function setupCodexMcp(options: SetupOptions): Promise<SetupResult> {
  const run = options.run ?? runCommand;
  if (options.buildRuntimeWorker ?? true) {
    await ensureRuntimeWorker(options.repoRoot, run);
  }
  const invocation = buildCodexMcpAddCommand(options);
  const result = await run(invocation.command, invocation.args);
  if (result.exitCode !== 0) {
    throw new Error(`codex mcp add failed: ${result.stderr || result.stdout}`);
  }
  if (options.verifyRegistration ?? true) {
    await verifyCodexRegistration(invocation.command, run);
  }
  return {
    command: invocation,
    stdout: result.stdout,
    stderr: result.stderr,
  };
}

function workerCommandForProfile(repoRoot: string, profile: BackendProfile): string[] {
  switch (profile) {
    case "mock":
      return [
        resolve(
          repoRoot,
          process.platform === "win32" ? "bin/coasonix-mock-worker.cmd" : "bin/coasonix-mock-worker",
        ),
        "review-diff",
      ];
  }
}

async function verifyCodexRegistration(
  codexCommand: string,
  run: (command: string, args: string[], options?: { cwd?: string }) => Promise<CommandResult>,
): Promise<void> {
  const getResult = await run(codexCommand, ["mcp", "get", "coasonix"]);
  if (getResult.exitCode !== 0) {
    throw new Error(`codex mcp get coasonix failed: ${getResult.stderr || getResult.stdout}`);
  }

  const listResult = await run(codexCommand, ["mcp", "list"]);
  if (listResult.exitCode !== 0) {
    throw new Error(`codex mcp list failed: ${listResult.stderr || listResult.stdout}`);
  }
  if (!listResult.stdout.includes("coasonix")) {
    throw new Error("codex mcp list did not include coasonix");
  }
}

async function ensureRuntimeWorker(
  repoRoot: string,
  run: (command: string, args: string[], options?: { cwd?: string }) => Promise<CommandResult>,
): Promise<void> {
  const workerPath = resolve(
    repoRoot,
    process.platform === "win32"
      ? "target/debug/coasonix-runtime-worker.exe"
      : "target/debug/coasonix-runtime-worker",
  );
  if (existsSync(workerPath)) {
    return;
  }
  const result = await run("cargo", ["build", "-p", "coasonix-runtime-worker"], { cwd: repoRoot });
  if (result.exitCode !== 0) {
    throw new Error(`failed to build runtime worker: ${result.stderr || result.stdout}`);
  }
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
  const args = parseArgs(process.argv.slice(2));
  const repoRoot = String(args.repoRoot ?? resolve(import.meta.dir, "../../../.."));
  const targetRepo = String(args.targetRepo ?? process.cwd());
  const profile = String(args.profile ?? "mock") as BackendProfile;
  const codexCommand = String(args.codexCommand ?? "codex");
  const bunCommand = String(args.bunCommand ?? process.execPath);

  setupCodexMcp({ repoRoot, targetRepo, profile, codexCommand, bunCommand })
    .then((result) => {
      process.stdout.write(result.stdout || "Registered Coasonix MCP server.\n");
    })
    .catch((error) => {
      const message = error instanceof Error ? error.message : String(error);
      process.stderr.write(`${message}\n`);
      process.exit(1);
    });
}
