import assert from "node:assert/strict";
import { test } from "node:test";
import { resolveCallChannel } from "../call-channel";

test("有通道名时显示通道名，缺名时回退通道编码", () => {
  assert.deepEqual(resolveCallChannel({ channel_name: "ws imgdef.compare" }), {
    kind: "channel",
    text: "ws imgdef.compare",
  });
  assert.deepEqual(
    resolveCallChannel({ channel_name: "", channel_code: "ENV-CP-CALL" }),
    {
      kind: "channel",
      text: "ENV-CP-CALL",
    },
  );
});

test("缓存命中 / 兜底加 CB 前缀，已有前缀不重复加", () => {
  assert.deepEqual(
    resolveCallChannel({ channel_name: "rest", detail: { cache_hit: true } }),
    { kind: "channel", text: "CB:rest" },
  );
  assert.deepEqual(
    resolveCallChannel({
      channel_name: "CB:rest",
      detail: { fallback_used: true },
    }),
    { kind: "channel", text: "CB:rest" },
  );
  assert.deepEqual(resolveCallChannel({ detail: { cache_hit: true } }), {
    kind: "channel",
    text: "CB",
  });
});

test("网关拒绝且无通道：未路由，带错误码，无产品", () => {
  assert.deepEqual(
    resolveCallChannel({
      channel_name: "",
      channel_code: "",
      error_code: "40113",
      detail: { message: "gateway_rejected", product_id: "" },
    }),
    { kind: "unrouted", productId: "", errorCode: "40113" },
  );
});

test("未路由但请求里带了产品：附带 product_id；error_code 可从 detail 回退", () => {
  assert.deepEqual(
    resolveCallChannel({
      channel_name: "",
      detail: { product_id: "8000000012", error_code: "E4000" },
    }),
    { kind: "unrouted", productId: "8000000012", errorCode: "E4000" },
  );
});

test("空行也按未路由处理，不抛错", () => {
  assert.deepEqual(resolveCallChannel(null), {
    kind: "unrouted",
    productId: "",
    errorCode: "",
  });
});
