import { describe, expect, test } from "bun:test";

import { loadServerConfig } from "./config";

describe("loadServerConfig", () => {
  test("does not require COAGENT_SCHEMA_PATH for the runtime architecture path", () => {
    const config = loadServerConfig({
      COAGENT_REPO_ROOT: "D:\\work\\target-repo",
      COAGENT_RUNTIME_WORKER: "D:\\Coagent\\target\\debug\\coagent-runtime-worker.exe",
      COAGENT_AGENT_COMMAND_JSON: JSON.stringify([
        "D:\\Coagent\\bin\\coasonix-mock-worker.cmd",
        "review-diff",
      ]),
    });

    expect(config.repoRoot).toContain("target-repo");
    expect(config.runtimeWorker).toContain("coagent-runtime-worker");
    expect(config.agentCommand[1]).toBe("review-diff");
  });
});





