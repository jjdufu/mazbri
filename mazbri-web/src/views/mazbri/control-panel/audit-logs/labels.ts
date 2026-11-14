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
  "apikey.rotate": "auditLogs.explicitActions.apiKeyRotate",
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
  "alert_rule.create": "auditLogs.explicitActions.alertRuleCreate",
  "alert_rule.update": "auditLogs.explicitActions.alertRuleUpdate",
  "alert_rule.delete": "auditLogs.explicitActions.alertRuleDelete",
  "alert_silence.create": "auditLogs.explicitActions.alertSilenceCreate",
  "alert_silence.delete": "auditLogs.explicitActions.alertSilenceDelete",
  "alert_inhibit.create": "auditLogs.explicitActions.alertInhibitCreate",
  "alert_inhibit.update": "auditLogs.explicitActions.alertInhibitUpdate",
  "alert_inhibit.delete": "auditLogs.explicitActions.alertInhibitDelete",
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
  "config_center.view": "auditLogs.explicitActions.configCenterView",
  "config_center.view_history":
    "auditLogs.explicitActions.configCenterViewHistory",
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
  // /settings/user 显式资料；个人 UI 偏好不再产生此动作
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
  "user.mfa_step_up": "auditLogs.explicitActions.userMfaStepUp",
  "user.mfa_step_up_end": "auditLogs.explicitActions.userMfaStepUpEnd",
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
  "billing.reconciliation_run": "auditLogs.explicitActions.reconciliationRun",
  // cert-service / ratelimit-service 写入的动作
  "certificate.verify": "auditLogs.explicitActions.certificateVerify",
  "authorization.root_material_read":
    "auditLogs.explicitActions.authorizationRootMaterialRead",
  "config_center.tag_create": "auditLogs.explicitActions.configCenterTagCreate",
  "config_center.tag_update": "auditLogs.explicitActions.configCenterTagUpdate",
  "config_center.tag_delete": "auditLogs.explicitActions.configCenterTagDelete",
  "config_center.category_create":
    "auditLogs.explicitActions.configCenterCategoryCreate",
  "config_center.category_update":
    "auditLogs.explicitActions.configCenterCategoryUpdate",
  "config_center.category_delete":
    "auditLogs.explicitActions.configCenterCategoryDelete",
  "ratelimit.command_ack": "auditLogs.explicitActions.ratelimitCommandAck",
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
  ratelimit: "auditLogs.resources.ratelimit",
  alert: "auditLogs.resources.alert",
};

/** 写入服务 → i18n key（与后端 `mazbri_audit::record::SERVICE_*` 一致）。 */
export const SERVICE_LABELS: Record<string, string> = {
  mgmt: "auditLogs.services.mgmt",
  cert: "auditLogs.services.cert",
  ratelimit: "auditLogs.services.ratelimit",
};

/** 审计服务取值（筛选下拉的枚举）。 */
export const AUDIT_SERVICES = Object.keys(SERVICE_LABELS);

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
  token_type_mismatch: "auditLogs.failureReasons.tokenTypeMismatch",
  legacy_token: "auditLogs.failureReasons.legacyToken",
  token_check_failed: "auditLogs.failureReasons.tokenCheckFailed",
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
  mfa_required_by_policy: "auditLogs.failureReasons.mfaRequiredByPolicy",
  password_reset_required: "auditLogs.failureReasons.passwordResetRequired",
  credentials_changed: "auditLogs.failureReasons.credentialsChanged",
  mfa_state_changed: "auditLogs.failureReasons.mfaStateChanged",
  mfa_setup_required: "auditLogs.failureReasons.mfaSetupRequired",
  mfa_code_reused: "auditLogs.failureReasons.mfaCodeReused",
  mfa_check_unavailable: "auditLogs.failureReasons.mfaCheckUnavailable",
  mfa_step_up_invalidated: "auditLogs.failureReasons.mfaStepUpInvalidated",
  mfa_fresh_required: "auditLogs.failureReasons.mfaFreshRequired",
  mfa_backup_restore_failed: "auditLogs.failureReasons.mfaBackupRestoreFailed",
  mfa_invalid_request: "auditLogs.failureReasons.mfaInvalidRequest",
  session_stale: "auditLogs.failureReasons.sessionStale",
  insufficient_permissions: "auditLogs.failureReasons.insufficientPermissions",
  route_not_authorized: "auditLogs.failureReasons.routeNotAuthorized",
  business_failure: "auditLogs.failureReasons.businessFailure",
  password_reset_not_required: "auditLogs.failureReasons.passwordResetNotRequired",
  password_update_indeterminate: "auditLogs.failureReasons.passwordUpdateIndeterminate",
  password_reset_post_commit_failed: "auditLogs.failureReasons.passwordResetPostCommitFailed",
  mfa_verify_post_commit_failed: "auditLogs.failureReasons.mfaVerifyPostCommitFailed",
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

