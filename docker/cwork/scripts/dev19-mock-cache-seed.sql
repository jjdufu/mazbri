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
