import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  buildSloTableColumns,
  formatBudgetRemaining,
  hasSlimErrorBudget,
  hasSloBudgetData,
  mergeSloBudgetRows,
  takeSloTopViolators,
} from "../slo";

describe("slo helpers", () => {
  it("mergeSloBudgetRows joins by name", () => {
    const rows = mergeSloBudgetRows(
      [
        {
          id: "1",
          name: "api",
          sli_type: "availability",
          query_kind: "promql",
          query: "x",
          objective: 0.999,
          window_days: 30,
          scope: "global",
          enabled: true,
        },
      ],
      [{ name: "api", sli: 0.9995, error_budget_remaining: 0.8, burn_page: 1 }],
    );
    assert.equal(rows.length, 1);
    assert.equal(rows[0]?.remaining, 0.8);
    assert.equal(rows[0]?.burn_page, 1);
    assert.equal(rows[0]?.sli_type, "availability");
  });

  it("hasSloBudgetData / slim gate", () => {
    assert.equal(hasSloBudgetData([]), false);
    assert.equal(hasSloBudgetData([{ name: "a" } as never]), true);
    assert.equal(hasSlimErrorBudget(true, 0.5), true);
    assert.equal(hasSlimErrorBudget(true, null), false);
    assert.equal(hasSlimErrorBudget(false, 0.5), false);
  });

  it("takeSloTopViolators prefers burn", () => {
    const rows = mergeSloBudgetRows(undefined, [
      { name: "ok", error_budget_remaining: 0.9, burn_page: 0 },
      { name: "hot", error_budget_remaining: 0.2, burn_page: 2 },
      { name: "warm", error_budget_remaining: 0.1, burn_ticket: 1 },
    ]);
    assert.deepEqual(
      takeSloTopViolators(rows, 2).map((r) => r.name),
      ["hot", "warm"],
    );
    assert.deepEqual(takeSloTopViolators(rows, 0), []);
  });

  it("columns hide adminOnly when not detail", () => {
    const t = (k: string) => k;
    const full = buildSloTableColumns(t, { adminDetail: true });
    const slim = buildSloTableColumns(t, { adminDetail: false });
    assert.ok(full.length > slim.length);
    assert.ok(full.some((c) => c.prop === "burn_5m"));
    assert.ok(!slim.some((c) => c.prop === "burn_5m"));
  });

  it("formatBudgetRemaining", () => {
    assert.equal(formatBudgetRemaining(0.8123), "81.23%");
    assert.equal(formatBudgetRemaining(null), "—");
  });
});
