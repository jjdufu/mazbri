import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { ref } from "vue";
import { useAdaptiveTableColumns } from "../useAdaptiveTableColumns";
import type { ColumnOption } from "../../types/component";

interface SupplierRow {
  supplier_code: string;
  status: string;
}

describe("useAdaptiveTableColumns contract", () => {
  it("respects explicit minWidth >= 200 on *_code columns instead of clamping to 110px", () => {
    const baseColumns: ColumnOption<SupplierRow>[] = [
      { prop: "supplier_code", label: "Code", minWidth: 200 },
      { prop: "status", label: "Status", width: 110 },
    ];
    const columns = ref<ColumnOption<SupplierRow>[]>([]);
    const rows = ref<SupplierRow[]>([
      { supplier_code: "UPUPT-ENV-SC-SM4", status: "active" },
    ]);
    const { syncAdaptiveColumns } = useAdaptiveTableColumns<SupplierRow>({
      columns,
      containerRef: ref(null),
      rows,
      baseColumns,
      softMinCap: 0,
      getCellText: ({ columnKey, row }) =>
        String(row[columnKey as keyof SupplierRow] ?? ""),
    });
    syncAdaptiveColumns();

    const codeCol = columns.value.find((c) => c.prop === "supplier_code");
    const widthNum = Number(codeCol?.minWidth ?? codeCol?.width ?? 0);
    assert.ok(
      widthNum >= 200,
      `expected supplier_code width >= 200, got ${widthNum}`,
    );
  });
});
