export type JsonObjectExtraction =
  | { ok: true; value: Record<string, unknown> }
  | { ok: false; error: string };

export function extractSingleJsonObject(stdout: string): JsonObjectExtraction {
  const trimmed = stdout.trim();
  if (!trimmed) {
    return { ok: false, error: "Reasonix stdout was empty" };
  }

  const fenced = extractFencedJson(trimmed);
  const candidate = fenced ?? trimmed;

  let parsed: unknown;
  try {
    parsed = JSON.parse(candidate);
  } catch {
    return { ok: false, error: "Reasonix stdout did not contain exactly one JSON object" };
  }

  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    return { ok: false, error: "Reasonix stdout JSON was not an object" };
  }

  return { ok: true, value: parsed as Record<string, unknown> };
}

function extractFencedJson(stdout: string): string | null {
  const matches = [...stdout.matchAll(/```(?:json)?\s*([\s\S]*?)```/g)];
  if (matches.length !== 1) {
    return null;
  }
  return matches[0][1].trim();
}
