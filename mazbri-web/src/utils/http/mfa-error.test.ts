import assert from "node:assert/strict";
import { test } from "node:test";
import {
  afterFreshResolved,
  classifyMfaError,
  parseJsonBlob,
  shouldInterceptFresh,
  shouldInterceptStepUp,
} from "./mfa-error";

test("classifyMfaError 区分 step_up、fresh、not_enabled", () => {
  assert.deepEqual(
    classifyMfaError({
      success: false,
      code: "MFA_REQUIRED",
      reason: "expired",
    }),
    { kind: "step_up", reason: "expired" },
  );
  assert.deepEqual(classifyMfaError({ code: "MFA_REQUIRED" }), {
    kind: "step_up",
    reason: null,
  });
  assert.deepEqual(
    classifyMfaError({
      code: "MFA_FRESH_REQUIRED",
      purpose: "api_key_create",
    }),
    { kind: "fresh", purpose: "api_key_create" },
  );
  assert.deepEqual(
    classifyMfaError({
      code: "MFA_NOT_ENABLED",
      setup_path: "/account/security",
    }),
    { kind: "not_enabled", setupPath: "/account/security" },
  );
});

test("MFA_LOCKED、MFA_CHECK_UNAVAILABLE 与 HTTP 503 不弹验证码框", () => {
  assert.equal(
    classifyMfaError({ code: "MFA_LOCKED", retry_after_secs: 900 }),
    null,
  );
  assert.equal(classifyMfaError({ code: "MFA_CHECK_UNAVAILABLE" }), null);
  assert.equal(classifyMfaError({ status: 503, code: "MFA_REQUIRED" }), null);
  assert.equal(
    classifyMfaError({
      status: 503,
      data: { code: "MFA_REQUIRED", reason: "none" },
    }),
    null,
  );
  assert.equal(classifyMfaError({ code: 503 }), null);
  assert.equal(classifyMfaError(null), null);
  assert.equal(classifyMfaError("MFA_REQUIRED"), null);
});

test("重试标记不二次弹窗，step-up 自身不拦截", () => {
  assert.equal(shouldInterceptStepUp({ url: "/api/v1/mgmt/tenants" }), true);
  assert.equal(
    shouldInterceptStepUp({ url: "/api/v1/mgmt/tenants", _mfaRetry: true }),
    false,
  );
  assert.equal(
    shouldInterceptFresh({
      url: "/api/v1/mgmt/tenants",
      _mfaFreshRetry: true,
    }),
    false,
  );
  assert.equal(
    shouldInterceptStepUp({
      url: "/api/v1/mgmt/auth/mfa/step-up",
    }),
    false,
  );
  assert.equal(
    shouldInterceptFresh({
      url: "https://mgmt.example/api/v1/mgmt/auth/mfa/step-up",
    }),
    false,
  );
  assert.equal(shouldInterceptStepUp(undefined), false);
  assert.equal(shouldInterceptFresh(null), false);
  assert.equal(
    shouldInterceptFresh({ url: "/auth/mfa/disable", _mfaRetry: true }),
    true,
  );
  assert.equal(
    shouldInterceptStepUp({ url: "/auth/mfa/disable", _mfaFreshRetry: true }),
    true,
  );
});

test("Blob 且 type 含 json 时解析后再分类 MFA_REQUIRED", async () => {
  const blob = new Blob(
    [JSON.stringify({ code: "MFA_REQUIRED", reason: "none" })],
    { type: "application/json" },
  );
  const parsed = await parseJsonBlob(blob);
  assert.deepEqual(classifyMfaError(parsed), {
    kind: "step_up",
    reason: "none",
  });

  const problem = new Blob(
    [JSON.stringify({ code: "MFA_FRESH_REQUIRED", purpose: "mfa_disable" })],
    { type: "application/problem+json" },
  );
  assert.deepEqual(classifyMfaError(await parseJsonBlob(problem)), {
    kind: "fresh",
    purpose: "mfa_disable",
  });

  const text = new Blob([JSON.stringify({ code: "MFA_REQUIRED" })], {
    type: "text/plain",
  });
  assert.equal(await parseJsonBlob(text), null);
});

test("ensureFresh 成功后，排队的 ensureStepUp 不再弹窗", () => {
  const pending = [
    { kind: "step_up", id: 1 },
    { kind: "fresh", purpose: "api_key_create", id: 2 },
    { kind: "step_up", id: 3 },
  ];
  const remaining = afterFreshResolved(pending);
  assert.deepEqual(
    remaining.map((item) => item.id),
    [2],
  );
  assert.equal(
    remaining.some((item) => item.kind === "step_up"),
    false,
  );
  assert.equal(afterFreshResolved(pending).length, 1);
});
