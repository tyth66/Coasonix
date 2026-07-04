import { afterEach, describe, expect, test } from "bun:test";
import { existsSync, mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import { createReasonixToolsAdapter } from "../mcp/tools";
import { RuntimeWorkerClient } from "../worker/client";
import { ReasonixProcessRunner } from "./runner";

const repoRoot = resolve(import.meta.dir, "../../../..");
const schemaPath = join(repoRoot, "schemas/coasonix-v1.schema.json");
const clients: RuntimeWorkerClient[] = [];

afterEach(async () => {
  await Promise.allSettled(clients.splice(0).map((client) => client.shutdown()));
});

describe("mock Reasonix review_diff vertical slice", () => {
  test("success returns structuredContent through Rust validation", async () => {
    const result = await runVerticalSlice("success");

    expect(result.isError).toBe(false);
    expect(result.structuredContent).toMatchObject({
      schema_version: "review_result_v1",
      task_id: "TASK-vertical-success",
      request_id: "REQ-vertical-success",
      status: "ok",
    });
  });

  test("markdown-fenced JSON is normalized into structuredContent", async () => {
    const result = await runVerticalSlice("markdown-fenced-json");

    expect(result.isError).toBe(false);
    expect(result.structuredContent?.schema_version).toBe("review_result_v1");
  });

  test.each([
    ["timeout", "timeout"],
    ["malformed-json", "reasonix_failed"],
    ["multiple-json", "reasonix_failed"],
    ["nonzero-exit", "reasonix_failed"],
    ["stderr-only-failure", "reasonix_failed"],
    ["schema-mismatch", "schema_validation_failed"],
    ["wrong-task-id", "schema_validation_failed"],
    ["wrong-request-id", "schema_validation_failed"],
    ["invalid-confidence", "schema_validation_failed"],
  ])("%s returns isError true without trusted structuredContent", async (mode, status) => {
    const result = await runVerticalSlice(mode);

    expect(result.isError).toBe(true);
    expect(result.structuredContent).toBeUndefined();
    expect(result.content[0].text).toContain(status);
  });

  test("runtime deny prevents mock Reasonix invocation", async () => {
    const fixture = createFixture("runtime-deny");
    const marker = join(fixture.repo, "reasonix-invoked.txt");
    const client = await startRuntimeWorker(fixture.repo);
    const adapter = createReasonixToolsAdapter({
      initialized: true,
      runtime: client,
      reasonix: new ReasonixProcessRunner({
        command: [process.execPath, writeMockReasonix("success", marker)],
        timeoutMs: 1_000,
      }),
    });

    const result = await adapter.callTool({
      name: "reasonix.review_diff",
      arguments: {
        ...reviewDiffInput("TASK-runtime-deny", "REQ-runtime-deny", fixture.repo),
        artifacts: { diff_path: ".agent/secrets/current.diff" },
      },
    });

    expect(result.isError).toBe(true);
    expect(result.content[0].text).toContain("permission_denied");
    expect(existsSync(marker)).toBe(false);
  });
});

async function runVerticalSlice(mode: string) {
  const fixture = createFixture(mode);
  const client = await startRuntimeWorker(fixture.repo);
  const adapter = createReasonixToolsAdapter({
    initialized: true,
    runtime: client,
    reasonix: new ReasonixProcessRunner({
      command: [process.execPath, writeMockReasonix(mode)],
      timeoutMs: mode === "timeout" ? 50 : 1_000,
    }),
  });

  return adapter.callTool({
    name: "reasonix.review_diff",
    arguments: reviewDiffInput(
      `TASK-vertical-${mode.replaceAll("-", "_")}`,
      `REQ-vertical-${mode.replaceAll("-", "_")}`,
      fixture.repo,
    ),
  });
}

async function startRuntimeWorker(repo: string): Promise<RuntimeWorkerClient> {
  const client = new RuntimeWorkerClient({
    command: [runtimeWorkerPath()],
    requestTimeoutMs: 2_000,
  });
  clients.push(client);
  await client.call("runtime.initialize", {
    repo_root: repo,
    schema_path: schemaPath,
    reasonix_executable: "reasonix",
  });
  return client;
}

function runtimeWorkerPath(): string {
  const exe = process.platform === "win32" ? "coasonix-runtime-worker.exe" : "coasonix-runtime-worker";
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
  const repo = mkdtempSync(join(tmpdir(), `coasonix-vertical-${name}-`));
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
  const dir = mkdtempSync(join(tmpdir(), "coasonix-mock-reasonix-"));
  const path = join(dir, "mock-reasonix.mjs");
  writeFileSync(
    path,
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
        case "markdown-fenced-json":
          stdout.write("before\\n\`\`\`json\\n" + JSON.stringify(result) + "\\n\`\`\`\\nafter");
          break;
        case "timeout":
          setInterval(() => {}, 1000);
          break;
        case "malformed-json":
          stdout.write("not-json");
          break;
        case "multiple-json":
          stdout.write(JSON.stringify(result) + "\\n" + JSON.stringify(result));
          break;
        case "nonzero-exit":
          stderr.write("nonzero failure");
          process.exit(7);
          break;
        case "stderr-only-failure":
          stderr.write("stderr-only failure");
          process.exit(1);
          break;
        case "schema-mismatch":
          delete result.summary;
          stdout.write(JSON.stringify(result));
          break;
        case "wrong-task-id":
          result.task_id = "TASK-wrong";
          stdout.write(JSON.stringify(result));
          break;
        case "wrong-request-id":
          result.request_id = "REQ-wrong";
          stdout.write(JSON.stringify(result));
          break;
        case "invalid-confidence":
          result.confidence = 2;
          stdout.write(JSON.stringify(result));
          break;
        default:
          stderr.write("unknown mode");
          process.exit(2);
      }
    `,
  );
  return path;
}
