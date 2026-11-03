import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  CHANGE_COLUMN_DEFS,
  buildChangeAnnotations,
  buildChangeDrawerFields,
  buildChangeTableColumns,
  changeAnnotationColor,
  changeFieldDisplay,
  formatChangeDetail,
  last24hWindow,
  normalizeChangeEvents,
  takeChangeTop,
} from "../changes";

describe("changes helpers", () => {
  it("normalizeChangeEvents fills defaults", () => {
    const rows = normalizeChangeEvents([
      {
        ts: "2026-01-01T00:00:00Z",
        kind: "route",
        title: "reload",
        detail: { path: "/x" },
      },
      {},
    ]);
    assert.equal(rows.length, 2);
    assert.equal(rows[0]?.kind, "route");
    assert.deepEqual(rows[0]?.detail, { path: "/x" });
    assert.equal(rows[1]?.ts, "");
    assert.equal(rows[1]?.title, "");
    assert.equal(rows[1]?.actor, null);
  });

  it("normalizeChangeEvents handles empty", () => {
    assert.deepEqual(normalizeChangeEvents(undefined), []);
    assert.deepEqual(normalizeChangeEvents(null), []);
    assert.deepEqual(normalizeChangeEvents([]), []);
  });

  it("takeChangeTop clamps", () => {
    const rows = normalizeChangeEvents([
      { ts: "a", kind: "1", title: "1" },
      { ts: "b", kind: "2", title: "2" },
      { ts: "c", kind: "3", title: "3" },
    ]);
    assert.deepEqual(
      takeChangeTop(rows, 2).map((r) => r.ts),
      ["a", "b"],
    );
    assert.deepEqual(takeChangeTop(rows, 0), []);
    assert.equal(takeChangeTop(rows, 99).length, 3);
  });

  it("last24hWindow is 86400 seconds", () => {
    const { start, end } = last24hWindow(1_700_000_000_000);
    assert.equal(end, 1_700_000_000);
    assert.equal(end - start, 86400);
  });

  it("column builders share defs", () => {
    const t = (k: string) => k;
    const table = buildChangeTableColumns(t);
    const drawer = buildChangeDrawerFields(t);
    assert.equal(
      table.length,
      CHANGE_COLUMN_DEFS.filter((d) => !d.drawerOnly).length,
    );
    assert.equal(drawer.length, CHANGE_COLUMN_DEFS.length);
    assert.equal(table[0]?.prop, "ts");
    assert.equal(drawer.at(-1)?.prop, "detail");
  });

  it("annotation helpers", () => {
    assert.equal(changeAnnotationColor("route"), "#409eff");
    assert.equal(changeAnnotationColor("publish"), "#67c23a");
    assert.equal(changeAnnotationColor("other"), "#e6a23c");
    const ann = buildChangeAnnotations(
      normalizeChangeEvents([
        { ts: "2026-01-01T00:00:00.000Z", kind: "route", title: "a" },
        { ts: "not-a-date", kind: "x", title: "b" },
      ]),
    );
    assert.equal(ann.length, 1);
    assert.equal(ann[0]?.label, "route");
    assert.equal(ann[0]?.color, "#409eff");
    assert.equal(ann[0]?.timestamp, Date.parse("2026-01-01T00:00:00.000Z") / 1000);
  });

  it("formatChangeDetail and field display", () => {
    assert.equal(formatChangeDetail({}), "—");
    assert.equal(formatChangeDetail(undefined), "—");
    assert.match(formatChangeDetail({ a: 1 }), /"a": 1/);
    const row = normalizeChangeEvents([
      { ts: "t", kind: "route", title: "hello", detail: { k: true } },
    ])[0]!;
    assert.equal(changeFieldDisplay(row, "title"), "hello");
    assert.equal(changeFieldDisplay(row, "actor"), "—");
    assert.match(changeFieldDisplay(row, "detail"), /"k": true/);
  });
});
