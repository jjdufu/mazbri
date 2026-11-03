-- =============================================================================
-- DEV ONLY · 显式种子（make -C docker/cwork/scripts seed-mock）
-- 禁止：redep / 生产 init-dbs / 默认启动路径自动执行本文件。
-- 作用库：mazbri_ratelimit。本脚本不写 alert_rules/alert_events；
-- 若另插告警演示数据，必须 labels.seed=dev19-mock，列表 API 默认过滤。
-- =============================================================================

-- Run against mazbri_ratelimit
-- 租户 CN5XGJRW / product 8019900019
-- 产品级 api_limits 压到 qps=2/burst=2，便于打出 429。
-- 应用后等网关策略同步（约 5s），再对 /api/v1/gateway 施压。

INSERT INTO rate_limit_tenant_policies (
  tenant_id, enabled, enforcement_mode, stale_policy, max_snapshot_age_secs,
  tenant_qps, tenant_burst, client_ip_qps, client_ip_burst, bandwidth_bps,
  api_limits, description, updated_by
) VALUES (
  '68d9c02a-2ff4-46a8-a76d-694e43235148',
  true, 'local_enforced', 'stale_allow', 300,
  1000, 1000, 1000, 1000, 0,
  '{"product:8019900019": {"qps": 2, "burst": 2}}'::jsonb,
  'dev19-mock low qps for 429 seed',
  'ops-seed'
)
ON CONFLICT (tenant_id) DO UPDATE SET
  enabled = EXCLUDED.enabled,
  enforcement_mode = EXCLUDED.enforcement_mode,
  api_limits = EXCLUDED.api_limits,
  description = EXCLUDED.description,
  updated_by = EXCLUDED.updated_by,
  updated_at = now();