/** 审计扩展字段 → i18n key。 */
export const DETAIL_LABELS: Record<string, string> = {
  reason: "auditLogs.detailLabels.reason",
  operator: "auditLogs.detailLabels.operator",
  operator_id: "auditLogs.detailLabels.operatorId",
  ip_address: "auditLogs.detailLabels.ipAddress",
  selected_channel_id: "auditLogs.detailLabels.selectedChannelId",
  route_rule_id: "auditLogs.detailLabels.routeRuleId",
  channel_id: "auditLogs.detailLabels.channelId",
  binding_id: "auditLogs.detailLabels.bindingId",
  old_route: "auditLogs.detailLabels.oldRoute",
  new_route: "auditLogs.detailLabels.newRoute",
  route_rule: "auditLogs.detailLabels.routeRule",
  bindings: "auditLogs.detailLabels.bindings",
  route: "auditLogs.detailLabels.route",
  message: "auditLogs.detailLabels.message",
  status: "auditLogs.detailLabels.status",
  name: "auditLogs.detailLabels.name",
  id: "auditLogs.detailLabels.id",
  created_at: "auditLogs.detailLabels.createdAt",
  updated_at: "auditLogs.detailLabels.updatedAt",
  expires_at: "auditLogs.detailLabels.expiresAt",
  product_id: "auditLogs.detailLabels.productId",
  tenant_id: "auditLogs.detailLabels.tenantId",
  tenant_code: "auditLogs.detailLabels.tenantCode",
  priority: "auditLogs.detailLabels.priority",
  metadata: "auditLogs.detailLabels.metadata",
  switch_condition: "auditLogs.detailLabels.switchCondition",
  channel_order: "auditLogs.detailLabels.channelOrder",
  description: "auditLogs.detailLabels.description",
  enabled: "auditLogs.detailLabels.enabled",
  request: "auditLogs.detailLabels.request",
  product: "auditLogs.detailLabels.product",
  reservation: "auditLogs.detailLabels.reservation",
  reservation_id: "auditLogs.detailLabels.reservationId",
  active_reservation_id: "auditLogs.detailLabels.activeReservationId",
  reserved_by: "auditLogs.detailLabels.reservedBy",
  reserved_at: "auditLogs.detailLabels.reservedAt",
  upstream_url: "auditLogs.detailLabels.upstreamUrl",
  timeout_secs: "auditLogs.detailLabels.timeoutSecs",
  bill_id: "auditLogs.detailLabels.billId",
  bill_kind: "auditLogs.detailLabels.billKind",
  transaction_id: "auditLogs.detailLabels.transactionId",
  amount: "auditLogs.detailLabels.amount",
  direction: "auditLogs.detailLabels.direction",
  ip_addresses: "auditLogs.detailLabels.ipAddresses",
  total_added: "auditLogs.detailLabels.totalAdded",
  total_errors: "auditLogs.detailLabels.totalErrors",
  algorithm: "auditLogs.detailLabels.algorithm",
  max_daily: "auditLogs.detailLabels.maxDaily",
  max_concurrent: "auditLogs.detailLabels.maxConcurrent",
  max_bandwidth_mbps: "auditLogs.detailLabels.maxBandwidthMbps",
  api_limits: "auditLogs.detailLabels.apiLimits",
  burst_size: "auditLogs.detailLabels.burstSize",
  client_ip_qps: "auditLogs.detailLabels.clientIpQps",
  client_ip_burst: "auditLogs.detailLabels.clientIpBurst",
  window_duration_secs: "auditLogs.detailLabels.windowDurationSecs",
  expected_updated_at: "auditLogs.detailLabels.expectedUpdatedAt",
  default_ttl: "auditLogs.detailLabels.defaultTtl",
  max_cache_size: "auditLogs.detailLabels.maxCacheSize",
  api_ttl: "auditLogs.detailLabels.apiTtl",
  tenant_isolated: "auditLogs.detailLabels.tenantIsolated",
  warmup_enabled: "auditLogs.detailLabels.warmupEnabled",
  warmup_keys: "auditLogs.detailLabels.warmupKeys",
  scope: "auditLogs.detailLabels.scope",
  changes: "auditLogs.detailLabels.changes",
  site_name: "auditLogs.detailLabels.siteName",
  site_url: "auditLogs.detailLabels.siteUrl",
  tenant_gateway_url: "auditLogs.detailLabels.tenantGatewayUrl",
  enable_notifications: "auditLogs.detailLabels.enableNotifications",
  enable_audit_log: "auditLogs.detailLabels.enableAuditLog",
  session_timeout: "auditLogs.detailLabels.sessionTimeout",
  max_upload_size: "auditLogs.detailLabels.maxUploadSize",
  username: "auditLogs.detailLabels.username",
  first_login: "auditLogs.detailLabels.firstLogin",
  language: "auditLogs.detailLabels.language",
  email: "auditLogs.detailLabels.email",
  encryption_mode: "auditLogs.detailLabels.encryptionMode",
  require_signature: "auditLogs.detailLabels.requireSignature",
  nonce_ttl_seconds: "auditLogs.detailLabels.nonceTtlSeconds",
  clock_skew_seconds: "auditLogs.detailLabels.clockSkewSeconds",
  current_key_version: "auditLogs.detailLabels.currentKeyVersion",
  key_version: "auditLogs.detailLabels.keyVersion",
  mode: "auditLogs.detailLabels.mode",
  manifest: "auditLogs.detailLabels.manifest",
  checked: "auditLogs.detailLabels.checked",
  verified: "auditLogs.detailLabels.verified",
  failed_count: "auditLogs.detailLabels.failedCount",
  path: "auditLogs.detailLabels.path",
  operation: "auditLogs.detailLabels.operation",
  authorization_type: "auditLogs.detailLabels.authorizationType",
  activate_now: "auditLogs.detailLabels.activateNow",
  delivery_log_id: "auditLogs.detailLabels.deliveryLogId",
  category: "auditLogs.detailLabels.category",
  mfa: "auditLogs.detailLabels.mfa",
  step_up_opened_at: "auditLogs.detailLabels.stepUpOpenedAt",
  step_up_expires_at: "auditLogs.detailLabels.stepUpExpiresAt",
  step_up_method: "auditLogs.detailLabels.stepUpMethod",
  ttl_secs: "auditLogs.detailLabels.ttlSecs",
  sid: "auditLogs.detailLabels.sid",
  method: "auditLogs.detailLabels.method",
  opened_at: "auditLogs.detailLabels.openedAt",
  remaining_secs: "auditLogs.detailLabels.remainingSecs",
  content_type: "auditLogs.detailLabels.contentType",
  content_length: "auditLogs.detailLabels.contentLength",
  body_keys: "auditLogs.detailLabels.bodyKeys",
  body_type: "auditLogs.detailLabels.bodyType",
  items: "auditLogs.detailLabels.items",
  error_code: "auditLogs.detailLabels.errorCode",
  login_stage: "auditLogs.detailLabels.loginStage",
  stage: "auditLogs.detailLabels.stage",
  required_roles: "auditLogs.detailLabels.requiredRoles",
  refresh_identity_updated: "auditLogs.detailLabels.refreshIdentityUpdated",
  role_changed: "auditLogs.detailLabels.roleChanged",
  username_changed: "auditLogs.detailLabels.usernameChanged",
  additional_events: "auditLogs.detailLabels.additionalEvents",
  service_name: "auditLogs.detailLabels.serviceName",
  version: "auditLogs.detailLabels.version",
  target_version: "auditLogs.detailLabels.targetVersion",
  is_sensitive: "auditLogs.detailLabels.isSensitive",
  subject: "auditLogs.detailLabels.subject",
  cert_type: "auditLogs.detailLabels.certType",
  fingerprint: "auditLogs.detailLabels.fingerprint",
  dns_names: "auditLogs.detailLabels.dnsNames",
  display_name: "auditLogs.detailLabels.displayName",
  role: "auditLogs.detailLabels.role",
  is_active: "auditLogs.detailLabels.isActive",
  mfa_required: "auditLogs.detailLabels.mfaRequired",
  force_password_reset: "auditLogs.detailLabels.forcePasswordReset",
  key_prefix: "auditLogs.detailLabels.keyPrefix",
  value: "auditLogs.detailLabels.value",
  actor_issuer: "auditLogs.detailLabels.actorIssuer",
  actor_mode: "auditLogs.detailLabels.actorMode",
  actor_verification: "auditLogs.detailLabels.actorVerification",
  auth_epoch_bumped: "auditLogs.detailLabels.authEpochBumped",
  caller: "auditLogs.detailLabels.caller",
  caller_generation: "auditLogs.detailLabels.callerGeneration",
  change_summary: "auditLogs.detailLabels.changeSummary",
  contact_email: "auditLogs.detailLabels.contactEmail",
  contact_person: "auditLogs.detailLabels.contactPerson",
  contact_phone: "auditLogs.detailLabels.contactPhone",
  mfa_verified: "auditLogs.detailLabels.mfaVerified",
  peer_ip: "auditLogs.detailLabels.peerIp",
  released: "auditLogs.detailLabels.released",
  route_version: "auditLogs.detailLabels.routeVersion",
  supplier_id: "auditLogs.detailLabels.supplierId",
  token_expiration_secs: "auditLogs.detailLabels.tokenExpirationSecs",
  upstream_type: "auditLogs.detailLabels.upstreamType",
  used_backup_code: "auditLogs.detailLabels.usedBackupCode",
  config_count: "auditLogs.detailLabels.configCount",
  imported: "auditLogs.detailLabels.imported",
  skipped: "auditLogs.detailLabels.skipped",
  errors: "auditLogs.detailLabels.errors",
  rollback_version: "auditLogs.detailLabels.rollbackVersion",
  retry_after_secs: "auditLogs.detailLabels.retryAfterSecs",
  setup_path: "auditLogs.detailLabels.setupPath",
  before_hash: "auditLogs.detailLabels.beforeHash",
  after_hash: "auditLogs.detailLabels.afterHash",
  watermark_visible: "auditLogs.detailLabels.watermarkVisible",
};

