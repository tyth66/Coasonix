import { describe, expect, test } from "bun:test";
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import { formatHealthReport, healthCodexMcp } from "./health";

const repoRoot = resolve(import.meta.dir, "../../../..");

describe("Codex MCP healthcheck", () => {
  test("root package exposes health:codex-mcp", () => {
    const packageJson = JSON.parse(readFileSync(resolve(repoRoot, "package.json"), "utf8"));

    expect(packageJson.scripts?.["health:codex-mcp"]).toBe(
      "bun packages/reasonix-expert-mcp/src/codex/health.ts",
    );
  });

  test("reports Codex registration failure without hiding the layer", async () => {
    const report = await healthCodexMcp({
      repoRoot,
      targetRepo: createFixtureRepo("codex-registration"),
      codexCommand: "codex",
      bunCommand: process.execPath,
      run: async (_command, args) => {
        if (args.join(" ") === "mcp get coasonix") {
          return { exitCode: 1, stdout: "", stderr: "No MCP server named coasonix" };
        }
        return { exitCode: 0, stdout: "coasonix enabled\n", stderr: "" };
      },
      skipGatewaySmoke: true,
    });

    expect(report.status).toBe("fail");
    expect(report.checks.find((check) => check.name === "codex_registration")).toMatchObject({
      status: "fail",
      code: "codex_mcp_not_registered",
    });
    expect(formatHealthReport(report)).toContain("[fail] codex_registration");
  });

  test("distinguishes server startup failure from Codex registration failure", async () => {
    const report = await healthCodexMcp({
      repoRoot,
      targetRepo: createFixtureRepo("server-startup"),
      codexCommand: "codex",
      bunCommand: resolve(repoRoot, "target/debug/missing-bun.exe"),
      run: codexRegistered,
    });

    expect(report.status).toBe("fail");
    expect(report.checks.find((check) => check.name === "codex_registration")).toMatchObject({
      status: "pass",
    });
    expect(report.checks.find((check) => check.name === "server_startup")).toMatchObject({
      status: "fail",
      code: "server_startup_failed",
    });
  });

  test("distinguishes runtime worker startup failure from backend worker failure", async () => {
    const report = await healthCodexMcp({
      repoRoot,
      targetRepo: createFixtureRepo("runtime-failure"),
      codexCommand: "codex",
      bunCommand: process.execPath,
      runtimeWorker: resolve(repoRoot, "target/debug/missing-coasonix-runtime-worker.exe"),
      run: codexRegistered,
    });

    expect(report.status).toBe("fail");
    expect(report.checks.find((check) => check.name === "runtime_initialize")).toMatchObject({
      status: "fail",
      code: "runtime_unavailable",
    });
    expect(report.checks.some((check) => check.code === "worker_nonzero_exit")).toBe(false);
  });

  test("distinguishes backend worker failure after server startup", async () => {
    const report = await healthCodexMcp({
      repoRoot,
      targetRepo: createFixtureRepo("backend-failure"),
      codexCommand: "codex",
      bunCommand: process.execPath,
      agentCommand: failingBackendCommand(),
      run: codexRegistered,
    });

    expect(report.status).toBe("fail");
    expect(report.checks.find((check) => check.name === "runtime_initialize")).toMatchObject({
      status: "pass",
    });
    expect(report.checks.find((check) => check.name === "mock_review_diff")).toMatchObject({
      status: "fail",
      code: "worker_nonzero_exit",
    });
  });

  test("passes against the mock profile through the real MCP server process", async () => {
    const report = await healthCodexMcp({
      repoRoot,
      targetRepo: createFixtureRepo("success"),
      codexCommand: "codex",
      bunCommand: process.execPath,
      run: codexRegistered,
    });

    expect(report.status).toBe("pass");
    expect(report.checks.map((check) => [check.name, check.status])).toEqual([
      ["codex_registration", "pass"],
      ["server_startup", "pass"],
      ["runtime_initialize", "pass"],
      ["tools_list", "pass"],
      ["stdout_protocol", "pass"],
      ["mock_review_diff", "pass"],
      ["runtime_shutdown", "pass"],
    ]);
    expect(formatHealthReport(report)).toContain("Coasonix Codex MCP health: pass");
  });

  test("CLI exits nonzero and writes a report when Codex config is missing", async () => {
    const fakeCodex = fakeCodexCommand({ registered: false });
    const child = Bun.spawn(
      [
        process.execPath,
        "packages/reasonix-expert-mcp/src/codex/health.ts",
        "--target-repo",
        createFixtureRepo("cli-missing-config"),
        "--codex-command",
        fakeCodex,
      ],
      {
        cwd: repoRoot,
        stdout: "pipe",
        stderr: "pipe",
      },
    );

    const [exitCode, stdout, stderr] = await Promise.all([
      child.exited,
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
    ]);

    expect(exitCode).toBe(1);
    expect(stderr).toBe("");
    expect(stdout).toContain("Coasonix Codex MCP health: fail");
    expect(stdout).toContain("codex_mcp_not_registered");
  });

  test("--help prints usage and exits zero", async () => {
    const child = Bun.spawn(
      [process.execPath, "packages/reasonix-expert-mcp/src/codex/health.ts", "--help"],
      { cwd: repoRoot, stdout: "pipe", stderr: "pipe" },
    );

    const [exitCode, stdout, stderr] = await Promise.all([
      child.exited,
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
    ]);

    expect(exitCode).toBe(0);
    expect(stderr).toBe("");
    expect(stdout).toContain("Usage:");
    expect(stdout).toContain("--target-repo");
  });
});

