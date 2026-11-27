import assert from "node:assert/strict";
import { test } from "node:test";
import {
  createEmptyInvalidation,
  extractRetryInfo,
  invalidationFields,
  invalidationFromApi,
  invalidationSource,
  invalidationToApi,
  parseEqualsText,
  validateInvalidation,
} from "../authInvalidation";
import { sanitizeAuthSpec } from "../authMatrix";

test("round-trips api json", () => {
  const api = {
    http_status: [401, 419],
    jsonrpc_codes: [-32001],
    body_match: [{ path: "$.code", equals: ["TOKEN_EXPIRED", 40101] }],
    ws_close_before_reply: true,
  };
  assert.deepEqual(invalidationToApi(invalidationFromApi(api)), api);
});

test("disabled rules are omitted instead of {}", () => {
  assert.equal(invalidationToApi(createEmptyInvalidation()), undefined);
  assert.equal(invalidationToApi(undefined), undefined);
  assert.equal(invalidationFromApi(undefined).enabled, false);
});

test("explicit empty status list is preserved", () => {
  const form = invalidationFromApi({ http_status: [] });
  assert.deepEqual(invalidationToApi(form), { http_status: [] });
});

test("equals text parses json literals and falls back to strings", () => {
  assert.deepEqual(parseEqualsText('"A", 1, true, plain'), [
    "A",
    1,
    true,
    "plain",
  ]);
  assert.deepEqual(parseEqualsText(" , "), []);
});

test("validation mirrors backend rules", () => {
  const form = { ...createEmptyInvalidation(), enabled: true };
  assert.equal(validateInvalidation(form), null);
  assert.deepEqual(validateInvalidation({ ...form, http_status: [200] }), {
    kind: "status_range",
    value: 200,
  });
  assert.deepEqual(validateInvalidation({ ...form, http_status: [401, 401] }), {
    kind: "status_duplicate",
    value: 401,
  });
  assert.deepEqual(validateInvalidation({ ...form, jsonrpc_codes: [-1, -1] }), {
    kind: "jsonrpc_duplicate",
    value: -1,
  });
  assert.deepEqual(
    validateInvalidation({ ...form, body_match: [{ path: "$", equals: "1" }] }),
    { kind: "body_path", index: 0 },
  );
  assert.deepEqual(
    validateInvalidation({
      ...form,
      body_match: [{ path: "$.code", equals: "" }],
    }),
    { kind: "body_equals", index: 0 },
  );
  assert.equal(
    validateInvalidation({ ...form, http_status: [200], enabled: false }),
    null,
  );
});

test("adapter-specific fields", () => {
  assert.deepEqual(invalidationFields("jsonrpc"), {
    jsonrpc: true,
    wsClose: false,
  });
  assert.deepEqual(invalidationFields("http_json"), {
    jsonrpc: false,
    wsClose: false,
  });
  assert.deepEqual(invalidationFields("ws_unary"), {
    jsonrpc: false,
    wsClose: true,
  });
  assert.deepEqual(invalidationFields(""), { jsonrpc: true, wsClose: true });
});

test("channel rules override supplier rules", () => {
  const supplier = {
    enabled: true,
    invalidation: { ...createEmptyInvalidation(), enabled: true },
  };
  assert.equal(
    invalidationSource({ invalidation: { http_status: [419] } }, supplier),
    "channel",
  );
  assert.equal(
    invalidationSource({ profile: "oauth_cc_form" }, supplier),
    "supplier",
  );
  assert.equal(
    invalidationSource(null, { ...supplier, enabled: false }),
    "default",
  );
  assert.equal(invalidationSource(null, null), "default");
});

test("retry info is summed from stage summaries", () => {
  assert.equal(
    extractRetryInfo([{ response_summary: { internal_retries: 0 } }]),
    null,
  );
  assert.equal(extractRetryInfo(undefined), null);
  assert.deepEqual(
    extractRetryInfo([
      { response_summary: { status: 200 } },
      {
        response_summary: {
          internal_retries: 1,
          retry_reasons: ["auth_rejected:jsonrpc_code"],
        },
      },
    ]),
    { count: 1, reasons: ["auth_rejected:jsonrpc_code"] },
  );
});

test("sanitizeAuthSpec keeps channel invalidation rules", () => {
  const rules = { http_status: [401], jsonrpc_codes: [-32001] };
  assert.deepEqual(
    sanitizeAuthSpec({ profile: "gettoken", invalidation: rules })
      ?.invalidation,
    rules,
  );
  assert.equal(
    sanitizeAuthSpec({ profile: "none", invalidation: rules }),
    null,
  );
});
