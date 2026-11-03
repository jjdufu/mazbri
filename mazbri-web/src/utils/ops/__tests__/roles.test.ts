import assert from "node:assert/strict";
import { test } from "node:test";
import {
  canAccessRoles,
  filterByRoles,
  isAdminRole,
  isBizRole,
  isViewerRole,
  normalizeAdminRole,
  ROLES_ADMIN,
  ROLES_ALL,
  ROLES_BIZ,
} from "../roles";

test("normalizeAdminRole accepts only the three known codes", () => {
  assert.equal(normalizeAdminRole("R_ADMIN"), "R_ADMIN");
  assert.equal(normalizeAdminRole("R_FINANCE"), "R_FINANCE");
  assert.equal(normalizeAdminRole("R_VIEWER"), "R_VIEWER");
  assert.equal(normalizeAdminRole("r_admin"), undefined);
  assert.equal(normalizeAdminRole(undefined), undefined);
  assert.equal(normalizeAdminRole(null), undefined);
});

test("isAdmin / isBiz / isViewer mirror route_access ADMIN/BIZ/VIEWER", () => {
  assert.equal(isAdminRole("R_ADMIN"), true);
  assert.equal(isAdminRole("R_FINANCE"), false);
  assert.equal(isBizRole("R_ADMIN"), true);
  assert.equal(isBizRole("R_FINANCE"), true);
  assert.equal(isBizRole("R_VIEWER"), false);
  assert.equal(isViewerRole("R_VIEWER"), true);
  assert.equal(isViewerRole("R_ADMIN"), false);
});

test("canAccessRoles denies empty allow-list and unknown roles", () => {
  assert.equal(canAccessRoles("R_VIEWER", ROLES_ALL), true);
  assert.equal(canAccessRoles("R_VIEWER", ROLES_BIZ), false);
  assert.equal(canAccessRoles("R_FINANCE", ROLES_ADMIN), false);
  assert.equal(canAccessRoles("R_ADMIN", ROLES_ADMIN), true);
  assert.equal(canAccessRoles("R_ADMIN", []), false);
  assert.equal(canAccessRoles("R_ADMIN", null), false);
  assert.equal(canAccessRoles("R_UNKNOWN", ROLES_ALL), false);
});

test("filterByRoles keeps items without roles and drops unauthorized ones", () => {
  const items = [
    { id: "a" },
    { id: "b", roles: ROLES_ALL },
    { id: "c", roles: ROLES_BIZ },
    { id: "d", roles: ROLES_ADMIN },
  ];
  assert.deepEqual(
    filterByRoles("R_VIEWER", items).map((item) => item.id),
    ["a", "b"],
  );
  assert.deepEqual(
    filterByRoles("R_FINANCE", items).map((item) => item.id),
    ["a", "b", "c"],
  );
  assert.deepEqual(
    filterByRoles("R_ADMIN", items).map((item) => item.id),
    ["a", "b", "c", "d"],
  );
});