async function codexRegistered(_command: string, args: string[]) {
  if (args.join(" ") === "mcp get coasonix") {
    return { exitCode: 0, stdout: "coasonix\n", stderr: "" };
  }
  if (args.join(" ") === "mcp list") {
    return { exitCode: 0, stdout: "coasonix enabled\n", stderr: "" };
  }
  return { exitCode: 1, stdout: "", stderr: `unexpected command: ${args.join(" ")}` };
}

function createFixtureRepo(name: string): string {
  const repo = mkdtempSync(join(tmpdir(), `coasonix-health-${name}-`));
  mkdirSync(join(repo, ".agent", "diffs"), { recursive: true });
  mkdirSync(join(repo, ".agent", "results"), { recursive: true });
  writeFileSync(join(repo, ".agent", "diffs", "current.diff"), "diff --git a/a.txt b/a.txt\n");
  return repo;
}

function failingBackendCommand(): string[] {
  const dir = mkdtempSync(join(tmpdir(), "coasonix-health-backend-"));
  const script = join(dir, "backend-fails.mjs");
  writeFileSync(
    script,
    `
      import { stderr } from "node:process";
      stderr.write("backend failed intentionally");
      process.exit(7);
    `,
  );

  if (process.platform !== "win32") {
    return [process.execPath, script, "review-diff"];
  }

  const command = join(dir, "backend-fails.cmd");
  writeFileSync(command, `@echo off\r\n"${process.execPath}" "${script}" %*\r\n`);
  return [command, "review-diff"];
}

function fakeCodexCommand(options: { registered: boolean }): string {
  const dir = mkdtempSync(join(tmpdir(), "coasonix-health-fake-codex-"));

  if (process.platform !== "win32") {
    const command = join(dir, "codex");
    writeFileSync(
      command,
      `#!/usr/bin/env sh
if [ "$1 $2 $3" = "mcp get coasonix" ]; then
  ${options.registered ? 'printf "coasonix\\n"; exit 0' : 'printf "No MCP server named coasonix\\n" >&2; exit 1'}
fi
if [ "$1 $2" = "mcp list" ]; then
  printf "coasonix enabled\\n"; exit 0
fi
printf "unexpected command\\n" >&2
exit 2
`,
    );
    chmodSync(command, 0o755);
    return command;
  }

  const command = join(dir, "codex.cmd");
  writeFileSync(
    command,
    options.registered
      ? `@echo off\r\nif "%1 %2 %3"=="mcp get coasonix" echo coasonix& exit /b 0\r\nif "%1 %2"=="mcp list" echo coasonix enabled& exit /b 0\r\necho unexpected command 1>&2\r\nexit /b 2\r\n`
      : `@echo off\r\nif "%1 %2 %3"=="mcp get coasonix" echo No MCP server named coasonix 1>&2& exit /b 1\r\nif "%1 %2"=="mcp list" echo coasonix enabled& exit /b 0\r\necho unexpected command 1>&2\r\nexit /b 2\r\n`,
  );
  return command;
}


