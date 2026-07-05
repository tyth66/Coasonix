import { resolve } from "node:path";

import { ERROR_CODES, errorLayerForCode } from "./agent/error-taxonomy";

export interface ServerConfig {
  repoRoot: string;
  runtimeWorker: string;
  agentCommand: string[];
  runtimeRequestTimeoutMs: number;
  agentTimeoutMs: number;
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
  const missing = ["COASONIX_REPO_ROOT", "COASONIX_RUNTIME_WORKER"].filter((key) => !env[key]);

  // COASONIX_AGENT_COMMAND_JSON first, fall back to legacy COASONIX_REASONIX_COMMAND_JSON
  const agentCmdJson = env.COASONIX_AGENT_COMMAND_JSON ?? env.COASONIX_REASONIX_COMMAND_JSON;
  const agentCmd = env.COASONIX_AGENT_COMMAND ?? env.COASONIX_REASONIX_COMMAND;
  if (!agentCmdJson && !agentCmd) {
    missing.push("COASONIX_AGENT_COMMAND_JSON or COASONIX_AGENT_COMMAND");
  }

  if (missing.length > 0) {
    throw new ConfigError(`Missing required configuration: ${missing.join(", ")}`);
  }

  return {
    repoRoot: resolve(required(env.COASONIX_REPO_ROOT, "COASONIX_REPO_ROOT")),
    runtimeWorker: resolve(required(env.COASONIX_RUNTIME_WORKER, "COASONIX_RUNTIME_WORKER")),
    agentCommand: parseAgentCommand(agentCmdJson, agentCmd),
    runtimeRequestTimeoutMs: parsePositiveInteger(
      env.COASONIX_RUNTIME_REQUEST_TIMEOUT_MS,
      2_000,
      "COASONIX_RUNTIME_REQUEST_TIMEOUT_MS",
    ),
    agentTimeoutMs: parsePositiveInteger(
      env.COASONIX_AGENT_TIMEOUT_MS ?? env.COASONIX_REASONIX_TIMEOUT_MS,
      10_000,
      "COASONIX_AGENT_TIMEOUT_MS",
    ),
  };
}

function required(value: string | undefined, name: string): string {
  if (!value) {
    throw new ConfigError(`Missing required configuration: ${name}`);
  }
  return value;
}

function parseAgentCommand(jsonCmd: string | undefined, plainCmd: string | undefined): string[] {
  if (jsonCmd) {
    let parsed: unknown;
    try {
      parsed = JSON.parse(jsonCmd);
    } catch (error) {
      throw new ConfigError(`Invalid COASONIX_AGENT_COMMAND_JSON: ${String(error)}`);
    }
    if (
      !Array.isArray(parsed) ||
      parsed.length === 0 ||
      parsed.some((item) => typeof item !== "string" || item.length === 0)
    ) {
      throw new ConfigError("COASONIX_AGENT_COMMAND_JSON must be a non-empty string array");
    }
    return parsed;
  }

  const raw = (plainCmd ?? "").trim();
  if (!raw) {
    throw new ConfigError("COASONIX_AGENT_COMMAND cannot be empty");
  }
  if (/["']/.test(raw)) {
    throw new ConfigError(
      "COASONIX_AGENT_COMMAND contains ambiguous quoting; use COASONIX_AGENT_COMMAND_JSON",
    );
  }
  return raw.split(/\s+/);
}

function parsePositiveInteger(value: string | undefined, fallback: number, name: string): number {
  if (!value) {
    return fallback;
  }
  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed <= 0) {
    throw new ConfigError(`${name} must be a positive integer`);
  }
  return parsed;
}
