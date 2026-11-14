import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  ACTION_LABELS,
  DETAIL_LABELS,
  FAILURE_REASON_LABELS,
  MFA_CATEGORY_LABELS,
  MFA_METHOD_LABELS,
  RESOURCE_TYPE_LABELS,
  SERVICE_LABELS,
  AUDIT_SERVICES,
  diffChanges,
  formatChainFlowCode,
  groupAuditLogsByRequestId,
  guardCsvFormula,
  isAuditDurationSecondsKey,
  isAuditTimestampKey,
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
  ...Object.values(DETAIL_LABELS),
  ...Object.values(MFA_CATEGORY_LABELS),
  ...Object.values(MFA_METHOD_LABELS),
  "auditLogs.columns.service",
  "auditLogs.filters.includeSelfAudit",
  "auditLogs.chain.expandTooltip",
  "auditLogs.chain.collapseTooltip",
  "auditLogs.chain.mirrorPrefix",
  "auditLogs.chain.detailTitle",
  "auditLogs.chain.switchToService",
  "auditLogs.chain.currentRecord",
  "auditLogs.detail.rawJsonTitle",
  "auditLogs.detail.copyRawJson",
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

assert.equal(isAuditTimestampKey("step_up_opened_at"), true);
assert.equal(isAuditTimestampKey("expires_at"), true);
assert.equal(isAuditTimestampKey("locked_until"), true);
assert.equal(isAuditTimestampKey("category"), false);
assert.equal(isAuditDurationSecondsKey("ttl_secs"), true);
assert.equal(isAuditDurationSecondsKey("nonce_ttl_seconds"), true);
assert.equal(isAuditDurationSecondsKey("category"), false);

const rawPageLogs = [
  { id: "cert-1", request_id: "req-pair", service: "cert" },
  { id: "mgmt-1", request_id: "req-pair", service: "mgmt" },
  { id: "solo-1", request_id: "req-solo", service: "mgmt" },
];
const collapsed = groupAuditLogsByRequestId(rawPageLogs, new Set());
assert.equal(collapsed.length, 2);
assert.equal(collapsed[0].id, "mgmt-1");
assert.deepEqual(collapsed[0].chainServices, ["mgmt", "cert"]);
assert.equal(formatChainFlowCode(collapsed[0].chainServices), "mgmt ➔ cert");
assert.equal(collapsed[0].mirrorChildren.length, 1);
assert.equal(collapsed[0].mirrorChildren[0].id, "cert-1");
assert.equal(collapsed[1].id, "solo-1");
assert.deepEqual(collapsed[1].chainServices, []);

const expanded = groupAuditLogsByRequestId(rawPageLogs, new Set(["req-pair"]));
assert.equal(expanded.length, 3);
assert.equal(expanded[0].id, "mgmt-1");
assert.equal(expanded[0].chainExpanded, true);
assert.equal(expanded[1].id, "cert-1");
assert.equal(expanded[1].isMirrorChild, true);

console.log("audit labels tests passed");
