import { describe, expect, test } from "bun:test";

import { loadServerConfig } from "./config";

describe("loadServerConfig", () => {
  test("does not require COASONIX_SCHEMA_PATH for the runtime architecture path", () => {
    const config = loadServerConfig({
      COASONIX_REPO_ROOT: "D:\\work\\target-repo",
      COASONIX_RUNTIME_WORKER: "D:\\Coasonix\\target\\debug\\coasonix-runtime-worker.exe",
      COASONIX_AGENT_COMMAND_JSON: JSON.stringify([
        "D:\\Coasonix\\bin\\coasonix-mock-worker.cmd",
        "review-diff",
      ]),
    });

    expect(config.repoRoot).toContain("target-repo");
    expect(config.runtimeWorker).toContain("coasonix-runtime-worker");
    expect(config.agentCommand[1]).toBe("review-diff");
  });
});


