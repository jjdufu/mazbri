import type { ISODateString } from "./common";
export interface AuditLog {
  id: string;
  timestamp: ISODateString;
  actor_id?: string | null;
  actor_type: "admin" | "system" | "anonymous";
  actor_name?: string;
  actor_role?: string | null;
  action: string;
  resource_type: string;
  resource_id?: string;
  details: Record<string, unknown>;
  ip_address?: string;
  user_agent?: string;
  status: AuditStatus;
  error_message?: string;
  failure_reason?: string | null;
  request_id?: string | null;
  method?: string | null;
  /** 路由模板（不含查询串） */
  path?: string | null;
  status_code?: number | null;
  duration_ms?: number | null;
  /** 请求体摘要（仅 content_type / 长度 / 顶层字段名，不含值） */
  request_summary?: Record<string, unknown> | null;
  /** 写入服务；同一请求的 mgmt 与 cert 记录通过 request_id 关联 */
  service: AuditService;
}
export type AuditService = "mgmt" | "cert" | "ratelimit";
export type AuditStatus = "success" | "failure" | "denied";
export interface AuditLogListResponse {
  logs: AuditLog[];
  total: number;
  page: number;
  limit: number;
}
export interface AuditLogListQuery {
  page?: number;
  limit?: number;
  tenant_id?: string;
  action?: string;
  resource_type?: string;
  actor_id?: string;
  q?: string;

  filters?: string;
  start_time?: number;
  end_time?: number;
  status?: AuditStatus;
  service?: AuditService;
}
export interface AuditLogDetail extends AuditLog {
  changes?: {
    before?: unknown;
    after?: unknown;
  } | null;
  related_logs?: AuditLog[];
}
export type AuditAction =
  | "admin.login"
  | "admin.logout"
  | "admin.create"
  | "admin.update"
  | "admin.delete"
  | "admin.password_reset"
  | "admin.status_change"
  | "tenant.create"
  | "tenant.update"
  | "tenant.delete"
  | "tenant.suspend"
  | "tenant.activate"
  | "apikey.create"
  | "apikey.update"
  | "apikey.revoke"
  | "user.login"
  | "user.logout"
  | "user.update"
  | "user.delete"
  | "quota.update"
  | "plan.change"
  | "product.grant"
  | "product.revoke"
  | "product.create"
  | "product.update"
  | "product.delete"
  | "product.reservation_create"
  | "product.reservation_release"
  | "account.recharge"
  | "account.refund"
  | "bill.generate"
  | "bill.delete"
  | "bill.status_update"
  | "account.adjustment"
  | "billing.reconciliation_audit"
  | "billing.reconciliation_discrepancy_update"
  | "billing.reconciliation_discrepancy_delete"
  | "billing.bill_adjustment_create"
  | "billing.bill_adjustment_void"
  | "billing.reconciliation_settings_update"
  | "certificate.issue"
  | "certificate.renew"
  | "certificate.revoke"
  | "settings.update"
  | "password_policy.update"
  | "config_center.create"
  | "config_center.update"
  | "config_center.delete"
  | "config_center.rollback"
  | "config_center.export"
  | "config_center.import"
  | "appearance.update"
  | "appearance.reset"
  | "mfa_settings.update"
  | "tenant.whitelist_add"
  | "tenant.whitelist_remove"
  | "tenant.crypto_profile_update"
  | "tenant.crypto_key_create"
  | "tenant.crypto_key_rotate"
  | "tenant.crypto_key_activate"
  | "tenant.crypto_key_deprecate"
  | "tenant.crypto_key_disable"
  | "tenant.crypto_export"
  | "tenant.crypto_verify"
  | "password.first_login_change"
  | "password.change"
  | "user.settings_update"
  | "tenant.ratelimit_update"
  | "tenant.ratelimit_delete"
  | "tenant.cache_update"
  | "callback.delivery_retry"
  | "route.emergency_switch"
  | "supplier.create"
  | "supplier.update"
  | "supplier.delete"
  | "channel.create"
  | "channel.update"
  | "channel.delete"
  | "route_rule.create"
  | "route_rule.update"
  | "route_rule.delete"
  | "route_channel_binding.create"
  | "route_channel_binding.update"
  | "route_channel_binding.delete"
  | "callback.subscription_create"
  | "callback.subscription_update"
  | "callback.subscription_delete"
  | "authorization.import"
  | "authorization.export"
  | "authorization.renew"
  | "authorization.activate"
  | "authorization.revoke"
  | "authorization.rotate"
  | "authorization.disable"
  | "tenant.ratelimit_command_create"
  | "tenant.ratelimit_command_cancel"
  | "tenant.crypto_key_delete"
  | "upstream.onboarding"
  | "supplier.validate_config"
  | "connector.run"
  | "connector.publish"
  | "connector.force_publish"
  | "connector.rollback"
  | "connector.import"
  | "admin.force_password_reset"
  | "admin.mfa_reset"
  | "user.mfa_setup"
  | "user.mfa_enable"
  | "user.mfa_disable"
  | "user.mfa_regenerate_codes"
  | "user.mfa_required_setup"
  | "user.mfa_required_enable"
  | "user.token_refresh"
  | "tenant.request_key_create"
  | "tenant.request_key_retire"
  | "tenant.request_key_list"
  | "tenant.crypto_key_list"
  | "tenant.enabled_change"
  | "apikey.list"
  | "certificate.view_full"
  | "certificate.auto_renewal_update"
  | "authorization.license_request"
  | "audit.list"
  | "audit.view"
  | "audit.export"
  | "connector.draft_create"
  | "connector.draft_update"
  | "connector.draft_delete"
  | "connector.draft_validate"
  | "connector.transform_preview"
  | "connector.draft_from_supplier"
  | "connector.artifact_create"
  | "connector.export"
  | "config_center.validate"
  | "upstream.onboarding_dry_run"
  | "product.tenant_config_update"
  | "bill.generate_async"
  | "billing.reconciliation_run"
  | "certificate.verify"
  | "authorization.root_material_read"
  | "config_center.tag_create"
  | "config_center.tag_update"
  | "config_center.tag_delete"
  | "config_center.category_create"
  | "config_center.category_update"
  | "config_center.category_delete"
  | "ratelimit.command_ack"
  | "api.read"
  | "api.get"
  | "api.post"
  | "api.put"
  | "api.patch"
  | "api.delete";
export type ResourceType =
  | "admin"
  | "tenant"
  | "product"
  | "apikey"
  | "certificate"
  | "system"
  | "user"
  | "quota"
  | "plan"
  | "account"
  | "config_center"
  | "supplier"
  | "channel"
  | "route_rule"
  | "route_channel_binding"
  | "callback"
  | "audit_log"
  | "connector"
  | "authorization"
  | "billing"
  | "ratelimit";
