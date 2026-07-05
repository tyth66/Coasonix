import { afterEach, describe, expect, test } from "bun:test";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { existsSync, mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const repoRoot = resolve(import.meta.dir, "../../../..");
const serverEntry = join(repoRoot, "packages/reasonix-expert-mcp/src/index.ts");
const processes: ReturnType<typeof Bun.spawn>[] = [];

afterEach(async () => {
  await Promise.allSettled(
    processes.splice(0).map(async (process) => {
      try {
        process.stdin.end();
      } catch {
        // Process may already have exited.
      }
      try {
        process.kill();
      } catch {
        // Process may already have exited.
      }
      await process.exited.catch(() => undefined);
    }),
  );
});

describe("reasonix-expert MCP stdio server", () => {
  test("missing required config exits nonzero without serving tools", async () => {
    const child = Bun.spawn([processExec(), serverEntry], {
      cwd: repoRoot,
      env: { PATH: process.env.PATH ?? "" },
      stdin: "pipe",
      stdout: "pipe",
      stderr: "pipe",
    });

    const [exitCode, stdout, stderr] = await Promise.all([
      child.exited,
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
    ]);

    expect(exitCode).not.toBe(0);
    expect(stdout.trim()).toBe("");
    expect(stderr).toBeTruthy();
  });

  test("tools/list and tools/call work through the real stdio server process", async () => {
    const fixture = createFixture("success");
    const workerPath = await runtimeWorkerPath();
    const reasonix = writeMockReasonix("success");
    const server = startServer({
      COASONIX_REPO_ROOT: fixture.repo,
      COASONIX_RUNTIME_WORKER: workerPath,
      COASONIX_AGENT_COMMAND_JSON: JSON.stringify([reasonix, "review-diff"]),
    });

    const initialize = await server.request("initialize", {
      protocolVersion: "2025-06-18",
      capabilities: {},
      clientInfo: { name: "coasonix-test", version: "0.0.0" },
    });
    expect(initialize.result).toMatchObject({
      capabilities: { tools: {} },
      serverInfo: { name: "reasonix-expert-mcp" },
    });

    const listed = await server.request("tools/list", {});
    expect(listed.result.tools.map((tool: { name: string }) => tool.name)).toEqual([
      "reasonix.review_diff",
    ]);

    const result = await server.request("tools/call", {
      name: "reasonix.review_diff",
      arguments: reviewDiffInput("TASK-server-success", "REQ-server-success", fixture.repo),
    });

    expect(result.result.isError).toBe(false);
    expect(result.result.structuredContent).toMatchObject({
      schema_version: "review_result_v1",
      task_id: "TASK-server-success",
      request_id: "REQ-server-success",
      status: "ok",
    });
  });

  test("official MCP SDK client can list and call review_diff", async () => {
    const fixture = createFixture("sdk-success");
    const workerPath = await runtimeWorkerPath();
    const reasonix = writeMockReasonix("success");
    const transport = new StdioClientTransport({
      command: processExec(),
      args: [serverEntry],
      cwd: repoRoot,
      stderr: "pipe",
      env: {
        ...process.env,
        COASONIX_REPO_ROOT: fixture.repo,
        COASONIX_RUNTIME_WORKER: workerPath,
        COASONIX_AGENT_COMMAND_JSON: JSON.stringify([reasonix, "review-diff"]),
      },
    });
    const client = new Client({ name: "coasonix-sdk-test", version: "0.0.0" });

    await client.connect(transport);
    try {
      const listed = await client.listTools();
      expect(listed.tools.map((tool) => tool.name)).toEqual(["reasonix.review_diff"]);

      const result = await client.callTool({
        name: "reasonix.review_diff",
        arguments: reviewDiffInput("TASK-sdk-success", "REQ-sdk-success", fixture.repo),
      });

      expect(result.isError).toBe(false);
      expect(result.structuredContent).toMatchObject({
        schema_version: "review_result_v1",
        task_id: "TASK-sdk-success",
        request_id: "REQ-sdk-success",
        status: "ok",
      });
    } finally {
      await client.close();
    }
  });

  test("runtime deny through the real server does not invoke mock Reasonix", async () => {
    const fixture = createFixture("deny");
    const marker = join(fixture.repo, "reasonix-invoked.txt");
    const workerPath = await runtimeWorkerPath();
    const reasonix = writeMockReasonix("success", marker);
    const server = startServer({
      COASONIX_REPO_ROOT: fixture.repo,
      COASONIX_RUNTIME_WORKER: workerPath,
      COASONIX_AGENT_COMMAND_JSON: JSON.stringify([reasonix, "review-diff"]),
    });

    await server.request("initialize", {
      protocolVersion: "2025-06-18",
      capabilities: {},
      clientInfo: { name: "coasonix-test", version: "0.0.0" },
    });

    const result = await server.request("tools/call", {
      name: "reasonix.review_diff",
      arguments: {
        ...reviewDiffInput("TASK-server-deny", "REQ-server-deny", fixture.repo),
        artifacts: { diff_path: ".agent/secrets/current.diff" },
      },
    });

    expect(result.result.isError).toBe(true);
    expect(result.result.content[0].text).toContain("runtime_policy_denied");
    expect(existsSync(marker)).toBe(false);
  });

  test("transport close exits the server process cleanly", async () => {
    const fixture = createFixture("shutdown");
    const workerPath = await runtimeWorkerPath();
    const reasonix = writeMockReasonix("success");
    const child = Bun.spawn([processExec(), serverEntry], {
      cwd: repoRoot,
      env: {
        ...process.env,
        COASONIX_REPO_ROOT: fixture.repo,
        COASONIX_RUNTIME_WORKER: workerPath,
        COASONIX_AGENT_COMMAND_JSON: JSON.stringify([reasonix, "review-diff"]),
      },
      stdin: "pipe",
      stdout: "pipe",
      stderr: "pipe",
    });
    processes.push(child);

    child.stdin.end();
    const exitCode = await child.exited;
    const stdout = await new Response(child.stdout).text();
    const stderr = await new Response(child.stderr).text();

    expect(exitCode).toBe(0);
    expect(stdout.trim()).toBe("");
    expect(stderr.trim()).toBe("");
  });

  test("package start script writes JSON-RPC frames only to stdout", async () => {
    const fixture = createFixture("start-script");
    const workerPath = await runtimeWorkerPath();
    const child = Bun.spawn([processExec(), "run", "--silent", `--cwd=${join(repoRoot, "packages/reasonix-expert-mcp")}`, "start:mcp"], {
      cwd: repoRoot,
      env: {
        ...process.env,
        COASONIX_REPO_ROOT: fixture.repo,
        COASONIX_RUNTIME_WORKER: workerPath,
        COASONIX_AGENT_COMMAND_JSON: JSON.stringify([processExec()]),
      },
      stdin: "pipe",
      stdout: "pipe",
      stderr: "pipe",
    });
    processes.push(child);

    child.stdin.write(
      `${JSON.stringify({
        jsonrpc: "2.0",
        id: 1,
        method: "initialize",
        params: {
          protocolVersion: "2025-06-18",
          capabilities: {},
          clientInfo: { name: "coasonix-start-script-test", version: "0.0.0" },
        },
      })}\n`,
    );
    child.stdin.flush();

    const reader = child.stdout.getReader();
    const { value, done } = await reader.read();
    expect(done).toBe(false);

    const firstLine = new TextDecoder().decode(value).split("\n")[0];
    expect(() => JSON.parse(firstLine)).not.toThrow();
    expect(JSON.parse(firstLine)).toMatchObject({
      jsonrpc: "2.0",
      id: 1,
      result: { serverInfo: { name: "reasonix-expert-mcp" } },
    });
  });
});

function processExec(): string {
  return process.execPath;
}

function startServer(env: Record<string, string>) {
  const child = Bun.spawn([processExec(), serverEntry], {
    cwd: repoRoot,
    env: { ...process.env, ...env },
    stdin: "pipe",
    stdout: "pipe",
    stderr: "pipe",
  });
  processes.push(child);

  const reader = child.stdout.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  let nextId = 1;

  async function readResponse(id: number): Promise<any> {
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

      const { value, done } = await reader.read();
      if (done) {
        throw new Error("server stdout closed before response");
      }
      buffer += decoder.decode(value, { stream: true });
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

async function runtimeWorkerPath(): Promise<string> {
  const exe =
    process.platform === "win32" ? "coasonix-runtime-worker.exe" : "coasonix-runtime-worker";
  const path = join(repoRoot, "target", "debug", exe);
  if (!existsSync(path)) {
    const build = Bun.spawnSync(["cargo", "build", "-p", "coasonix-runtime-worker"], {
      cwd: repoRoot,
      stdout: "pipe",
      stderr: "pipe",
    });
    if (!build.success) {
      throw new Error(`failed to build runtime worker: ${build.stderr.toString()}`);
    }
  }
  return path;
}

function createFixture(name: string) {
  const repo = mkdtempSync(join(tmpdir(), `coasonix-server-${name}-`));
  mkdirSync(join(repo, ".agent", "diffs"), { recursive: true });
  mkdirSync(join(repo, ".agent", "results"), { recursive: true });
  writeFileSync(join(repo, ".agent", "diffs", "current.diff"), "diff --git a/a.txt b/a.txt\n");
  return { repo };
}

function reviewDiffInput(taskId: string, requestId: string, repo: string) {
  return {
    schema_version: "review_diff_input_v1",
    task_id: taskId,
    request_id: requestId,
    mode: "review_diff",
    goal: "Review the current diff.",
    repo: { root: repo },
    artifacts: { diff_path: ".agent/diffs/current.diff" },
    permission_level: "L1_DIFF_REVIEW",
    output_schema: "review_result_v1",
  };
}

function writeMockReasonix(mode: string, markerPath?: string): string {
  const dir = mkdtempSync(join(tmpdir(), "coasonix-server-mock-reasonix-"));
  const script = join(dir, "mock-reasonix.mjs");
  writeFileSync(
    script,
    `
      import { mkdirSync, writeFileSync } from "node:fs";
      import { dirname } from "node:path";
      import { stdin, stdout, stderr } from "node:process";

      const mode = ${JSON.stringify(mode)};
      const markerPath = ${JSON.stringify(markerPath ?? null)};
      if (markerPath) {
        mkdirSync(dirname(markerPath), { recursive: true });
        writeFileSync(markerPath, "invoked");
      }
      const input = JSON.parse(await new Response(stdin).text());
      const result = {
        schema_version: "review_result_v1",
        task_id: input.task_id,
        request_id: input.request_id,
        status: "ok",
        verdict: "pass",
        summary: "No findings.",
        confidence: 0.9
      };

      switch (mode) {
        case "success":
          stdout.write(JSON.stringify(result));
          break;
        default:
          stderr.write("unknown mode");
          process.exit(2);
      }
    `,
  );

  if (process.platform !== "win32") {
    return process.execPath;
  }

  const command = join(dir, "mock-reasonix.cmd");
  writeFileSync(command, `@echo off\r\n"${process.execPath}" "${script}" %*\r\n`);
  return command;
}


