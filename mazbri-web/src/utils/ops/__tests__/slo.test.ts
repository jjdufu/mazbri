import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  buildSloTableColumns,
  formatBudgetRemaining,
  formatBurnRate,
  formatSloRatio,
  type SloBudgetRow,
} from "../slo";

const stubT = (key: string) => key;

describe("SLO formatting and table columns", () => {
  it("formats SLO ratios and burn rates cleanly", () => {
    assert.equal(formatSloRatio(0.99), "99.00%");
    assert.equal(formatSloRatio(0.999), "99.90%");
    assert.equal(formatSloRatio(1), "100.00%");
    assert.equal(formatSloRatio(null), "—");

    assert.equal(formatBurnRate(0), "0x");
    assert.equal(formatBurnRate(1.25), "1.25x");
    assert.equal(formatBurnRate(null), "—");

    assert.equal(formatBudgetRemaining(0.8843), "88.43%");
  });

  it("mounts formatters on buildSloTableColumns and renders burn_page as boolean state", () => {
    const cols = buildSloTableColumns(stubT, { adminDetail: true });
    const row: SloBudgetRow = {
      name: "gateway-availability",
      sli_type: "availability",
      objective: 0.999,
      window_days: 30,
      sli: 0.9995,
      remaining: 0.5,
      burn_5m: 0,
      burn_page: 1,
      burn_ticket: 0,
      ts: "2026-11-26 08:00:00",
    };

    const objCol = cols.find((c) => c.prop === "objective");
    assert.equal(objCol?.formatter?.(row), "99.90%");

    const sliCol = cols.find((c) => c.prop === "sli");
    assert.equal(sliCol?.formatter?.(row), "99.95%");

    const burn5mCol = cols.find((c) => c.prop === "burn_5m");
    assert.equal(burn5mCol?.formatter?.(row), "0x");

    const burnPageCol = cols.find((c) => c.prop === "burn_page");
    assert.ok(Number(burnPageCol?.minWidth ?? 0) >= 120);
    const firingVnode = burnPageCol?.formatter?.(row) as {
      children?: string;
    };
    assert.equal(firingVnode?.children, "ops.slo.burnFiring");

    const burnTicketCol = cols.find((c) => c.prop === "burn_ticket");
    const normalVnode = burnTicketCol?.formatter?.(row) as {
      children?: string;
    };
    assert.equal(normalVnode?.children, "ops.slo.burnNormal");
  });
});
