import type { ISODateString } from "./common";
export interface AuditLog {
  id: string;
  timestamp: ISODateString;
  actor_id: string;
  actor_type: "admin" | "system";
  actor_name?: string;
  action: string;
  resource_type: string;
  resource_id?: string;
  details: Record<string, unknown>;
  ip_address?: string;
  user_agent?: string;
  status: "success" | "failure";
  error_message?: string;
}
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
  status?: "success" | "failure";
}
export interface AuditLogDetail extends AuditLog {
  changes?: {
    before?: Record<string, unknown>;
    after?: Record<string, unknown>;
  };
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
  | "user.settings_update"
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
  | "account.adjustment"
  | "billing.reconciliation_audit"
  | "billing.reconciliation_discrepancy_update"
  | "billing.bill_adjustment_create"
  | "billing.bill_adjustment_void"
  | "billing.reconciliation_settings_update"
  | "bill.generate"
  | "bill.delete"
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
  | "tenant.ratelimit_update"
  | "tenant.ratelimit_delete"
  | "tenant.cache_update"
  | "callback.delivery_retry"
  | "supplier.create"
  | "supplier.update"
  | "supplier.delete"
  | "channel.create"
  | "channel.update"
  | "channel.delete"
  | "route_rule.create"
  | "route_rule.update"
  | "route_rule.delete"
  | "route.emergency_switch"
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
  | "authorization.disable";
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
  | "callback";
