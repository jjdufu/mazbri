import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  ACTION_LABELS,
  FAILURE_REASON_LABELS,
  RESOURCE_TYPE_LABELS,
  SERVICE_LABELS,
  AUDIT_SERVICES,
  diffChanges,
  guardCsvFormula,
  statusLabelKey,
  statusTagType,
} from "../labels";

const zh = JSON.parse(
  readFileSync(
    new URL("../../../../../locales/langs/zh.json", import.meta.url),
    "utf8",
  ),
) as Record<string, unknown>;
const en = JSON.parse(
  readFileSync(
    new URL("../../../../../locales/langs/en.json", import.meta.url),
    "utf8",
  ),
) as Record<string, unknown>;

const lookup = (dict: Record<string, unknown>, key: string): unknown =>
  key
    .split(".")
    .reduce<unknown>(
      (node, part) =>
        node && typeof node === "object"
          ? (node as Record<string, unknown>)[part]
          : undefined,
      dict,
    );

const allKeys = [
  ...Object.values(ACTION_LABELS),
  ...Object.values(RESOURCE_TYPE_LABELS),
  ...Object.values(FAILURE_REASON_LABELS),
  ...Object.values(SERVICE_LABELS),
  "auditLogs.columns.service",
  statusLabelKey("success"),
  statusLabelKey("failure"),
  statusLabelKey("denied"),
];
for (const key of allKeys) {
  assert.equal(typeof lookup(zh, key), "string", `zh missing ${key}`);
  assert.equal(typeof lookup(en, key), "string", `en missing ${key}`);
}

for (const action of [
  "api.read",
  "api.post",
  "user.mfa_setup",
  "audit.export",
  "certificate.view_full",
  "admin.force_password_reset",
  "certificate.verify",
  "config_center.tag_create",
  "ratelimit.command_ack",
  "billing.reconciliation_run",
]) {
  assert.ok(ACTION_LABELS[action], `missing action label ${action}`);
}
assert.deepEqual(AUDIT_SERVICES, ["mgmt", "cert", "ratelimit"]);
for (const type of [
  "audit_log",
  "connector",
  "authorization",
  "billing",
  "ratelimit",
]) {
  assert.ok(RESOURCE_TYPE_LABELS[type], `missing resource label ${type}`);
}

assert.equal(statusTagType("success"), "success");
assert.equal(statusTagType("denied"), "warning");
assert.equal(statusTagType("failure"), "danger");
assert.equal(statusLabelKey("unknown"), "auditLogs.status.failure");

for (const dangerous of ["=1+1", "+cmd", "-2", "@SUM(A1)", "\tx", "\rx"]) {
  assert.equal(guardCsvFormula(dangerous), `'${dangerous}`);
}
for (const safe of ["admin", "1-2", "", "a=b"]) {
  assert.equal(guardCsvFormula(safe), safe);
}

assert.deepEqual(diffChanges(undefined, undefined), []);
assert.deepEqual(
  diffChanges(
    { a: 1, b: { x: 1, y: 2 }, c: "same" },
    { a: 2, b: { y: 2, x: 1 }, c: "same", d: true },
  ),
  [
    { key: "a", before: 1, after: 2, changed: true },
    { key: "b", before: { x: 1, y: 2 }, after: { y: 2, x: 1 }, changed: false },
    { key: "c", before: "same", after: "same", changed: false },
    { key: "d", before: undefined, after: true, changed: true },
  ],
);
assert.deepEqual(diffChanges(null, { a: 1 }), [
  { key: "a", before: undefined, after: 1, changed: true },
]);
assert.deepEqual(diffChanges("x", "y"), [
  { key: "value", before: "x", after: "y", changed: true },
]);

console.log("audit labels tests passed");
