import { createInterface } from "node:readline";
import type { Readable, Writable } from "node:stream";

import { loadServerConfig, type ServerConfig } from "../config";
import { AgentProcessRunner } from "../agents/process-runner";
import { RuntimeWorkerClient } from "../worker/client";
import { createReasonixToolsAdapter } from "./adapter";

interface JsonRpcRequest {
  jsonrpc: "2.0";
  id?: string | number | null;
  method?: string;
  params?: unknown;
}

interface RunningServer {
  ready: Promise<void>;
  shutdown(): Promise<void>;
}

export async function runServerFromEnv(): Promise<void> {
  const config = loadServerConfig();
  await runMcpServer(config, process.stdin, process.stdout, process.stderr);
}

export async function runMcpServer(
  config: ServerConfig,
  input: Readable,
  output: Writable,
  errorOutput: Writable,
): Promise<void> {
  const running = await startMcpServer(config, input, output, errorOutput);
  await running.ready;
}

async function startMcpServer(
  config: ServerConfig,
  input: Readable,
  output: Writable,
  errorOutput: Writable,
): Promise<RunningServer> {
  const runtime = new RuntimeWorkerClient({
    command: [config.runtimeWorker],
    requestTimeoutMs: config.runtimeRequestTimeoutMs,
  });
  let shuttingDown = false;

  const shutdown = async () => {
    if (shuttingDown) {
      return;
    }
    shuttingDown = true;
    await runtime.shutdown().catch((error) => {
      errorOutput.write(`runtime shutdown failed: ${formatError(error)}\n`);
    });
  };

  const handleSignal = () => {
    void shutdown().finally(() => process.exit(0));
  };
  process.once("SIGINT", handleSignal);
  process.once("SIGTERM", handleSignal);
  process.once("uncaughtException", (error) => {
    errorOutput.write(`fatal server error: ${formatError(error)}\n`);
    void shutdown().finally(() => process.exit(1));
  });
  process.once("unhandledRejection", (error) => {
    errorOutput.write(`fatal server rejection: ${formatError(error)}\n`);
    void shutdown().finally(() => process.exit(1));
  });

  try {
    await runtime.call("runtime.initialize", {
      repo_root: config.repoRoot,
      agent_executable: config.agentCommand[0],
    });
  } catch (error) {
    await shutdown();
    throw error;
  }

  const adapter = createReasonixToolsAdapter({
    initialized: true,
    runtime,
    agentCommand: config.agentCommand,
    reasonix: new AgentProcessRunner({
      command: config.agentCommand,
      timeoutMs: config.agentTimeoutMs,
    }),
  });

  const lineReader = createInterface({ input });
  const ready = (async () => {
    try {
      for await (const line of lineReader) {
        if (!line.trim()) {
          continue;
        }
        const response = await handleRequestLine(line, adapter);
        if (response) {
          output.write(`${JSON.stringify(response)}\n`);
        }
      }
    } finally {
      await shutdown();
    }
  })();

  return { ready, shutdown };
}

async function handleRequestLine(line: string, adapter: ReturnType<typeof createReasonixToolsAdapter>) {
  let value: unknown;
  try {
    value = JSON.parse(line);
  } catch {
    return errorResponse(null, -32700, "Parse error");
  }

  if (!isJsonRpcRequest(value)) {
    return errorResponse(null, -32600, "Invalid Request");
  }

  if (value.id === undefined || value.id === null) {
    return null;
  }

  switch (value.method) {
    case "initialize":
      return successResponse(value.id, {
        protocolVersion: protocolVersion(value.params),
        capabilities: { tools: {} },
        serverInfo: { name: "reasonix-expert-mcp", version: "0.1.0" },
      });
    case "notifications/initialized":
      return null;
    case "tools/list":
      return successResponse(value.id, adapter.listTools());
    case "tools/call":
      return successResponse(value.id, await adapter.callTool(toolCallParams(value.params)));
    default:
      return errorResponse(value.id, -32601, "Method not found");
  }
}

function isJsonRpcRequest(value: unknown): value is JsonRpcRequest {
  if (!value || typeof value !== "object") {
    return false;
  }
  const request = value as Record<string, unknown>;
  return request.jsonrpc === "2.0" && typeof request.method === "string";
}

function protocolVersion(params: unknown): string {
  if (params && typeof params === "object") {
    const value = (params as Record<string, unknown>).protocolVersion;
    if (typeof value === "string") {
      return value;
    }
  }
  return "2025-06-18";
}

function toolCallParams(params: unknown) {
  if (!params || typeof params !== "object") {
    return { name: "", arguments: undefined };
  }
  const value = params as Record<string, unknown>;
  return {
    name: typeof value.name === "string" ? value.name : "",
    arguments: value.arguments,
  };
}

function successResponse(id: string | number, result: unknown) {
  return { jsonrpc: "2.0", id, result };
}

function errorResponse(id: string | number | null, code: number, message: string) {
  return { jsonrpc: "2.0", id, error: { code, message } };
}

function formatError(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

