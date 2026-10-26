/**
 * 审计日志标签与纯函数工具（无 Vue / i18n 依赖，便于单测）。
 *
 * 动作列表须与后端 `ACTION_LABELS`（mazbri-mgmt/src/audit/service.rs）保持一致，
 * 后端测试 `every_action_has_frontend_label` 会校验本文件。
 */

/** 动作 → i18n key。 */
export const ACTION_LABELS: Record<string, string> = {
  "admin.login": "auditLogs.explicitActions.adminLogin",
  "admin.logout": "auditLogs.explicitActions.adminLogout",
  "admin.create": "auditLogs.explicitActions.adminCreate",
  "admin.update": "auditLogs.explicitActions.adminUpdate",
  "admin.delete": "auditLogs.explicitActions.adminDelete",
  "admin.password_reset": "auditLogs.explicitActions.adminPasswordReset",
  "admin.status_change": "auditLogs.explicitActions.adminStatusChange",
  "tenant.create": "auditLogs.explicitActions.tenantCreate",
  "tenant.update": "auditLogs.explicitActions.tenantUpdate",
  "tenant.delete": "auditLogs.explicitActions.tenantDelete",
  "tenant.suspend": "auditLogs.explicitActions.tenantSuspend",
  "tenant.activate": "auditLogs.explicitActions.tenantActivate",
  "apikey.create": "auditLogs.explicitActions.apiKeyCreate",
  "apikey.update": "auditLogs.explicitActions.apiKeyUpdate",
  "apikey.revoke": "auditLogs.explicitActions.apiKeyRevoke",
  "user.login": "auditLogs.explicitActions.userLogin",
  "user.logout": "auditLogs.explicitActions.userLogout",
  "user.update": "auditLogs.explicitActions.userUpdate",
  "user.delete": "auditLogs.explicitActions.userDelete",
  "quota.update": "auditLogs.explicitActions.quotaUpdate",
  "plan.change": "auditLogs.explicitActions.planChange",
  "product.grant": "auditLogs.explicitActions.productGrant",
  "product.revoke": "auditLogs.explicitActions.productRevoke",
  "product.create": "auditLogs.explicitActions.productCreate",
  "product.update": "auditLogs.explicitActions.productUpdate",
  "product.delete": "auditLogs.explicitActions.productDelete",
  "product.reservation_create":
    "auditLogs.explicitActions.productReservationCreate",
  "product.reservation_release":
    "auditLogs.explicitActions.productReservationRelease",
  "account.recharge": "auditLogs.explicitActions.accountRecharge",
  "account.refund": "auditLogs.explicitActions.accountRefund",
  "bill.generate": "auditLogs.explicitActions.billGenerate",
  "bill.delete": "auditLogs.explicitActions.billDelete",
  "bill.status_update": "auditLogs.explicitActions.billStatusUpdate",
  "account.adjustment": "auditLogs.explicitActions.accountAdjustment",
  "billing.reconciliation_audit":
    "auditLogs.explicitActions.reconciliationAudit",
  "billing.reconciliation_discrepancy_update":
    "auditLogs.explicitActions.reconciliationDiscrepancyUpdate",
  "billing.reconciliation_discrepancy_delete":
    "auditLogs.explicitActions.billingReconciliationDiscrepancyDelete",
  "billing.bill_adjustment_create":
    "auditLogs.explicitActions.billAdjustmentCreate",
  "billing.bill_adjustment_void":
    "auditLogs.explicitActions.billAdjustmentVoid",
  "billing.reconciliation_settings_update":
    "auditLogs.explicitActions.reconciliationSettingsUpdate",
  "certificate.issue": "auditLogs.explicitActions.certificateIssue",
  "certificate.renew": "auditLogs.explicitActions.certificateRenew",
  "certificate.revoke": "auditLogs.explicitActions.certificateRevoke",
  "settings.update": "auditLogs.explicitActions.settingsUpdate",
  "password_policy.update": "auditLogs.explicitActions.passwordPolicyUpdate",
  "config_center.create": "auditLogs.explicitActions.configCenterCreate",
  "config_center.update": "auditLogs.explicitActions.configCenterUpdate",
  "config_center.delete": "auditLogs.explicitActions.configCenterDelete",
  "config_center.rollback": "auditLogs.explicitActions.configCenterRollback",
  "config_center.export": "auditLogs.explicitActions.configCenterExport",
  "config_center.import": "auditLogs.explicitActions.configCenterImport",
  "appearance.update": "auditLogs.explicitActions.appearanceUpdate",
  "appearance.reset": "auditLogs.explicitActions.appearanceReset",
  "mfa_settings.update": "auditLogs.explicitActions.mfaSettingsUpdate",
  "tenant.whitelist_add": "auditLogs.explicitActions.whitelistAdd",
  "tenant.whitelist_remove": "auditLogs.explicitActions.whitelistRemove",
  "tenant.crypto_profile_update":
    "auditLogs.explicitActions.tenantCryptoProfileUpdate",
  "tenant.crypto_key_create": "auditLogs.explicitActions.tenantCryptoKeyCreate",
  "tenant.crypto_key_rotate": "auditLogs.explicitActions.tenantCryptoKeyRotate",
  "tenant.crypto_key_activate":
    "auditLogs.explicitActions.tenantCryptoKeyActivate",
  "tenant.crypto_key_deprecate":
    "auditLogs.explicitActions.tenantCryptoKeyDeprecate",
  "tenant.crypto_key_disable":
    "auditLogs.explicitActions.tenantCryptoKeyDisable",
  "tenant.crypto_export": "auditLogs.explicitActions.tenantCryptoExport",
  "tenant.crypto_verify": "auditLogs.explicitActions.tenantCryptoVerify",
  "password.first_login_change":
    "auditLogs.explicitActions.passwordFirstLoginChange",
  "password.change": "auditLogs.explicitActions.passwordChange",
  "user.settings_update": "auditLogs.explicitActions.userSettingsUpdate",
  "tenant.ratelimit_update": "auditLogs.explicitActions.tenantRateLimitUpdate",
  "tenant.ratelimit_delete": "auditLogs.explicitActions.tenantRateLimitDelete",
  "tenant.cache_update": "auditLogs.explicitActions.tenantCacheUpdate",
  "callback.delivery_retry": "auditLogs.explicitActions.callbackDeliveryRetry",
  "route.emergency_switch": "auditLogs.explicitActions.routeEmergencySwitch",
  "supplier.create": "auditLogs.explicitActions.supplierCreate",
  "supplier.update": "auditLogs.explicitActions.supplierUpdate",
  "supplier.delete": "auditLogs.explicitActions.supplierDelete",
  "channel.create": "auditLogs.explicitActions.channelCreate",
  "channel.update": "auditLogs.explicitActions.channelUpdate",
  "channel.delete": "auditLogs.explicitActions.channelDelete",
  "route_rule.create": "auditLogs.explicitActions.routeRuleCreate",
  "route_rule.update": "auditLogs.explicitActions.routeRuleUpdate",
  "route_rule.delete": "auditLogs.explicitActions.routeRuleDelete",
  "route_channel_binding.create":
    "auditLogs.explicitActions.routeChannelBindingCreate",
  "route_channel_binding.update":
    "auditLogs.explicitActions.routeChannelBindingUpdate",
  "route_channel_binding.delete":
    "auditLogs.explicitActions.routeChannelBindingDelete",
  "callback.subscription_create":
    "auditLogs.explicitActions.callbackSubscriptionCreate",
  "callback.subscription_update":
    "auditLogs.explicitActions.callbackSubscriptionUpdate",
  "callback.subscription_delete":
    "auditLogs.explicitActions.callbackSubscriptionDelete",
  "authorization.import": "auditLogs.explicitActions.authorizationImport",
  "authorization.export": "auditLogs.explicitActions.authorizationExport",
  "authorization.renew": "auditLogs.explicitActions.authorizationRenew",
  "authorization.activate": "auditLogs.explicitActions.authorizationActivate",
  "authorization.revoke": "auditLogs.explicitActions.authorizationRevoke",
  "authorization.rotate": "auditLogs.explicitActions.authorizationRotate",
  "authorization.disable": "auditLogs.explicitActions.authorizationDisable",
  "tenant.ratelimit_command_create":
    "auditLogs.explicitActions.tenantRatelimitCommandCreate",
  "tenant.ratelimit_command_cancel":
    "auditLogs.explicitActions.tenantRatelimitCommandCancel",
  "tenant.crypto_key_delete": "auditLogs.explicitActions.tenantCryptoKeyDelete",
  "upstream.onboarding": "auditLogs.explicitActions.upstreamOnboarding",
  "supplier.validate_config":
    "auditLogs.explicitActions.supplierValidateConfig",
  "connector.run": "auditLogs.explicitActions.connectorRun",
  "connector.publish": "auditLogs.explicitActions.connectorPublish",
  "connector.force_publish": "auditLogs.explicitActions.connectorForcePublish",
  "connector.rollback": "auditLogs.explicitActions.connectorRollback",
  "connector.import": "auditLogs.explicitActions.connectorImport",
  "admin.force_password_reset":
    "auditLogs.explicitActions.adminForcePasswordReset",
  "admin.mfa_reset": "auditLogs.explicitActions.adminMfaReset",
  "user.mfa_setup": "auditLogs.explicitActions.userMfaSetup",
  "user.mfa_enable": "auditLogs.explicitActions.userMfaEnable",
  "user.mfa_disable": "auditLogs.explicitActions.userMfaDisable",
  "user.mfa_regenerate_codes":
    "auditLogs.explicitActions.userMfaRegenerateCodes",
  "user.mfa_required_setup": "auditLogs.explicitActions.userMfaRequiredSetup",
  "user.mfa_required_enable": "auditLogs.explicitActions.userMfaRequiredEnable",
  "user.token_refresh": "auditLogs.explicitActions.userTokenRefresh",
  "tenant.request_key_create":
    "auditLogs.explicitActions.tenantRequestKeyCreate",
  "tenant.request_key_retire":
    "auditLogs.explicitActions.tenantRequestKeyRetire",
  "tenant.request_key_list": "auditLogs.explicitActions.tenantRequestKeyList",
  "tenant.crypto_key_list": "auditLogs.explicitActions.tenantCryptoKeyList",
  "tenant.enabled_change": "auditLogs.explicitActions.tenantEnabledChange",
  "apikey.list": "auditLogs.explicitActions.apikeyList",
  "certificate.view_full": "auditLogs.explicitActions.certificateViewFull",
  "certificate.auto_renewal_update":
    "auditLogs.explicitActions.certificateAutoRenewalUpdate",
  "authorization.license_request":
    "auditLogs.explicitActions.authorizationLicenseRequest",
  "audit.list": "auditLogs.explicitActions.auditList",
  "audit.view": "auditLogs.explicitActions.auditView",
  "audit.export": "auditLogs.explicitActions.auditExport",
  "connector.draft_create": "auditLogs.explicitActions.connectorDraftCreate",
  "connector.draft_update": "auditLogs.explicitActions.connectorDraftUpdate",
  "connector.draft_delete": "auditLogs.explicitActions.connectorDraftDelete",
  "connector.draft_validate":
    "auditLogs.explicitActions.connectorDraftValidate",
  "connector.transform_preview":
    "auditLogs.explicitActions.connectorTransformPreview",
  "connector.draft_from_supplier":
    "auditLogs.explicitActions.connectorDraftFromSupplier",
  "connector.artifact_create":
    "auditLogs.explicitActions.connectorArtifactCreate",
  "connector.export": "auditLogs.explicitActions.connectorExport",
  "config_center.validate": "auditLogs.explicitActions.configCenterValidate",
  "upstream.onboarding_dry_run":
    "auditLogs.explicitActions.upstreamOnboardingDryRun",
  "product.tenant_config_update":
    "auditLogs.explicitActions.productTenantConfigUpdate",
  "bill.generate_async": "auditLogs.explicitActions.billGenerateAsync",
  "api.read": "auditLogs.explicitActions.apiRead",
  "api.get": "auditLogs.explicitActions.apiGet",
  "api.post": "auditLogs.explicitActions.apiPost",
  "api.put": "auditLogs.explicitActions.apiPut",
  "api.patch": "auditLogs.explicitActions.apiPatch",
  "api.delete": "auditLogs.explicitActions.apiDelete",
};

