import assert from "node:assert/strict";
import { test } from "node:test";
import { resolveOpsTab } from "../tabs";
import {
  getMergedRouteTabs,
  MERGED_DEFAULT_TAB,
} from "../merged-route-tabs";
import { ROLES_ADMIN, ROLES_ALL, ROLES_BIZ } from "../roles";

test("infra tabs are ADMIN-only", () => {
  const tabs = [...getMergedRouteTabs("infra")];
  assert.deepEqual(
    tabs.map((tab) => tab.id),
    ["pipeline", "middleware", "hosts"],
  );
  for (const tab of tabs) {
    assert.deepEqual(tab.roles, ROLES_ADMIN);
  }
});

test("VIEWER on alerts only sees events", () => {
  const result = resolveOpsTab({
    role: "R_VIEWER",
    tabs: [...getMergedRouteTabs("alerts")],
    queryTab: "rules",
    preferredDefault: MERGED_DEFAULT_TAB.alerts,
  });
  assert.deepEqual(
    result.visibleTabs.map((tab) => tab.id),
    ["events"],
  );
  assert.equal(result.activeTab, "events");
  assert.equal(result.fellBack, true);
});

test("VIEWER cannot see billing/flow/infra/reliability parents' tabs", () => {
  for (const domain of ["billing", "flow", "infra", "reliability"] as const) {
    const result = resolveOpsTab({
      role: "R_VIEWER",
      tabs: [...getMergedRouteTabs(domain)],
      preferredDefault: MERGED_DEFAULT_TAB[domain],
    });
    assert.equal(result.visibleTabs.length, 0, domain);
    assert.equal(result.activeTab, undefined, domain);
  }
});

test("traffic VIEWER only red; FINANCE gets latency/errors", () => {
  const viewer = resolveOpsTab({
    role: "R_VIEWER",
    tabs: [...getMergedRouteTabs("traffic")],
    queryTab: "latency",
    preferredDefault: MERGED_DEFAULT_TAB.traffic,
  });
  assert.deepEqual(
    viewer.visibleTabs.map((tab) => tab.id),
    ["red"],
  );
  assert.equal(viewer.activeTab, "red");

  const finance = resolveOpsTab({
    role: "R_FINANCE",
    tabs: [...getMergedRouteTabs("traffic")],
    queryTab: "errors",
  });
  assert.deepEqual(
    finance.visibleTabs.map((tab) => tab.id),
    ["red", "latency", "errors"],
  );
  assert.equal(finance.activeTab, "errors");
  assert.deepEqual(getMergedRouteTabs("traffic")[0]?.roles, ROLES_ALL);
  assert.deepEqual(getMergedRouteTabs("traffic")[1]?.roles, ROLES_BIZ);
});

test("upstream breaker is ADMIN; sla is ALL", () => {
  const tabs = getMergedRouteTabs("upstream");
  assert.deepEqual(tabs[0]?.roles, ROLES_ALL);
  assert.deepEqual(tabs[1]?.roles, ROLES_ADMIN);
  const viewer = resolveOpsTab({
    role: "R_VIEWER",
    tabs: [...tabs],
    queryTab: "breaker",
    preferredDefault: MERGED_DEFAULT_TAB.upstream,
  });
  assert.deepEqual(
    viewer.visibleTabs.map((tab) => tab.id),
    ["sla"],
  );
});

test("services topology ADMIN-only; health ALL", () => {
  const result = resolveOpsTab({
    role: "R_VIEWER",
    tabs: [...getMergedRouteTabs("services")],
    queryTab: "topology",
    preferredDefault: MERGED_DEFAULT_TAB.services,
  });
  assert.deepEqual(
    result.visibleTabs.map((tab) => tab.id),
    ["health"],
  );
  assert.equal(result.activeTab, "health");
});

test("flow cache is ADMIN; ratelimit is BIZ; FINANCE falls back from cache", () => {
  const tabs = getMergedRouteTabs("flow");
  assert.deepEqual(tabs[0]?.roles, ROLES_BIZ);
  assert.deepEqual(tabs[1]?.roles, ROLES_ADMIN);
  const finance = resolveOpsTab({
    role: "R_FINANCE",
    tabs: [...tabs],
    queryTab: "cache",
    preferredDefault: MERGED_DEFAULT_TAB.flow,
  });
  assert.deepEqual(
    finance.visibleTabs.map((tab) => tab.id),
    ["ratelimit"],
  );
  assert.equal(finance.activeTab, "ratelimit");
  assert.equal(finance.fellBack, true);

  const admin = resolveOpsTab({
    role: "R_ADMIN",
    tabs: [...tabs],
    queryTab: "cache",
    preferredDefault: MERGED_DEFAULT_TAB.flow,
  });
  assert.deepEqual(
    admin.visibleTabs.map((tab) => tab.id),
    ["ratelimit", "cache"],
  );
  assert.equal(admin.activeTab, "cache");
});
