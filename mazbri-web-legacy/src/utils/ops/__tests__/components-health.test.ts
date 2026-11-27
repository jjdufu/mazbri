import assert from "node:assert/strict";
import { describe, it } from "node:test";
import type { ComponentHealth } from "@/types/api/monitoring";
import {
  countAbnormal,
  formatShortVersion,
  groupComponents,
} from "../components-health";

function c(key: string, group: ComponentHealth["group"], status: ComponentHealth["status"], name = key): ComponentHealth {
  return {
    key,
    name,
    group,
    status,
    reasons: [],
    container_running: true,
    restarts_1h: 0,
    cpu_cores: null,
    memory_bytes: null,
    version: null,
  };
}

describe("components-health", () => {
  it("groups in fixed order, abnormal first, dedupes by key", () => {
    const groups = groupComponents([
      c("redis", "middleware", "green", "Redis 缓存"),
      c("mazbri-auth", "app", "green", "认证服务"),
      c("mazbri-gateway", "app", "red", "网关"),
      c("mazbri-gateway", "app", "red", "网关"),
      c("postgresql", "middleware", "unknown", "PostgreSQL 数据库"),
    ]);
    assert.deepEqual(groups.map((g) => g.group), ["app", "middleware"]);
    assert.deepEqual(groups[0]?.items.map((i) => i.key), ["mazbri-gateway", "mazbri-auth"]);
    assert.equal(groups[0]?.abnormal, 1);
    assert.deepEqual(groups[1]?.items.map((i) => i.key), ["postgresql", "redis"]);
  });

  it("countAbnormal", () => {
    assert.equal(countAbnormal(undefined), 0);
    assert.equal(
      countAbnormal([c("a", "app", "yellow"), c("b", "other", "green")]),
      1,
    );
  });

  it("formatShortVersion strips git commit hash", () => {
    assert.equal(formatShortVersion("0.3.5 (98ac85f38ca0)"), "v0.3.5");
    assert.equal(formatShortVersion("v0.3.7"), "v0.3.7");
    assert.equal(formatShortVersion(null), undefined);
    assert.equal(formatShortVersion(""), undefined);
  });
});
