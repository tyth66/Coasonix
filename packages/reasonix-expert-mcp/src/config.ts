import { resolve } from "node:path";

import { ERROR_CODES, errorLayerForCode } from "./agent/error-taxonomy";

export interface ServerConfig {
  repoRoot: string;
  runtimeWorker: string;
  reasonixCommand: string[];
  runtimeRequestTimeoutMs: number;
  reasonixTimeoutMs: number;
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

  if (!env.COASONIX_REASONIX_COMMAND_JSON && !env.COASONIX_REASONIX_COMMAND) {
    missing.push("COASONIX_REASONIX_COMMAND_JSON or COASONIX_REASONIX_COMMAND");
  }

  if (missing.length > 0) {
    throw new ConfigError(`Missing required configuration: ${missing.join(", ")}`);
  }

  return {
    repoRoot: resolve(required(env.COASONIX_REPO_ROOT, "COASONIX_REPO_ROOT")),
    runtimeWorker: resolve(required(env.COASONIX_RUNTIME_WORKER, "COASONIX_RUNTIME_WORKER")),
    reasonixCommand: parseReasonixCommand(env),
    runtimeRequestTimeoutMs: parsePositiveInteger(
      env.COASONIX_RUNTIME_REQUEST_TIMEOUT_MS,
      2_000,
      "COASONIX_RUNTIME_REQUEST_TIMEOUT_MS",
    ),
    reasonixTimeoutMs: parsePositiveInteger(
      env.COASONIX_REASONIX_TIMEOUT_MS,
      10_000,
      "COASONIX_REASONIX_TIMEOUT_MS",
    ),
  };
}

function required(value: string | undefined, name: string): string {
  if (!value) {
    throw new ConfigError(`Missing required configuration: ${name}`);
  }
  return value;
}

function parseReasonixCommand(env: Environment): string[] {
  if (env.COASONIX_REASONIX_COMMAND_JSON) {
    let parsed: unknown;
    try {
      parsed = JSON.parse(env.COASONIX_REASONIX_COMMAND_JSON);
    } catch (error) {
      throw new ConfigError(`Invalid COASONIX_REASONIX_COMMAND_JSON: ${String(error)}`);
    }
    if (
      !Array.isArray(parsed) ||
      parsed.length === 0 ||
      parsed.some((item) => typeof item !== "string" || item.length === 0)
    ) {
      throw new ConfigError("COASONIX_REASONIX_COMMAND_JSON must be a non-empty string array");
    }
    return parsed;
  }

  const raw = required(env.COASONIX_REASONIX_COMMAND, "COASONIX_REASONIX_COMMAND").trim();
  if (!raw) {
    throw new ConfigError("COASONIX_REASONIX_COMMAND cannot be empty");
  }
  if (/["']/.test(raw)) {
    throw new ConfigError(
      "COASONIX_REASONIX_COMMAND contains ambiguous quoting; use COASONIX_REASONIX_COMMAND_JSON",
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
