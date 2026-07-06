import { resolve } from "node:path";

import { ERROR_CODES, errorLayerForCode } from "./agent/error-taxonomy";

// ---- Backend selection ----

export type BackendId = "reasonix" | "mock";

export interface ServerConfig {
  repoRoot: string;
  runtimeWorker: string;
  agentCommand: string[];
  runtimeRequestTimeoutMs: number;
  agentTimeoutMs: number;
  backend: BackendId;
  // Valid only when backend === "reasonix"
  reasonixModel: string;
}

type Environment = Record<string, string | undefined>;

export class ConfigError extends Error {
  readonly code = ERROR_CODES.CONFIG_MISSING;
  readonly layer = errorLayerForCode(ERROR_CODES.CONFIG_MISSING);

  constructor(message: string) {
    super(`${ERROR_CODES.CONFIG_MISSING}: ${message}`);
    this.name = "ConfigError";
  }
}

export function loadServerConfig(env: Environment = process.env): ServerConfig {
  const missing = ["COAGENT_REPO_ROOT", "COAGENT_RUNTIME_WORKER"].filter((key) => !env[key]);

  const agentCmdJson = env.COAGENT_AGENT_COMMAND_JSON;
  const agentCmd = env.COAGENT_AGENT_COMMAND;
  if (!agentCmdJson && !agentCmd) {
    missing.push("COAGENT_AGENT_COMMAND_JSON or COAGENT_AGENT_COMMAND");
  }

  if (missing.length > 0) {
    throw new ConfigError(`Missing required configuration: ${missing.join(", ")}`);
  }

  const backend = normalizeBackend(env.COAGENT_BACKEND);
  if (backend === "reasonix" && !env.COAGENT_REASONIX_MODEL) {
    throw new ConfigError("COAGENT_REASONIX_MODEL is required when COAGENT_BACKEND=reasonix");
  }

  return {
    repoRoot: resolve(required(env.COAGENT_REPO_ROOT, "COAGENT_REPO_ROOT")),
    runtimeWorker: resolve(required(env.COAGENT_RUNTIME_WORKER, "COAGENT_RUNTIME_WORKER")),
    agentCommand: parseAgentCommand(agentCmdJson, agentCmd),
    runtimeRequestTimeoutMs: parsePositiveInteger(
      env.COAGENT_RUNTIME_REQUEST_TIMEOUT_MS,
      2_000,
      "COAGENT_RUNTIME_REQUEST_TIMEOUT_MS",
    ),
    agentTimeoutMs: parsePositiveInteger(
      env.COAGENT_AGENT_TIMEOUT_MS ?? env.COAGENT_AGENT_TIMEOUT_MS,
      120_000,
      "COAGENT_AGENT_TIMEOUT_MS",
    ),
    backend,
    reasonixModel: env.COAGENT_REASONIX_MODEL ?? "",
  };
}

function normalizeBackend(value: string | undefined): BackendId {
  if (!value || value === "mock") return "mock";
  if (value === "reasonix") return "reasonix";
  throw new ConfigError(`COAGENT_BACKEND must be "reasonix" or "mock", got "${value}"`);
}

function required(value: string | undefined, name: string): string {
  if (!value) throw new ConfigError(`Missing required configuration: ${name}`);
  return value;
}

function parseAgentCommand(jsonCmd: string | undefined, plainCmd: string | undefined): string[] {
  if (jsonCmd) {
    let parsed: unknown;
    try { parsed = JSON.parse(jsonCmd); } catch (error) {
      throw new ConfigError(`Invalid COAGENT_AGENT_COMMAND_JSON: ${String(error)}`);
    }
    if (!Array.isArray(parsed) || parsed.length === 0 || parsed.some((item) => typeof item !== "string" || item.length === 0)) {
      throw new ConfigError("COAGENT_AGENT_COMMAND_JSON must be a non-empty string array");
    }
    return parsed;
  }
  const raw = (plainCmd ?? "").trim();
  if (!raw) throw new ConfigError("COAGENT_AGENT_COMMAND cannot be empty");
  if (/["']/.test(raw)) throw new ConfigError("COAGENT_AGENT_COMMAND contains ambiguous quoting; use COAGENT_AGENT_COMMAND_JSON");
  return raw.split(/\s+/);
}

function parsePositiveInteger(value: string | undefined, fallback: number, name: string): number {
  if (!value) return fallback;
  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed <= 0) throw new ConfigError(`${name} must be a positive integer`);
  return parsed;
}
