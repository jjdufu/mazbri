# 开发演示种子（seed-mock）

## 原则

- **默认路径零注入**：`redep`、生产 / 正式 `init-dbs`、栈默认启动 **不得**执行本目录 `dev19-mock-*.sql`。
- **显式才注入**：开发机需要 429 / 缓存演示时，手动：

```bash
make -C docker/cwork/scripts seed-mock \
  RATELIMIT_DB_URL='postgres://…/mazbri_ratelimit' \
  CACHE_DB_URL='postgres://…/mazbri_cache'
```

## 告警 seed 约定

- 本目录 SQL **只**改限流 / 缓存策略，**不**写入 `alert_rules` / `alert_events`。
- 若手工插入演示告警，必须带 `labels.seed = 'dev19-mock'`。
- 管理端 `GET /alerts/events` 默认 `include_seed=false`；仅超级管理员可 `include_seed=true` 做诊断。
- 真实 SLO 燃烧规则 **禁止**打 `seed` 标签（analytics upsert 已按此约束）。

## 清理误打 seed 的存量

升级后若库中仍有历史 `seed=dev19-mock`（早期 SLO upsert 误标），在 `mazbri_admin` 执行仓库外一次性脚本：

`/workspace/scripts/purge-alert-seed-labels.sql`

或整库 `redep` 重建。
