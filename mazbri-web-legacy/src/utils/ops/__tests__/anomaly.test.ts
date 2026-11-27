import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  ANOMALY_COLUMN_DEFS,
  anomalyFieldDisplay,
  buildAnomalyDrawerFields,
  buildAnomalyTableColumns,
  formatAnomalyDetail,
  normalizeAnomalyEvents,
  takeAnomalyTop,
} from "../anomaly";

describe("anomaly helpers", () => {
  it("normalizeAnomalyEvents fills defaults", () => {
    const rows = normalizeAnomalyEvents([
      {
        id: "1",
        detected_at: "2026-01-01T00:00:00Z",
        kind: "yoy",
        value: 12,
        detail: { ratio: 3 },
      },
      {},
    ]);
    assert.equal(rows.length, 2);
    assert.equal(rows[0]?.id, "1");
    assert.equal(rows[0]?.value, 12);
    assert.deepEqual(rows[0]?.detail, { ratio: 3 });
    assert.equal(rows[1]?.id, "");
    assert.equal(rows[1]?.kind, "");
    assert.deepEqual(rows[1]?.detail, {});
  });

  it("normalizeAnomalyEvents handles empty", () => {
    assert.deepEqual(normalizeAnomalyEvents(undefined), []);
    assert.deepEqual(normalizeAnomalyEvents(null), []);
    assert.deepEqual(normalizeAnomalyEvents([]), []);
  });

  it("takeAnomalyTop clamps", () => {
    const rows = normalizeAnomalyEvents([
      { id: "a" },
      { id: "b" },
      { id: "c" },
    ]);
    assert.deepEqual(
      takeAnomalyTop(rows, 2).map((r) => r.id),
      ["a", "b"],
    );
    assert.deepEqual(takeAnomalyTop(rows, 0), []);
    assert.equal(takeAnomalyTop(rows, 99).length, 3);
  });

  it("column builders share defs", () => {
    const t = (k: string) => k;
    const table = buildAnomalyTableColumns(t);
    const drawer = buildAnomalyDrawerFields(t);
    assert.equal(
      table.length,
      ANOMALY_COLUMN_DEFS.filter((d) => !d.drawerOnly).length,
    );
    assert.equal(drawer.length, ANOMALY_COLUMN_DEFS.length);
    assert.equal(table[0]?.prop, "detected_at");
    assert.equal(drawer.at(-1)?.prop, "detail");
  });

  it("formatAnomalyDetail and field display", () => {
    assert.equal(formatAnomalyDetail({}), "—");
    assert.equal(formatAnomalyDetail(undefined), "—");
    assert.match(formatAnomalyDetail({ a: 1 }), /"a": 1/);
    const row = normalizeAnomalyEvents([
      { id: "x", value: 1.5, detail: { k: true } },
    ])[0]!;
    assert.equal(anomalyFieldDisplay(row, "value"), "1.5");
    assert.equal(anomalyFieldDisplay(row, "kind"), "—");
    const at = normalizeAnomalyEvents([{ detected_at: "2026-01-01T16:30:00Z" }])[0]!;
    assert.equal(anomalyFieldDisplay(at, "detected_at"), "2026-01-02 00:30:00");
    assert.match(anomalyFieldDisplay(row, "detail"), /"k": true/);
  });
});
