import { test } from "node:test";
import assert from "node:assert/strict";

import { callMonitorCatalogFor } from "../../../list-catalog/call-monitor";
import { timestampFiltersToUnix } from "../../ops/call-filters";
import { looksLikeStrongClause, parseClause } from "../parse-clause";
import { formatFilterChipText } from "../serialize";

const catalog = callMonitorCatalogFor("customer");
const labels: Record<string, string> = {
  "operations.monitoring.columns.responseTime": "时间戳",
};
const t = (key: string) => labels[key] ?? key;

test("field picked from suggestions then a Unix value parses as eq in Beijing time", () => {
  const text = "时间戳 1793504280";
  assert.equal(looksLikeStrongClause(text, catalog, t), true);
  const r = parseClause(text, catalog, t);
  assert.deepEqual(r, {
    ok: true,
    filter: { field: "timestamp", op: "eq", value: "2026-11-01 11:38:00" },
  });
});

test("explicit eq on a Unix value yields the same Beijing wall clock", () => {
  const r = parseClause("时间戳:1793504280", catalog, t);
  assert.ok(r.ok);
  assert.equal(r.filter.value, "2026-11-01 11:38:00");
});

test("eq chip label shows Beijing time like the range chips", () => {
  const r = parseClause("时间戳 1793504280", catalog, t);
  assert.ok(r.ok);
  assert.equal(
    formatFilterChipText({ fieldLabel: "时间戳", opLabel: "=", value: r.filter.value }),
    "时间戳 = 2026-11-01 11:38:00",
  );
});

test("eq value goes back to the original Unix second at the request boundary", () => {
  const r = parseClause("时间戳 1793504280", catalog, t);
  assert.ok(r.ok);
  assert.deepEqual(timestampFiltersToUnix([r.filter]), [
    { field: "timestamp", op: "eq", value: 1793504280 },
  ]);
});

test("a non-datetime word after the field is still not a clause", () => {
  assert.equal(looksLikeStrongClause("时间戳 hello", catalog, t), false);
});