/** 资源类型 → i18n key。 */
export const RESOURCE_TYPE_LABELS: Record<string, string> = {
  admin: "auditLogs.resources.admin",
  tenant: "auditLogs.resources.tenant",
  product: "auditLogs.resources.product",
  apikey: "auditLogs.resources.apiKey",
  certificate: "auditLogs.resources.certificate",
  system: "auditLogs.resources.system",
  user: "auditLogs.resources.user",
  quota: "auditLogs.resources.quota",
  plan: "auditLogs.resources.plan",
  account: "auditLogs.resources.account",
  config_center: "auditLogs.resources.configCenter",
  supplier: "auditLogs.resources.supplier",
  channel: "auditLogs.resources.channel",
  route_rule: "auditLogs.resources.routeRule",
  route_channel_binding: "auditLogs.resources.routeChannelBinding",
  callback: "auditLogs.resources.callback",
  audit_log: "auditLogs.resources.auditLog",
  connector: "auditLogs.resources.connector",
  authorization: "auditLogs.resources.authorization",
  billing: "auditLogs.resources.billing",
};

export type AuditStatusValue = "success" | "failure" | "denied";

/** 状态 → i18n key（未知状态按失败展示）。 */
export function statusLabelKey(status: string): string {
  if (status === "success") return "auditLogs.status.success";
  if (status === "denied") return "auditLogs.status.denied";
  return "auditLogs.status.failure";
}

