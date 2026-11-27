import assert from "node:assert/strict";
import { test } from "node:test";
import {
  isGenericUnauthorizedMessage,
  shouldAttemptSessionRefreshOn401,
} from "./auth-route-policy";

const STAGE_PATHS = [
  "/api/v1/mgmt/auth/login",
  "/api/v1/mgmt/auth/first-login-change-password",
  "/api/v1/mgmt/auth/mfa/verify",
  "/api/v1/mgmt/auth/mfa/required-setup",
  "/api/v1/mgmt/auth/mfa/required-enable",
  "/api/v1/mgmt/auth/logout",
  "/api/v1/mgmt/auth/refresh",
];

test("登录阶段和会话边界 401 不刷新 access", () => {
  assert.equal(shouldAttemptSessionRefreshOn401(""), false);
  assert.equal(shouldAttemptSessionRefreshOn401("   "), false);
  assert.equal(shouldAttemptSessionRefreshOn401(undefined), false);
  for (const path of STAGE_PATHS) {
    assert.equal(shouldAttemptSessionRefreshOn401(path), false, path);
    assert.equal(
      shouldAttemptSessionRefreshOn401(`https://mgmt.example${path}?x=1`),
      false,
      path,
    );
    assert.equal(shouldAttemptSessionRefreshOn401(`${path}/`), false, path);
    assert.equal(shouldAttemptSessionRefreshOn401(`${path}?retry=1`), false, path);
  }
});

test("受保护会话接口 401 仍刷新 access", () => {
  for (const path of [
    "/api/v1/mgmt/auth/me",
    "/api/v1/mgmt/auth/change-password",
    "/api/v1/mgmt/auth/mfa/setup",
    "/api/v1/mgmt/auth/mfa/disable",
    "/api/v1/mgmt/auth/mfa/step-up",
  ]) {
    assert.equal(shouldAttemptSessionRefreshOn401(path), true, path);
  }
});

test("只把空文案和 Unauthorized 当成泛化未认证", () => {
  assert.equal(isGenericUnauthorizedMessage(""), true);
  assert.equal(isGenericUnauthorizedMessage(undefined), true);
  assert.equal(isGenericUnauthorizedMessage("Unauthorized"), true);
  assert.equal(isGenericUnauthorizedMessage("  Unauthorized  "), true);
  assert.equal(isGenericUnauthorizedMessage("令牌已注销或已使用"), false);
  assert.equal(isGenericUnauthorizedMessage("用户名或密码无效"), false);
});
