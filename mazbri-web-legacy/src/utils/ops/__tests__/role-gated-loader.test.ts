import assert from "node:assert/strict";
import { test } from "node:test";
import {
  runRoleGatedTasks,
  selectRoleGatedTasks,
} from "../role-gated-loader";

const TASKS = [
  { key: "overview", gate: "all" as const },
  { key: "billing", gate: "biz" as const },
  { key: "topology", gate: "admin" as const },
  { key: "custom", roles: ["R_FINANCE"] as const },
];

test("selectRoleGatedTasks drops BIZ/ADMIN work for VIEWER", () => {
  assert.deepEqual(
    selectRoleGatedTasks("R_VIEWER", TASKS).map((task) => task.key),
    ["overview"],
  );
});

test("selectRoleGatedTasks keeps BIZ for FINANCE and honors explicit roles", () => {
  assert.deepEqual(
    selectRoleGatedTasks("R_FINANCE", TASKS).map((task) => task.key),
    ["overview", "billing", "custom"],
  );
});

test("runRoleGatedTasks never invokes unauthorized factories", async () => {
  const called: string[] = [];
  const result = await runRoleGatedTasks({
    role: "R_VIEWER",
    tasks: TASKS,
    run: async (key) => {
      called.push(key);
      return `${key}-ok`;
    },
  });
  assert.deepEqual(called, ["overview"]);
  assert.deepEqual(result, { overview: "overview-ok" });
  assert.equal("billing" in result, false);
  assert.equal("topology" in result, false);
});