/** MFA 提权分类 → i18n key。 */
export const MFA_CATEGORY_LABELS: Record<string, string> = {
  certificate: "auditLogs.mfaCategories.certificate",
  platform: "auditLogs.mfaCategories.platform",
  admin: "auditLogs.mfaCategories.admin",
  key: "auditLogs.mfaCategories.key",
  finance: "auditLogs.mfaCategories.finance",
  security: "auditLogs.mfaCategories.security",
};

/** MFA 二次验证方式 → i18n key。 */
export const MFA_METHOD_LABELS: Record<string, string> = {
  totp: "auditLogs.mfaMethods.totp",
  backup_code: "auditLogs.mfaMethods.backup_code",
};

/** 判断扩展属性键名是否为时间戳字段（如 `step_up_opened_at`、`expires_at`）。 */
export function isAuditTimestampKey(key?: string): boolean {
  if (!key) return false;
  const k = key.toLowerCase();
  return (
    k.endsWith("_at") ||
    k.endsWith("_time") ||
    k === "timestamp" ||
    k === "until" ||
    k.endsWith("_until")
  );
}

/** 判断扩展属性键名是否为秒级时长字段（如 `ttl_secs`、`remaining_secs`）。 */
export function isAuditDurationSecondsKey(key?: string): boolean {
  if (!key) return false;
  const k = key.toLowerCase();
  return k.endsWith("_secs") || k.endsWith("_seconds") || k === "default_ttl" || k === "api_ttl";
}

