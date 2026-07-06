import { afterEach, describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { RuntimeWorkerClient, RuntimeWorkerError } from "./RuntimeWorkerClient";
import { encodeRequestFrame, parseResponseFrame } from "./protocol";

const clients: RuntimeWorkerClient[] = [];

afterEach(async () => {
  await Promise.allSettled(clients.splice(0).map((client) => client.shutdown()));
});

describe("JSON-RPC worker protocol", () => {
  test("encodes one complete JSON-RPC request per line", () => {
    const frame = encodeRequestFrame("REQ-frame", "runtime.initialize", {
      repo_root: "D:/repo",
    });

    expect(frame.endsWith("\n")).toBe(true);
    expect(frame.trim()).not.toContain("\n");
    expect(JSON.parse(frame)).toEqual({
      jsonrpc: "2.0",
      id: "REQ-frame",
      method: "runtime.initialize",
      params: { repo_root: "D:/repo" },
    });
  });

  test("rejects non JSON-RPC response frames", () => {
    expect(() => parseResponseFrame("not-json\n")).toThrow(RuntimeWorkerError);
    expect(() => parseResponseFrame(JSON.stringify({ ok: true }) + "\n")).toThrow(
      RuntimeWorkerError,
    );
  });
});

describe("RuntimeWorkerClient", () => {
  test("sends framed requests and receives JSON-RPC results", async () => {
    const client = track(
      new RuntimeWorkerClient({
        command: [process.execPath, writeResponsiveWorker()],
        requestTimeoutMs: 1_000,
      }),
    );

    const result = await client.call("runtime.initialize", {
      repo_root: "D:/repo",
    });

    expect(result).toEqual({
      method: "runtime.initialize",
      requestId: "REQ-1",
    });
  });

  test("shutdown is explicit", async () => {
    const client = track(
      new RuntimeWorkerClient({
        command: [process.execPath, writeResponsiveWorker()],
        requestTimeoutMs: 1_000,
      }),
    );

    await client.call("runtime.initialize", {});
    const result = await client.shutdown();

    expect(result).toEqual({ shutdown: true });
    expect(client.isRunning()).toBe(false);
  });

  test("timeout maps to runtime_unavailable and stops the worker", async () => {
    const client = track(
      new RuntimeWorkerClient({
        command: [process.execPath, writeHangingWorker()],
        requestTimeoutMs: 25,
      }),
    );

    await expect(client.call("runtime.initialize", {})).rejects.toMatchObject({
      code: "runtime_unavailable",
    });
    expect(client.isRunning()).toBe(false);
  });

  test("worker crash maps to runtime_unavailable", async () => {
    const client = track(
      new RuntimeWorkerClient({
        command: [process.execPath, writeCrashingWorker()],
        requestTimeoutMs: 1_000,
      }),
    );

    await expect(client.call("runtime.initialize", {})).rejects.toMatchObject({
      code: "runtime_unavailable",
    });
    expect(client.isRunning()).toBe(false);
  });

  test("worker JSON-RPC runtime_unavailable errors use symbolic codes", async () => {
    const client = track(
      new RuntimeWorkerClient({
        command: [process.execPath, writeRuntimeUnavailableWorker()],
        requestTimeoutMs: 1_000,
      }),
    );

    await expect(client.call("runtime.evaluate_operation", {})).rejects.toMatchObject({
      code: "runtime_unavailable",
    });
  });

  test("missing worker executable maps to runtime_unavailable", async () => {
    const client = track(
      new RuntimeWorkerClient({
        command: ["coagent-worker-does-not-exist"],
        requestTimeoutMs: 100,
      }),
    );

    await expect(client.call("runtime.initialize", {})).rejects.toMatchObject({
      code: "runtime_unavailable",
    });
    expect(client.isRunning()).toBe(false);
  });

  test("restart replaces the worker process", async () => {
    const client = track(
      new RuntimeWorkerClient({
        command: [process.execPath, writeResponsiveWorker()],
        requestTimeoutMs: 1_000,
      }),
    );

    const before = await client.call("runtime.initialize", {});
    await client.restart();
    const after = await client.call("runtime.initialize", {});

    expect(before).toEqual({ method: "runtime.initialize", requestId: "REQ-1" });
    expect(after).toEqual({ method: "runtime.initialize", requestId: "REQ-2" });
  });
});

function track(client: RuntimeWorkerClient): RuntimeWorkerClient {
  clients.push(client);
  return client;
}

function writeResponsiveWorker(): string {
  return writeWorkerScript(`
    import { createInterface } from "node:readline";
    import { stdin, stdout } from "node:process";
    const rl = createInterface({ input: stdin });
    rl.on("line", (line) => {
      const request = JSON.parse(line);
      const result = request.method === "runtime.shutdown"
        ? { shutdown: true }
        : { method: request.method, requestId: request.id };
      stdout.write(JSON.stringify({ jsonrpc: "2.0", id: request.id, result }) + "\\n");
      if (request.method === "runtime.shutdown") process.exit(0);
    });
  `);
}

function writeHangingWorker(): string {
  return writeWorkerScript("setInterval(() => {}, 1000);");
}

function writeCrashingWorker(): string {
  return writeWorkerScript("process.exit(42);");
}

function writeRuntimeUnavailableWorker(): string {
  return writeWorkerScript(`
    import { createInterface } from "node:readline";
    import { stdin, stdout } from "node:process";
    const rl = createInterface({ input: stdin });
    rl.on("line", (line) => {
      const request = JSON.parse(line);
      stdout.write(JSON.stringify({
        jsonrpc: "2.0",
        id: request.id,
        error: { code: -32008, message: "runtime_unavailable" }
      }) + "\\n");
    });
  `);
}

function writeWorkerScript(source: string): string {
  const dir = mkdtempSync(join(tmpdir(), "coagent-worker-client-"));
  const path = join(dir, "worker.mjs");
  writeFileSync(path, source);
  return path;
}

