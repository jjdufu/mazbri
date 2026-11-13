import assert from "node:assert/strict";
import { test } from "node:test";
import { resolveOpsTab } from "../tabs";
import {
  getMergedRouteTabs,
  MERGED_DEFAULT_TAB,
} from "../merged-route-tabs";
import { ROLES_ADMIN, ROLES_ALL, ROLES_BIZ } from "../roles";

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

test("traffic customer/upstream ALL; traces ADMIN-only", () => {
  const tabs = [...getMergedRouteTabs("traffic")];
  assert.deepEqual(
    tabs.map((tab) => tab.id),
    ["customer", "upstream", "traces"],
  );
  assert.deepEqual(tabs[0]?.roles, ROLES_ALL);
  assert.deepEqual(tabs[1]?.roles, ROLES_ALL);
  assert.deepEqual(tabs[2]?.roles, ROLES_ADMIN);

  const viewer = resolveOpsTab({
    role: "R_VIEWER",
    tabs,
    queryTab: "traces",
    preferredDefault: MERGED_DEFAULT_TAB.traffic,
  });
  assert.deepEqual(
    viewer.visibleTabs.map((tab) => tab.id),
    ["customer", "upstream"],
  );
  assert.equal(viewer.activeTab, "customer");
  assert.equal(viewer.fellBack, true);

  const finance = resolveOpsTab({
    role: "R_FINANCE",
    tabs,
    queryTab: "upstream",
    preferredDefault: MERGED_DEFAULT_TAB.traffic,
  });
  assert.deepEqual(
    finance.visibleTabs.map((tab) => tab.id),
    ["customer", "upstream"],
  );
  assert.equal(finance.activeTab, "upstream");

  const admin = resolveOpsTab({
    role: "R_ADMIN",
    tabs,
    queryTab: "traces",
    preferredDefault: MERGED_DEFAULT_TAB.traffic,
  });
  assert.deepEqual(
    admin.visibleTabs.map((tab) => tab.id),
    ["customer", "upstream", "traces"],
  );
  assert.equal(admin.activeTab, "traces");
});

test("runtime apps ALL; pipeline BIZ; middleware/hosts ADMIN-only", () => {
  const tabs = [...getMergedRouteTabs("runtime")];
  assert.deepEqual(
    tabs.map((tab) => tab.id),
    ["apps", "middleware", "pipeline", "hosts"],
  );
  assert.deepEqual(tabs[0]?.roles, ROLES_ALL);
  assert.deepEqual(tabs[1]?.roles, ROLES_ADMIN);
  assert.deepEqual(tabs[2]?.roles, ROLES_BIZ);
  assert.deepEqual(tabs[3]?.roles, ROLES_ADMIN);
  const viewer = resolveOpsTab({
    role: "R_VIEWER",
    tabs,
    queryTab: "pipeline",
    preferredDefault: MERGED_DEFAULT_TAB.runtime,
  });
  assert.deepEqual(
    viewer.visibleTabs.map((tab) => tab.id),
    ["apps"],
  );
  assert.equal(viewer.activeTab, "apps");

  const finance = resolveOpsTab({
    role: "R_FINANCE",
    tabs,
    queryTab: "pipeline",
    preferredDefault: MERGED_DEFAULT_TAB.runtime,
  });
  assert.deepEqual(
    finance.visibleTabs.map((tab) => tab.id),
    ["apps", "pipeline"],
  );
  assert.equal(finance.activeTab, "pipeline");
});