export type GroupedAuditRow<T> = T & {
  chainServices: string[];
  mirrorChildren: T[];
  chainExpanded: boolean;
  isMirrorChild: boolean;
};

/** 格式化代理链路徽标编码（如 `mgmt ➔ cert`）。 */
export function formatChainFlowCode(services: readonly string[]): string {
  return services.join(" ➔ ");
}

/**
 * 将同页具有相同 `request_id` 的跨服务代理链路记录（如 `mgmt` + `cert`）聚合折叠：
 * - 优先以入口层 `mgmt` 记录为主行，底层服务记录收折为 `mirrorChildren`；
 * - 当 `expandedRequestIds` 包含该 `request_id` 时，在主行下方紧邻展开底层服务镜像子行。
 */
export function groupAuditLogsByRequestId<
  T extends { id: string; request_id?: string | null; service: string },
>(logs: readonly T[], expandedRequestIds: ReadonlySet<string>): GroupedAuditRow<T>[] {
  const byRequestId = new Map<string, T[]>();
  for (const log of logs) {
    const reqId = log.request_id?.trim();
    if (!reqId) continue;
    const bucket = byRequestId.get(reqId);
    if (bucket) {
      bucket.push(log);
    } else {
      byRequestId.set(reqId, [log]);
    }
  }

  const emittedRequestIds = new Set<string>();
  const result: GroupedAuditRow<T>[] = [];

  for (const log of logs) {
    const reqId = log.request_id?.trim();
    if (!reqId) {
      result.push({
        ...log,
        chainServices: [],
        mirrorChildren: [],
        chainExpanded: false,
        isMirrorChild: false,
      });
      continue;
    }

    const group = byRequestId.get(reqId) ?? [log];
    const uniqueServices = new Set(group.map((item) => item.service));
    if (group.length < 2 || uniqueServices.size < 2) {
      result.push({
        ...log,
        chainServices: [],
        mirrorChildren: [],
        chainExpanded: false,
        isMirrorChild: false,
      });
      continue;
    }

    if (emittedRequestIds.has(reqId)) {
      continue;
    }
    emittedRequestIds.add(reqId);

    const primaryIdx = group.findIndex((item) => item.service === "mgmt");
    const primary = primaryIdx >= 0 ? group[primaryIdx] : group[0];
    const mirrorChildren = group.filter((item) => item.id !== primary.id);
    const chainServices: string[] = [primary.service];
    for (const child of mirrorChildren) {
      if (!chainServices.includes(child.service)) {
        chainServices.push(child.service);
      }
    }
    const expanded = expandedRequestIds.has(reqId);
    result.push({
      ...primary,
      chainServices,
      mirrorChildren,
      chainExpanded: expanded,
      isMirrorChild: false,
    });
    if (expanded) {
      for (const child of mirrorChildren) {
        result.push({
          ...child,
          chainServices: [],
          mirrorChildren: [],
          chainExpanded: false,
          isMirrorChild: true,
        });
      }
    }
  }

  return result;
}
