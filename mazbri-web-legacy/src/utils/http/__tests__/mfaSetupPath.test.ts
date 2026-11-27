import assert from "node:assert/strict";
import { test } from "node:test";
import {
  MFA_SETUP_PATH,
  getMfaNotEnabledSetupPath,
  resolveMfaSetupPath,
} from "../mfa-setup-path";

test("MFA_NOT_ENABLED 403 返回响应中的 setup_path", () => {
  const error = {
    code: 403,
    data: {
      success: false,
      code: "MFA_NOT_ENABLED",
      error: "此操作需要 MFA 二次验证",
      setup_path: "/?setupMfa=1",
    },
  };
  assert.equal(getMfaNotEnabledSetupPath(error), "/?setupMfa=1");
});

test("MFA_NOT_ENABLED 缺少 setup_path 时返回空字符串", () => {
  assert.equal(
    getMfaNotEnabledSetupPath({ code: 403, data: { code: "MFA_NOT_ENABLED" } }),
    "",
  );
});

test("其他错误不识别为 MFA_NOT_ENABLED", () => {
  assert.equal(
    getMfaNotEnabledSetupPath({ code: 403, data: { code: "MFA_REQUIRED" } }),
    null,
  );
  assert.equal(
    getMfaNotEnabledSetupPath({ code: 400, data: { code: "MFA_NOT_ENABLED" } }),
    null,
  );
  assert.equal(getMfaNotEnabledSetupPath({ code: 403, data: null }), null);
  assert.equal(getMfaNotEnabledSetupPath(new Error("boom")), null);
  assert.equal(getMfaNotEnabledSetupPath(undefined), null);
});

test("setup_path 只接受站内相对路径", () => {
  assert.equal(resolveMfaSetupPath("/?setupMfa=1"), "/?setupMfa=1");
  assert.equal(resolveMfaSetupPath(""), MFA_SETUP_PATH);
  assert.equal(resolveMfaSetupPath(undefined), MFA_SETUP_PATH);
  assert.equal(
    resolveMfaSetupPath("https://evil.example"),
    MFA_SETUP_PATH,
  );
  assert.equal(resolveMfaSetupPath("//evil.example"), MFA_SETUP_PATH);
  assert.equal(resolveMfaSetupPath("/\\evil.example"), MFA_SETUP_PATH);
  assert.equal(
    resolveMfaSetupPath("javascript:alert(1)"),
    MFA_SETUP_PATH,
  );
});
