-- =============================================================================
-- DEV ONLY · 显式种子（make -C docker/cwork/scripts seed-mock）
-- 禁止：redep / 生产 init-dbs / 默认启动路径自动执行本文件。
-- 作用库：mazbri_cache。不写告警表；告警演示须带 labels.seed=dev19-mock。
-- =============================================================================

-- Run against mazbri_cache
INSERT INTO cache_configs (
  tenant_id, enabled, default_ttl, max_cache_size, api_ttl, tenant_isolated, warmup_enabled
) VALUES (
  '68d9c02a-2ff4-46a8-a76d-694e43235148',
  true, 60, 10000,
  '{"/dev19-mock": 60, "/dev19-mock/*": 60, "/dev19-mock/ping": 60}'::jsonb,
  true, false
)
ON CONFLICT (tenant_id) DO UPDATE SET
  enabled = true,
  default_ttl = EXCLUDED.default_ttl,
  api_ttl = EXCLUDED.api_ttl,
  updated_at = now();
