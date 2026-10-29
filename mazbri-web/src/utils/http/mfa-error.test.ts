import assert from "node:assert/strict";
import { test } from "node:test";
import {
  classifyMfaError,
  drainDialogQueue,
  insertDialogTask,
  parseJsonBlob,
  shouldInterceptCategory,
} from "./mfa-error";

test("classifyMfaError 只认带合法 category 的 MFA_REQUIRED", () => {
  for (const category of [
    "certificate",
    "admin",
    "mfa",
    "key",
    "finance",
    "platform",
  ]) {
    assert.deepEqual(
      classifyMfaError({
        success: false,
        code: "MFA_REQUIRED",
        category,
        reason: "expired",
      }),
      { kind: "category", category, reason: "expired" },
    );
  }
  assert.equal(classifyMfaError({ code: "MFA_REQUIRED" }), null);
  assert.equal(
    classifyMfaError({ code: "MFA_REQUIRED", category: "certificate_issue" }),
    null,
  );
  assert.equal(
    classifyMfaError({ code: "MFA_REQUIRED", category: ["certificate"] }),
    null,
  );
  assert.equal(classifyMfaError({ code: "MFA_FRESH_REQUIRED", purpose: "api_key_create" }), null);
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
  assert.equal(
    classifyMfaError({
      status: 503,
      code: "MFA_REQUIRED",
      category: "certificate",
    }),
    null,
  );
  assert.equal(
    classifyMfaError({
      status: 503,
      data: { code: "MFA_REQUIRED", category: "admin", reason: "none" },
    }),
    null,
  );
  assert.equal(classifyMfaError({ code: 503 }), null);
  assert.equal(classifyMfaError(null), null);
  assert.equal(classifyMfaError("MFA_REQUIRED"), null);
});

test("重试标记不二次弹窗，step-up 自身不拦截", () => {
  assert.equal(shouldInterceptCategory({ url: "/api/v1/mgmt/tenants" }), true);
  assert.equal(
    shouldInterceptCategory({ url: "/api/v1/mgmt/tenants", _mfaRetry: true }),
    false,
  );
  assert.equal(
    shouldInterceptCategory({
      url: "/api/v1/mgmt/auth/mfa/step-up",
    }),
    false,
  );
  assert.equal(
    shouldInterceptCategory({
      url: "https://mgmt.example/api/v1/mgmt/auth/mfa/step-up",
    }),
    false,
  );
  assert.equal(shouldInterceptCategory(undefined), false);
  assert.equal(shouldInterceptCategory(null), false);
});

test("Blob 且 type 含 json 时解析后再分类 MFA_REQUIRED", async () => {
  const blob = new Blob(
    [JSON.stringify({ code: "MFA_REQUIRED", category: "key", reason: "none" })],
    { type: "application/json" },
  );
  const parsed = await parseJsonBlob(blob);
  assert.deepEqual(classifyMfaError(parsed), {
    kind: "category",
    category: "key",
    reason: "none",
  });

  const problem = new Blob(
    [JSON.stringify({ code: "MFA_FRESH_REQUIRED", purpose: "mfa_disable" })],
    { type: "application/problem+json" },
  );
  assert.equal(classifyMfaError(await parseJsonBlob(problem)), null);

  const text = new Blob(
    [JSON.stringify({ code: "MFA_REQUIRED", category: "finance" })],
    { type: "text/plain" },
  );
  assert.equal(await parseJsonBlob(text), null);
});

test("不同类别的弹窗任务按顺序都执行，成功一个不会丢掉另一个", async () => {
  const pending: Array<{
    category: string;
    resolve: () => void;
    reject: (error: unknown) => void;
  }> = [];
  const resolved: string[] = [];
  const executed: string[] = [];
  insertDialogTask(pending, {
    category: "certificate",
    resolve: () => resolved.push("certificate"),
    reject: () => undefined,
  });
  insertDialogTask(pending, {
    category: "admin",
    resolve: () => resolved.push("admin"),
    reject: () => undefined,
  });
  assert.deepEqual(
    pending.map((item) => item.category),
    ["certificate", "admin"],
  );
  await drainDialogQueue(pending, async (task) => {
    executed.push(task.category);
  });
  assert.deepEqual(executed, ["certificate", "admin"]);
  assert.deepEqual(resolved, ["certificate", "admin"]);
  assert.equal(pending.length, 0);
});
