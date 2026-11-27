import assert from "node:assert/strict";
import { describe, it } from "node:test";
import type { ServiceHealthRow } from "@/types/api/monitoring";
import {
  SERVICES_WITHOUT_PG,
  filterRowsByGroup,
  formatCpuCoresDisplay,
  serviceDetailStatColSpan,
  serviceHasPostgresPool,
} from "../service-topology";

function makeRow(
  service: string,
  group?: ServiceHealthRow["group"],
): ServiceHealthRow {
  return {
    service,
    name: service,
    group,
    health_ok: true,
    ready: true,
    config_center: true,
    db_pool_usage: null,
    panic_count: 0,
    tokio_busy: 0,
    last_restart_ts: null,
    version: "0.3.5",
    cpu_cores: 0.005,
    memory_bytes: 1048576,
    open_fds: 32,
  };
}

describe("service-topology", () => {
  it("marks only cbpro and analytics as PG-less services", () => {
    assert.deepEqual(
      [...SERVICES_WITHOUT_PG].sort(),
      ["mazbri-analytics", "mazbri-cbpro"],
    );
    assert.equal(serviceHasPostgresPool("mazbri-cbpro"), false);
    assert.equal(serviceHasPostgresPool("mazbri-analytics"), false);
    assert.equal(serviceDetailStatColSpan("mazbri-cbpro"), 8);
    assert.equal(serviceDetailStatColSpan("mazbri-analytics"), 8);

    for (const pgSvc of [
      "mazbri-gateway",
      "mazbri-mgmt",
      "mazbri-cert",
      "mazbri-auth",
      "mazbri-billing",
      "mazbri-ratelimit",
      "mazbri-cache",
      "mazbri-cbpro-ext",
      "mazbri-bypass",
    ]) {
      assert.equal(serviceHasPostgresPool(pgSvc), true, pgSvc);
      assert.equal(serviceDetailStatColSpan(pgSvc), 6, pgSvc);
    }
  });

  it("splits app microservices from middleware and observability components", () => {
    const rows = [
      makeRow("mazbri-gateway", "app"),
      makeRow("mazbri-auth"),
      makeRow("postgres", "middleware"),
      makeRow("vector", "observability"),
    ];
    assert.deepEqual(
      filterRowsByGroup(rows, "apps").map((r) => r.service),
      ["mazbri-gateway", "mazbri-auth"],
    );
    assert.deepEqual(
      filterRowsByGroup(rows, "infra").map((r) => r.service),
      ["postgres", "vector"],
    );
  });

  it("formats fractional CPU cores in mCPU", () => {
    assert.equal(formatCpuCoresDisplay(null, "核"), "—");
    assert.equal(formatCpuCoresDisplay(0, "核"), "0 核");
    assert.equal(formatCpuCoresDisplay(0.0045, "核"), "4.5 mCPU");
    assert.equal(formatCpuCoresDisplay(0.35, "核"), "0.35 核");
  });
});