/** 状态 → 标签颜色。 */
export function statusTagType(
  status: string,
): "success" | "warning" | "danger" {
  if (status === "success") return "success";
  if (status === "denied") return "warning";
  return "danger";
}

/** 已知失败原因 → i18n key；未知原因直接展示代码。 */
export const FAILURE_REASON_LABELS: Record<string, string> = {
  missing_token: "auditLogs.failureReasons.missingToken",
  invalid_token: "auditLogs.failureReasons.invalidToken",
  invalid_token_type: "auditLogs.failureReasons.invalidTokenType",
  token_expired: "auditLogs.failureReasons.tokenExpired",
  token_revoked: "auditLogs.failureReasons.tokenRevoked",
  forbidden: "auditLogs.failureReasons.forbidden",
  unauthorized: "auditLogs.failureReasons.unauthorized",
  user_not_found: "auditLogs.failureReasons.userNotFound",
  invalid_password: "auditLogs.failureReasons.invalidPassword",
  account_disabled: "auditLogs.failureReasons.accountDisabled",
  account_locked: "auditLogs.failureReasons.accountLocked",
  lock_triggered: "auditLogs.failureReasons.lockTriggered",
  password_expired: "auditLogs.failureReasons.passwordExpired",
  mfa_invalid_code: "auditLogs.failureReasons.mfaInvalidCode",
  mfa_locked: "auditLogs.failureReasons.mfaLocked",
  mfa_required: "auditLogs.failureReasons.mfaRequired",
  rate_limited: "auditLogs.failureReasons.rateLimited",
  client_disconnected: "auditLogs.failureReasons.clientDisconnected",
  server_error: "auditLogs.failureReasons.serverError",
  client_error: "auditLogs.failureReasons.clientError",
  db_error: "auditLogs.failureReasons.dbError",
  panic: "auditLogs.failureReasons.panic",
  mfa_check_error: "auditLogs.failureReasons.mfaCheckError",
  mfa_not_enabled: "auditLogs.failureReasons.mfaNotEnabled",
  mfa_lock_triggered: "auditLogs.failureReasons.mfaLockTriggered",
  invalid_temp_token: "auditLogs.failureReasons.invalidTempToken",
  password_hash_missing: "auditLogs.failureReasons.passwordHashMissing",
  business_failure: "auditLogs.failureReasons.businessFailure",
};

