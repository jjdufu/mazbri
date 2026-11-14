import assert from "node:assert/strict";
import { describe, test } from "node:test";
import { buildAdaptiveColumns } from "../useAdaptiveTableColumns";
import type { ColumnOption } from "@/types/component";

interface SampleRow {
  category: string;
  operator: string;
  created_at: string;
}

describe("buildAdaptiveColumns datetime & substring rules", () => {
  test("does not cap non-datetime columns containing 'at' to 150px", () => {
    const baseColumns: ColumnOption<SampleRow>[] = [
      { prop: "category", label: "分类", minWidth: 120 },
      { prop: "operator", label: "操作人", minWidth: 120 },
      { prop: "created_at", label: "创建时间", minWidth: 176, maxWidth: 188 },
    ];
    const rows: SampleRow[] = [
      {
        category: "enterprise-platform-governance-category-value",
        operator: "platform-security-administrator@mazbri.internal",
        created_at: "2026-11-13 10:37:09",
      },
    ];
    const result = buildAdaptiveColumns(
      baseColumns,
      rows,
      undefined,
      50,
      28,
      1280,
      48,
      88,
      undefined,
      72,
      168,
      260,
      true,
    );
    const categoryCol = result.find((c) => c.prop === "category");
    const operatorCol = result.find((c) => c.prop === "operator");
    const createdCol = result.find((c) => c.prop === "created_at");

    assert.ok(categoryCol && Number(categoryCol.width) > 150);
    assert.ok(operatorCol && Number(operatorCol.width) > 150);
    assert.ok(
      createdCol &&
        Number(createdCol.width) >= 172 &&
        Number(createdCol.width) <= 188,
    );
  });

  test("protects datetime columns at >= 172px under narrow container shrink", () => {
    const baseColumns: ColumnOption<SampleRow>[] = [
      { prop: "category", label: "分类", minWidth: 160 },
      { prop: "operator", label: "操作人", minWidth: 160 },
      { prop: "created_at", label: "创建时间", minWidth: 176, maxWidth: 188 },
    ];
    const rows: SampleRow[] = [
      {
        category: "enterprise-category-long-text-sample",
        operator: "operator-long-name-sample",
        created_at: "2026-11-13 10:37:09",
      },
    ];
    const result = buildAdaptiveColumns(
      baseColumns,
      rows,
      undefined,
      50,
      28,
      380,
      48,
      88,
      undefined,
      72,
      168,
      260,
      true,
    );
    const createdCol = result.find((c) => c.prop === "created_at");
    assert.ok(createdCol && Number(createdCol.width) >= 172);
  });
});
