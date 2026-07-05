import { stdin, stdout, stderr } from "node:process";

if (process.argv.at(-1) !== "review-diff") {
  stderr.write("mock worker only supports review-diff\n");
  process.exit(2);
}

let input: Record<string, unknown>;
try {
  input = JSON.parse(await new Response(stdin).text());
} catch {
  stderr.write("mock worker received invalid JSON\n");
  process.exit(2);
}

stdout.write(
  JSON.stringify({
    schema_version: "review_result_v1",
    task_id: input.task_id,
    request_id: input.request_id,
    status: "ok",
    verdict: "pass",
    summary: "Mock worker completed review_diff.",
    confidence: 0.9,
  }),
);