/** CSV 公式注入防护：以 = + - @ 制表符 回车 开头的单元格前置单引号。 */
export function guardCsvFormula(value: string): string {
  return /^[=+\-@\t\r]/.test(value) ? `'${value}` : value;
}

export interface ChangeDiffRow {
  key: string;
  before: unknown;
  after: unknown;
  changed: boolean;
}

function asRecord(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  return value as Record<string, unknown>;
}

function stableStringify(value: unknown): string {
  if (value === undefined) return "undefined";
  const record = asRecord(value);
  if (record) {
    const keys = Object.keys(record).sort();
    return `{${keys.map((k) => `${JSON.stringify(k)}:${stableStringify(record[k])}`).join(",")}}`;
  }
  if (Array.isArray(value)) {
    return `[${value.map(stableStringify).join(",")}]`;
  }
  return JSON.stringify(value);
}

/**
 * 变更前后对比：按顶层字段合并，标记是否变化。
 * 非对象值整体作为 `value` 一行比较。
 */
export function diffChanges(before: unknown, after: unknown): ChangeDiffRow[] {
  const beforeRecord = asRecord(before);
  const afterRecord = asRecord(after);
  if (!beforeRecord && !afterRecord) {
    if (before === undefined && after === undefined) return [];
    return [
      {
        key: "value",
        before,
        after,
        changed: stableStringify(before) !== stableStringify(after),
      },
    ];
  }
  const keys = Array.from(
    new Set([
      ...Object.keys(beforeRecord ?? {}),
      ...Object.keys(afterRecord ?? {}),
    ]),
  ).sort();
  return keys.map((key) => {
    const b = beforeRecord?.[key];
    const a = afterRecord?.[key];
    return {
      key,
      before: b,
      after: a,
      changed: stableStringify(b) !== stableStringify(a),
    };
  });
}
