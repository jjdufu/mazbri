import assert from "node:assert/strict";
import { test } from "node:test";
import { ROLES_ADMIN, ROLES_ALL, ROLES_BIZ } from "../roles";
import { resolveOpsTab } from "../tabs";

const TABS = [
  { id: "red", roles: ROLES_ALL, label: "RED" },
  { id: "latency", roles: ROLES_BIZ, label: "Latency" },
  { id: "errors", roles: ROLES_BIZ, label: "Errors" },
  { id: "breaker", roles: ROLES_ADMIN, label: "Breaker" },
] as const;

test("VIEWER only sees ALL tabs and falls back from ADMIN deep link", () => {
  const result = resolveOpsTab({
    role: "R_VIEWER",
    tabs: [...TABS],
    queryTab: "breaker",
    preferredDefault: "red",
  });
  assert.deepEqual(
    result.visibleTabs.map((tab) => tab.id),
    ["red"],
  );
  assert.equal(result.activeTab, "red");
  assert.equal(result.fellBack, true);
});

test("FINANCE sees BIZ tabs and keeps deep link to latency", () => {
  const result = resolveOpsTab({
    role: "R_FINANCE",
    tabs: [...TABS],
    queryTab: "latency",
  });
  assert.deepEqual(
    result.visibleTabs.map((tab) => tab.id),
    ["red", "latency", "errors"],
  );
  assert.equal(result.activeTab, "latency");
  assert.equal(result.fellBack, false);
});

test("ADMIN sees every tab; unknown query falls back to preferred", () => {
  const result = resolveOpsTab({
    role: "R_ADMIN",
    tabs: [...TABS],
    queryTab: "nope",
    preferredDefault: "errors",
  });
  assert.equal(result.visibleTabs.length, 4);
  assert.equal(result.activeTab, "errors");
  assert.equal(result.fellBack, true);
});

test("array query tab values are normalized", () => {
  const result = resolveOpsTab({
    role: "R_ADMIN",
    tabs: [...TABS],
    queryTab: ["breaker", "red"],
  });
  assert.equal(result.activeTab, "breaker");
  assert.equal(result.fellBack, false);
});
