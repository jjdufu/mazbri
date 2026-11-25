# Step 07：控制面细粒度 RBAC 裁剪、有状态会话纪元与跨表跨库事务一致性

## 1. 检测出的问题清单与源码定位

| 编号 | 级别 | 问题描述 | 精确源码位置 |
| :--- | :--- | :--- | :--- |
| **S07-1** | High | **多路径 RBAC 角色越权读取与前端角色门控漏口**：`route_access` 允许 `R_VIEWER` 访问 `/tenants/{code}/spend`、`/billing/accounts`、`/billing/bills*`、`/channels*`（含采购底价 `cost_price`）及 OLAP `tenant_overview`/`product_rank`（未剥离 `cost` 列）/`billing_funnel_ch`；`R_VIEWER`/`R_FINANCE` 可通过 `/stats/metrics/timeseries`、`/stats/ops/slos/status`、`/billing/reconciliation/settings`、`/upstream-connectors/drafts*` 读取证书/License 指标、内部 PromQL、对账 Webhook Token 与工作台调试凭据；同时前端 `/operations/bills`、`ops/overview` 的 `<SecurityGovSummary />` 与 `overview360.vue` 存在对 `R_VIEWER` 未门控的受限请求 | [route_access.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/middleware/route_access.rs#L103-L168)、[analytics.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/analytics.rs#L1140-L1205)、[mazbri.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/router/modules/mazbri.ts#L130-L139)、[index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/overview/index.vue#L242-L252)、[overview360.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/tenants/[code]/tabs/overview360.vue#L1-L95) |
| **S07-2** | High | **管理端长效 Access Token 无状态逃逸**：`resolve_access_token_expiration_secs` 优先读取数据库 `system_settings.session_timeout`（默认 2 小时，最高 24 小时）覆盖 `jwt_expiration_secs`，且 `admin_auth_middleware` 不核验 `auth_epoch`/`is_active`/实时 `role`；管理员被禁用、删除、改密或从 `R_FINANCE` 降权为 `R_VIEWER` 后，已签发令牌在免 MFA 写接口上持续有效；MFA 二阶段签发正式令牌前未复核账户最新状态 | [auth_flow.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/admin/auth_flow.rs#L17-L33)、[auth.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/middleware/auth.rs#L140-L206)、[verification_flow.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/mfa/verification_flow.rs#L302-L379) |
| **S07-3** | Medium | **MFA 提权新鲜度守卫失效与回滚越权**：`handlers/admin_management/crud.rs` 将布尔值 `lifts` 本身传给 `fresh_for_lift` 判断，未核验提权窗口签发新鲜度，使解除强制 MFA 的新鲜度要求失效；连接器版本 `rollback` 漏调 `guard_supplier_tls_privilege`，`R_FINANCE` 可通过回滚开启 `insecure_skip_verify = true` | [step_up.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/mfa/step_up.rs#L235-L250)、[crud.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/handlers/admin_management/crud.rs#L79-L99)、[versions.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/upstream_connector/versions.rs#L84-L163) |
| **S07-4** | Medium | **登录防爆破非原子计数与唯一超管远程锁死 DoS**：`auth_flow` 调用非原子的 `update_login_failure`（而非原子 SQL `record_login_failure`），导致并发爆破漏计且锁过期后单次错密即重锁；无源 IP 限流使攻击者可周期性发送错误密码永久锁死不可被管理接口解锁的唯一超管 `R_ADMIN`；改密接口错密不计数 | [auth_flow.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/admin/auth_flow.rs#L120-L174)、[admin.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/repositories/admin.rs#L193-L267)、[passwords.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/admin/passwords.rs#L386-L394) |
| **S07-5** | Medium | **控制面事务缺失、配置中心并发覆盖与选择性导出失效、审计外键连坐**：`tenant_crypto::set_key_status` 无事务先废弃旧密钥再查目标版本（传错版本号致租户断流）；`delete_tenant` 跨库非原子硬删且残留限流与缓存孤儿策略；连接器 `rollback` 无事务；配置中心修改无 CAS 乐观锁、导出忽略 `service_names`、导入 ZIP `read_to_string` 信任伪造头大小；`mazbri-audit` 批量外键冲突清空整批 100 条 `user_id` 且停机超时丢弃未刷盘记录 | [tenant_crypto.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/tenant_crypto.rs#L578-L612)、[tenant.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/tenant.rs#L259-L291)、[import_export.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/config_center/import_export.rs#L20-L58)、[import_export.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/config_center/import_export.rs#L195-L230)、[sink.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-audit/src/sink.rs#L235-L344) |

---

## 2. 第一性原理本质归因

本步对应 **公理 A3（最小权限视图投影）** 与 **公理 A4（主体权限实时性）**：
1. **资源级投影必须前后端同构**：同一领域聚合（如通道、产品排行、租户 360 概览、指标目录）同时承载运维可观测指标与商业成本机密。纯商业账务路由必须在路由表直接限定 `Roles(BIZ)` 并对前端隐藏对应导航与请求；跨角色共享的运维视图则必须通过集中式白名单投影器 `RoleDataProjection` 按角色剥离成本与凭据字段，确保 `R_VIEWER` 既能查看运行健康度又零泄漏成本数据、零触发 `403`。
2. **会话总时长（Session Lifetime）与访问凭证有效期（Access Token TTL）分离**：`system_settings.session_timeout` 定义的是管理员一次登录会话（`sxp` / Refresh Cookie）的最大存续期，而 Access Token 是高频免查库通行证，必须保持固定短 TTL（如 15~30 分钟）并配合 `auth_epoch` 短缓存复核，才能在保留无感续期体验的同时实现秒级封禁与降权生效。

---

## 3. 详尽可落地解决方案

### 3.1 收口控制面 RBAC 路由矩阵、敏感字段白名单裁剪与前端三角色同步

1. **路由访问矩阵收紧（`middleware/route_access.rs` & `access_assert.rs`）**
   - 将纯账务与对账接口统一收紧为 `Roles(BIZ)`（仅 `R_ADMIN`、`R_FINANCE`）：`GET /billing/accounts`、`/billing/adjustments`、`/billing/bills`、`/billing/bills/{id}`、`/billing/reconciliation/discrepancies`、`/billing/reconciliation/settings`（回显时对 `alert_webhook_url` 的密钥段脱敏）、`/tenants/{code}/spend`、`/tenants/{code}/account`、`/tenants/{code}/account/transactions`，同步更新 `access_assert.rs` 锁死基线。
2. **集中式角色字段裁剪器 `RoleDataProjection`**
   - **OLAP 模板编译期穷举门禁**：在 `mazbri-analytics` 与 `mazbri-mgmt` 对全部 24 个 `OlapQuery` 模板采用无 `_` 通配符的编译期穷举 `match`：
     - 纯金额模板 `cost_trend`、`top_by_dim(metric="cost")`、`billing_funnel_ch` 限定 `BIZ`；
     - 安全模板 `security_events`、`security_top` 限定 `R_ADMIN`；
     - 混合运维模板 `product_rank` 与 `tenant_overview` 保持对 `AnyRole` 开放（支撑 `R_VIEWER` 查看调用量、错误率与延迟），但在 `RoleDataProjection` 中对 `R_VIEWER` 强制剥离 `cost` 字段（置 `None`）。
   - **指标目录与共享资源脱敏**：
     - `get_metric_timeseries` 与 `get_metrics_catalog` 注入 `AdminClaims`：`cert_*`、`tls_*`、`license_*` 限定 `R_ADMIN`，`billing_*`、`balance_*` 限定 `BIZ`；
     - `GET /stats/ops/slos/status` 对非 `R_ADMIN` 剥离 `definitions[].query` PromQL 表达式；
     - `GET /channels*` 与 `GET /tenants/{code}/analytics` 对 `R_VIEWER` 剥离 `cost_price`、`total_amount`、`total_cost`；
     - `GET /stats/calls/*` 对 `R_VIEWER` 脱敏 `detail.raw` 与 `api_key_id`；
     - `GET /upstream-connectors/drafts*` 及 `/runs*` 对 `R_VIEWER` 掩码 `workbench_env` 变量值、静态认证头与试跑凭据回显。
3. **前端三角色零 `403` 严格对齐（`mazbri-web`）**
   - **账单结算路由收口**：在 [mazbri.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/router/modules/mazbri.ts#L130-L139) 移除 `/operations/bills`（`BillList`）路由 `roles` 中的 `'R_VIEWER'`，并清理 [index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/billing/bills/index.vue) 中面向非 `isBiz` 的冗余分支。
   - **运维总览安全摘要卡门控**：在 [index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/overview/index.vue#L242-L252) 使用 `<RoleGate gate="admin">` 包裹 `<SecurityGovSummary />`，并在刷新调度中仅当 `isAdmin.value` 为真时调用 `securityGovRef.value?.reload`，阻断非超管角色请求 `security_top`。
   - **租户详情页响应式页签与消费卡片门控**：在 [index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/tenants/[code]/index.vue) 将 `canViewApiKeys` 与 `tabOptions` 改为 `computed` 响应式属性，对 `R_VIEWER` 隐藏 `api-keys` 与 `account` 页签；在 [overview360.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/tenants/[code]/tabs/overview360.vue) 仅在 `isBiz` 下请求 `/tenants/{code}/spend` 并渲染近 30 天消费额卡片，`R_VIEWER` 正常加载 `tenant_overview`（已剥离 `cost`）展示调用量、成功率、P95 与健康分。

### 3.2 解耦短效 Access Token、实时核验 `auth_epoch`、收口分类提权新鲜度与保护唯一超管

1. **分离 Access Token 短 TTL 与会话 `sxp`，接入 `auth_epoch` 实时复核**
   - 修改 [auth_flow.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/admin/auth_flow.rs#L17-L33)：`system_settings.session_timeout` 仅控制登录会话绝对上限 `sxp` 与 Refresh Cookie 存活期；Access Token 固定采用短效 `config.auth.jwt_expiration_secs`（默认 `1800` 秒，上限 `3600` 秒），由前端单飞 `/auth/refresh` 无感续签。
   - `AdminClaims` 携带 `auth_epoch: i64`；`admin_auth_middleware` 通过 Redis 缓存 `admin:subject_state:{user_id}`（TTL `30s`，未命中查 `admin_users`）核验 `is_active == true && locked_until < now && db.auth_epoch == claims.auth_epoch && db.role == claims.role`，失配立即返回 `401 SESSION_STALE`。
   - 管理员停用、删除、改角色、重置密码、重置 MFA 及自助改密提交后，主动执行 `DEL admin:subject_state:{user_id}` 实现秒级全局踢下线与降权生效；MFA 二阶段 `verify` 与 `required_enable` 签发正式会话前强制重查数据库复核账号状态。
2. **保持六分类会话提权模型，修复 `fresh_for_lift` 新鲜度校验并收紧高危窗口 TTL**
   - 保持平台现有的六分类会话绑定 `StepUpWindow`（`certificate`/`admin`/`mfa`/`key`/`finance`/`platform`）与前端 `mfa-dialog-service.ts` 架构，避免逐请求单次消费与 `advance_totp_step` 防重放步长产生 30 秒互斥锁死（保障表格多选批量删除体验）。
   - 修复 [step_up.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/mfa/step_up.rs) 与 [crud.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/handlers/admin_management/crud.rs#L79-L99)：在 `StepUpWindow` 记录窗口签发时间戳 `issued_at: i64`；当 `lifts == true`（解除目标管理员强制 MFA）或关闭全局安全审计开关时，调用 `require_category_fresh(&state, &claims, category, 60)` 校验 `now - window.issued_at <= 60`（窗口建立超 60 秒则返回 `403 MFA_REQUIRED` 触发前端重新弹出二次验证刷新窗口）；并将 `Category::Admin` 与 `Category::Platform` 默认窗口上限收敛至 `300` 秒。
   - 在连接器版本回滚 [versions.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/upstream_connector/versions.rs) 补调 `guard_supplier_tls_privilege(&snapshot_config, is_admin(&claims))?`。
3. **原子登录失败计数与唯一超管源 IP 隔离防锁死**
   - 将 `auth_flow`、`change_password` 及 `first_login_change_password` 的验密失败分支全部统一至原子 SQL `repo::record_login_failure`（锁过期后自动归零重计为 `1`）。
   - 在 `/auth/login` 增加 Redis 滑动窗口限流 `login_fail:ip:{client_ip}` 与 `login_fail:user_ip:{username}:{client_ip}`（单源 IP 连续 5 次失败封禁该源 IP 15 分钟）；针对唯一超管账号（`is_super_admin == true`），**仅在 `(username, client_ip)` 源 IP 维度执行封禁，不写入全局 `admin_users.locked_until`**，彻底杜绝外部攻击者远程锁死唯一超管。
   - 统一未知账号与密码错误的 HTTP 状态码为 `401` 与相同提示文案；凡新增审计动作或 `set_failure_reason` 字面量，同步补齐 `audit/service.rs` `ACTION_LABELS`、前端 [labels.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/audit-logs/labels.ts) 及中英文词典。

### 3.3 控制面单库事务、跨库软停清理、配置中心 CAS 与审计可靠刷盘

1. **PostgreSQL 单事务与跨库幂等删除状态机**
   - [tenant_crypto.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/tenant_crypto.rs) 的 `set_key_status` 与 `create_key` 包裹进单事务，先以 `SELECT id FROM tenants WHERE id = $1 FOR UPDATE` 加锁并确认目标 `version` 存在且合法，再更新旧版本与目标版本状态。
   - 连接器 `versions::rollback` 将 `UPDATE suppliers`、锁定计算 `next_version_no` 与 `INSERT INTO upstream_connector_versions` 合并进单事务。
   - `delete_tenant` 跨库清理：先在主库将租户置为 `enabled = false` 并递增广播 `AUTH_API_KEY_VERSION_KEY` 立即阻断数据面流量，随后依次幂等删除 `auth_db`、`ratelimit_db`（`rate_limit_tenant_policies`、`rate_limit_control_commands`）、`cache_db`（`cache_configs`）关联策略，最后物理删除主库租户行。
2. **配置中心选择性导出、流式限长导入与 CAS 乐观锁**
   - [import_export.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/config_center/import_export.rs)：`ExportParams` 解析 `service_names` 逗号分隔列表，精确过滤仅打包指定微服务配置并写入审计详情；ZIP 导入改用 `file.take(MAX_EXTRACTED_FILE_SIZE + 1).read_to_string(&mut content)` 按真实解压字节累加校验 `MAX_TOTAL_EXTRACTED_SIZE`。
   - `UpdateConfigRequest` 增加可选 `expected_content_hash: Option<String>`，`cert-service` 执行 `UPDATE config_entries ... WHERE service_name = $1 AND ($6::text IS NULL OR content_hash = $6)`，冲突返回 `409 Conflict`；`batch_update` 包裹进单事务原子提交。
3. **`mazbri-audit` 精确外键降级、停机零丢失刷盘与 `ILIKE` 转义**
   - [sink.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-audit/src/sink.rs)：批量落库遇到 `23503` 外键冲突时，查询本批 `user_id` 中已不存在于 `admin_users` 的子集，仅将已删管理员对应的记录置 `user_id = None`（保留 `actor_username`），其余记录完整保留 `user_id`；`shutdown` 超时前将未提交批次与通道残余记录全部刷入 `0o600` 权限的 `FallbackWriter`。
   - 统一提取 `escape_ilike_pattern`（转义 `\`、`%`、`_` 并配 `ESCAPE '\\'`，检索词限长 `128`）应用于 `audit/service.rs`、`list_query/pg.rs`、`callback.rs` 及产品检索；为 `PgCredential`、`DatabaseConfig`、`RedisConfig` 实现脱敏 `Debug`。

---

## 4. 二阶与三阶影响深度推演

| 影响阶次 | 潜在连锁反应场景 | 架构消解与防护设计 |
| :--- | :--- | :--- |
| **二阶影响 1：若将高危操作改为逐请求单次消费 TOTP 步长，会撞击防重放锁导致批量删除失败** | `repositories/mfa.rs` 的 `advance_totp_step` 强制要求每次 TOTP 步长严格单调递增（30 秒内同一 6 位动态码不可二次通过）；若每次 API 调用都销毁提权凭证，表格勾选删除多条记录或 30 秒内连续保存两个配置会立即报错 `MFA_CODE_REUSED`。 | 坚持会话绑定的分类 `StepUpWindow` 模型，仅对解除强制 MFA 等极少数策略降级动作要求 `issued_at <= 60s` 的近期窗口，既杜绝长期空闲窗口被滥用，又与 RFC 6238 步长防重放及批量操作完全自洽。 |
| **二阶影响 2：解耦 `session_timeout` 与 Access Token TTL 后的会话超时表现** | 管理员在平台设置修改“会话超时时间（分钟）”时，期望控制登录态总有效时长。 | `session_timeout` 直接写入登录签发的 `sxp`（会话绝对截止时间）与 Refresh Cookie `Max-Age`，到达 `session_timeout` 后 `/auth/refresh` 准时拒签退回登录页；而单张 Access Token 保持 30 分钟短周期轮换，安全粒度与设置语义各司其职。 |
| **三阶影响：OLAP 模板编译期穷举分类消除双端权限漂移** | 彻底消除在 `analytics` 与 `mgmt` 维护平行字符串白名单导致的漏配风险。 | 新增任何 `OlapQuery` 变体时若未显式声明角色等级与字段投影规则，Rust 编译器直接拒绝通过编译。 |

---

## 5. 验收门禁与实测清单

1. **三角色与会话安全核验**
   - 分别以 `R_VIEWER`、`R_FINANCE`、`R_ADMIN` 登录调用全量账务、通道、24 个 OLAP 模板、时序指标、SLO 状态与连接器草稿接口：断言越权接口 100% 返回 `403` 或已剥离 `cost`/`cost_price`/凭据字段；在 `9223`/`9224` 浏览器下以 `R_VIEWER` 遍历全部可见菜单（含运维总览、租户详情全页签），断言控制台与网络面板**零 `403` 请求、零报错弹窗**。
   - 登录管理员 B 获取有效会话后，用超管停用 B 或将其降权为 `R_VIEWER`：断言 1 秒内 B 调用免 MFA 写接口立即返回 `401`/`403`。
   - 从单一测试源 IP 对超管账号连续提交 10 次错误密码：断言该源 IP 被拦截，而从另一正常管理 IP 输入正确密码可立即登录超管账号。
   - 在配置中心仅勾选 `mazbri-gateway` 导出 ZIP，解压断言包内仅含 `mazbri-gateway.toml` 及其元数据。
2. **部署与验证顺序**
   - 先在 `mazbri-web` 执行 `pnpm build` 与全量前端测试，再编译并依次更新 `mazbri-cert-service` 与 `mazbri-mgmt`。
