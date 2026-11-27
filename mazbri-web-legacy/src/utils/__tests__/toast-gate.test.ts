import assert from "node:assert/strict";
import { test } from "node:test";
import { createToastGate } from "../ui/toast-gate";

function clock(start = 0) {
  let t = start;
  return {
    now: () => t,
    advance: (ms: number) => {
      t += ms;
    },
  };
}

test("相同文案在去重窗口内只放行一条，窗口过后可再次提示", () => {
  const c = clock();
  const gate = createToastGate({ now: c.now, dedupeWindowMs: 3000 });
  assert.equal(gate.admit("加载失败"), true);
  assert.equal(gate.admit("加载失败"), false);
  assert.equal(gate.admit("保存失败"), true);
  c.advance(2999);
  assert.equal(gate.admit("加载失败"), false);
  c.advance(1);
  assert.equal(gate.admit("加载失败"), true);
});

test("并发网络错误在冷却期内全局只放行一条，并压住连带的其他错误", () => {
  const c = clock();
  const gate = createToastGate({ now: c.now, networkCooldownMs: 5000 });
  const admitted = Array.from({ length: 15 }, () =>
    gate.admit("网络连接异常", "network"),
  ).filter(Boolean);
  assert.equal(admitted.length, 1);
  assert.equal(gate.admit("加载失败"), false);
  assert.equal(gate.admit("链路加载失败"), false);
  c.advance(5000);
  assert.equal(gate.admit("网络连接异常", "network"), true);
  assert.equal(gate.admit("网络连接异常", "network"), false);
});

test("离线期间不放行任何错误，恢复在线后立即可再次提示网络错误", () => {
  const c = clock();
  const gate = createToastGate({ now: c.now, networkCooldownMs: 5000 });
  assert.equal(gate.admit("网络连接异常", "network"), true);
  gate.setOffline(true);
  assert.equal(gate.isOffline(), true);
  c.advance(60_000);
  assert.equal(gate.admit("网络连接异常", "network"), false);
  assert.equal(gate.admit("加载失败"), false);
  gate.setOffline(false);
  assert.equal(gate.admit("网络连接异常", "network"), true);
});

test("在线恢复会清空网络冷却", () => {
  const c = clock();
  const gate = createToastGate({ now: c.now, networkCooldownMs: 5000 });
  assert.equal(gate.admit("网络连接异常", "network"), true);
  c.advance(100);
  gate.setOffline(false);
  assert.equal(gate.admit("加载失败"), true);
});
