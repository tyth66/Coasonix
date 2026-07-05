import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

import { buildCodexMcpAddCommand, setupCodexMcp } from "./setup";

const repoRoot = resolve(import.meta.dir, "../../../..");
const runtimeWorkerName =
  process.platform === "win32" ? "coasonix-runtime-worker.exe" : "coasonix-runtime-worker";
const mockWorkerName =
  process.platform === "win32" ? "coasonix-mock-worker.cmd" : "coasonix-mock-worker";

describe("Codex MCP setup", () => {
  test("builds a protocol-clean codex mcp add command with stable paths", () => {
    const command = buildCodexMcpAddCommand({
      repoRoot,
      targetRepo: "D:\\work\\target-repo",
      codexCommand: "codex",
      bunCommand: "bun",
      profile: "mock",
    });

    expect(command.command).toBe("codex");
    expect(command.args).toContain("mcp");
    expect(command.args).toContain("add");
    expect(command.args).toContain("coasonix");
    expect(command.args).toContain("--");
    expect(command.args).toContain("run");
    expect(command.args).toContain("--silent");
    expect(command.args).toContain("start:mcp");
    expect(command.args).toContain(`--cwd=${resolve(repoRoot, "packages/reasonix-expert-mcp")}`);
    expect(command.args).toContain("COASONIX_REPO_ROOT=D:\\work\\target-repo");
    expect(command.args.join("\n")).not.toContain("COASONIX_SCHEMA_PATH");
    expect(command.args).toContain(
      `COASONIX_RUNTIME_WORKER=${resolve(repoRoot, "target/debug", runtimeWorkerName)}`,
    );
    const workerEnv = command.args.find((arg) => arg.startsWith("COASONIX_REASONIX_COMMAND_JSON="));
    expect(workerEnv).toContain(mockWorkerName);
    const workerArgv = JSON.parse(workerEnv!.replace("COASONIX_REASONIX_COMMAND_JSON=", ""));
    expect(workerArgv[1]).toBe("review-diff");
    expect(existsSync(workerArgv[0])).toBe(true);
    expect(command.args.join(" ")).not.toContain("Temp");
  });

  test("conformance profile uses the same repo-local contract worker shape", () => {
    const command = buildCodexMcpAddCommand({
      repoRoot,
      targetRepo: "D:\\work\\target-repo",
      codexCommand: "codex",
      bunCommand: "bun",
      profile: "conformance",
    });

    const workerEnv = command.args.find((arg) => arg.startsWith("COASONIX_REASONIX_COMMAND_JSON="));
    const workerArgv = JSON.parse(workerEnv!.replace("COASONIX_REASONIX_COMMAND_JSON=", ""));
    expect(workerArgv[0]).toContain(mockWorkerName);
    expect(workerArgv[1]).toBe("review-diff");
    expect(command.args).toContain("COASONIX_REASONIX_TIMEOUT_MS=10000");
  });

  test("reasonix-cli profile requires explicit backend command JSON", () => {
    expect(() =>
      buildCodexMcpAddCommand({
        repoRoot,
        targetRepo: "D:\\work\\target-repo",
        codexCommand: "codex",
        bunCommand: "bun",
        profile: "reasonix-cli",
        env: {},
      }),
    ).toThrow("backend_not_configured");
  });

  test("reasonix-cli profile uses explicit backend command JSON without changing runtime startup", () => {
    const command = buildCodexMcpAddCommand({
      repoRoot,
      targetRepo: "D:\\work\\target-repo",
      codexCommand: "codex",
      bunCommand: "bun",
      profile: "reasonix-cli",
      env: { COASONIX_REASONIX_CLI_COMMAND_JSON: '["reasonix-cli","review-diff"]' },
    });

    const workerEnv = command.args.find((arg) => arg.startsWith("COASONIX_REASONIX_COMMAND_JSON="));
    expect(JSON.parse(workerEnv!.replace("COASONIX_REASONIX_COMMAND_JSON=", ""))).toEqual([
      "reasonix-cli",
      "review-diff",
    ]);
    expect(command.args).toContain("COASONIX_REASONIX_TIMEOUT_MS=10000");
    expect(command.args).toContain("start:mcp");
    expect(command.args).toContain("--silent");
  });

  test("runs codex mcp add through the injected runner", async () => {
    const calls: Array<{ command: string; args: string[] }> = [];

    await setupCodexMcp({
      repoRoot,
      targetRepo: "D:\\work\\target-repo",
      codexCommand: "codex",
      bunCommand: "bun",
      profile: "mock",
      buildRuntimeWorker: false,
      verifyRegistration: false,
      run: async (command, args) => {
        calls.push({ command, args });
        return { exitCode: 0, stdout: "Added global MCP server 'coasonix'.", stderr: "" };
      },
    });

    expect(calls).toHaveLength(1);
    expect(calls[0].command).toBe("codex");
    expect(calls[0].args.slice(0, 4)).toEqual(["mcp", "add", "coasonix", "--env"]);
  });

  test("verifies the registered Codex MCP server after add", async () => {
    const calls: Array<{ command: string; args: string[] }> = [];

    await setupCodexMcp({
      repoRoot,
      targetRepo: "D:\\work\\target-repo",
      codexCommand: "codex",
      bunCommand: "bun",
      profile: "mock",
      buildRuntimeWorker: false,
      run: async (command, args) => {
        calls.push({ command, args });
        if (args[0] === "mcp" && args[1] === "add" && args[2] === "coasonix") {
          return { exitCode: 0, stdout: "Added global MCP server 'coasonix'.", stderr: "" };
        }
        if (args.join(" ") === "mcp get coasonix") {
          return { exitCode: 0, stdout: "coasonix\n", stderr: "" };
        }
        if (args.join(" ") === "mcp list") {
          return { exitCode: 0, stdout: "coasonix enabled\n", stderr: "" };
        }
        return { exitCode: 1, stdout: "", stderr: `unexpected command: ${args.join(" ")}` };
      },
    });

    expect(calls.map((call) => call.args.slice(0, 3))).toEqual([
      ["mcp", "add", "coasonix"],
      ["mcp", "get", "coasonix"],
      ["mcp", "list"],
    ]);
  });

  test("fails setup if codex mcp list does not include coasonix", async () => {
    await expect(
      setupCodexMcp({
        repoRoot,
        targetRepo: "D:\\work\\target-repo",
        codexCommand: "codex",
        bunCommand: "bun",
        profile: "mock",
        buildRuntimeWorker: false,
        run: async (_command, args) => {
          if (args[1] === "add") {
            return { exitCode: 0, stdout: "Added global MCP server 'coasonix'.", stderr: "" };
          }
          if (args[1] === "get") {
            return { exitCode: 0, stdout: "coasonix\n", stderr: "" };
          }
          if (args[1] === "list") {
            return { exitCode: 0, stdout: "other-server enabled\n", stderr: "" };
          }
          return { exitCode: 1, stdout: "", stderr: `unexpected command: ${args.join(" ")}` };
        },
      }),
    ).rejects.toThrow("codex mcp list did not include coasonix");
  });

  test("root package exposes setup:codex-mcp", () => {
    const packageJson = JSON.parse(readFileSync(resolve(repoRoot, "package.json"), "utf8"));

    expect(packageJson.scripts?.["setup:codex-mcp"]).toBe(
      "bun packages/reasonix-expert-mcp/src/codex/setup.ts",
    );
  });

  test("mock profile worker emits review_result_v1 through the generated command", async () => {
    const command = buildCodexMcpAddCommand({
      repoRoot,
      targetRepo: "D:\\work\\target-repo",
      codexCommand: "codex",
      bunCommand: "bun",
      profile: "mock",
    });
    const workerEnv = command.args.find((arg) => arg.startsWith("COASONIX_REASONIX_COMMAND_JSON="));
    const workerArgv = JSON.parse(workerEnv!.replace("COASONIX_REASONIX_COMMAND_JSON=", ""));
    const worker = Bun.spawn(workerArgv, {
      stdin: "pipe",
      stdout: "pipe",
      stderr: "pipe",
    });

    worker.stdin.write(
      JSON.stringify({
        schema_version: "review_diff_input_v1",
        task_id: "TASK-setup-worker",
        request_id: "REQ-setup-worker",
      }),
    );
    worker.stdin.end();

    const [stdout, stderr, exitCode] = await Promise.all([
      new Response(worker.stdout).text(),
      new Response(worker.stderr).text(),
      worker.exited,
    ]);

    expect(exitCode).toBe(0);
    expect(stderr).toBe("");
    expect(JSON.parse(stdout)).toMatchObject({
      schema_version: "review_result_v1",
      task_id: "TASK-setup-worker",
      request_id: "REQ-setup-worker",
      status: "ok",
    });
  });

  test("--help prints usage and exits zero", async () => {
    const child = Bun.spawn([process.execPath, "packages/reasonix-expert-mcp/src/codex/setup.ts", "--help"], {
      cwd: repoRoot,
      stdout: "pipe",
      stderr: "pipe",
    });

    const [exitCode, stdout, stderr] = await Promise.all([
      child.exited,
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
    ]);

    expect(exitCode).toBe(0);
    expect(stderr).toBe("");
    expect(stdout).toContain("Usage:");
    expect(stdout).toContain("--target-repo");
    expect(stdout).toContain("--profile");
  });
});
