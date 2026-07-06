// Standalone mock Reasonix worker for conformance testing.
// Reads a review_diff_input_v1 JSON object from stdin and writes a
// valid review_result_v1 JSON object to stdout.
//
// Used by bin/coasonix-mock-worker.cmd and by worker-contract conformance tests.

const decoder = new TextDecoder();
const chunks: Uint8Array[] = [];

for await (const chunk of Bun.stdin.stream() as AsyncIterable<Uint8Array>) {
  chunks.push(chunk);
}

const raw = decoder.decode(Bun.concatArrayBuffers(chunks as unknown as ArrayBuffer[])).trim();

let input: Record<string, unknown>;
try {
  input = JSON.parse(raw);
} catch {
  process.stderr.write("mock worker: invalid JSON on stdin\n");
  process.exit(1);
}

const output = {
  schema_version: "review_result_v1",
  task_id: input.task_id ?? "TASK-mock",
  request_id: input.request_id ?? "REQ-mock",
  status: "ok",
  verdict: "pass",
  summary: "Mock worker completed review.",
  findings: [] as Array<Record<string, unknown>>,
  tests_to_run: [] as string[],
  risks: [] as string[],
  assumptions: [] as string[],
  confidence: 0.9,
};

process.stdout.write(JSON.stringify(output));
process.exit(0);
