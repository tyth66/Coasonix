import { expect, test } from "bun:test";
import { adapterScaffoldReady } from "./index";

test("adapter scaffold is importable", () => {
  expect(adapterScaffoldReady()).toBe(true);
});
