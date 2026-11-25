# Step 08：可观测四大引擎校准、前端安全与竞态收敛及确定性 CI 发版

## 1. 检测出的问题清单与源码定位

| 编号 | 级别 | 问题描述 | 精确源码位置 |
| :--- | :--- | :--- | :--- |
| **S08-1** | Critical | `mazbri-analytics` 路由无内部鉴权，`fetch_upstream_usage_summary` 未校验参数且采用手工单引号转义，可被 ClickHouse 反斜杠 `\` 逃逸引发 SQL 注入；搜索含 `?` 的 URL 因 `clickhouse 0.12` 客户端拆分占位符必报 `500`，ClickHouse `LIKE` 单反斜杠转义失效，空 `IN ()` 穿透为 `500` | [client.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/clickhouse/client.rs#L141-L184)、[sanitization.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/greptimedb/sanitization.rs#L108-L162)、[mod.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/list_query/mod.rs#L517-L556) |
| **S08-2** | Critical | `release-please.yml` 半态恢复兜底调用 `release.yml` 时，`release.yml` 各 Job 未指定 `ref` 默认检出 `HEAD`，会将后续新合入代码覆盖推送至历史正式版镜像 Tag；`ci.yml` 漏检根级 `mazbri-license.pub` 与跨包 `labels.ts` 契约单测，`release.yml` 复用镜像仅探活 Docker Hub 未探活阿里云 ACR | [release-please.yml](file:///workspace/rust-proj/mazbri/.github/workflows/release-please.yml#L69-L101)、[release.yml](file:///workspace/rust-proj/mazbri/.github/workflows/release.yml#L63-L107)、[release.yml](file:///workspace/rust-proj/mazbri/.github/workflows/release.yml#L225-L234)、[ci.yml](file:///workspace/rust-proj/mazbri/.github/workflows/ci.yml#L99-L116) |
| **S08-3** | Critical | 前端全屏路由（无代码工作台 `isFullPage: true`）通过 `v-show="!isFullPage"` 整块隐藏 `#app-global`，导致全局防截屏水印、锁屏遮罩、MFA 提权弹窗与个人安全抽屉在全屏工作台失效；且 `isLock` 未持久化、路由守卫未拦截地址栏切换绕过锁屏 | [index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/index/index.vue#L16-L18)、[beforeEach.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/router/guards/beforeEach.ts#L74-L105)、[user.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/store/modules/user.ts#L193-L199) |
| **S08-4** | High | **可观测四大后台引擎失准**：① SLO `rewrite_window` 仅替换首个 `[...]` 破坏分子/分母比值公式、查询失败或空序列静默当 `SLI=1.0` 掩盖故障、忽略 `window_days` 使 30d 错误预算 5 分钟重置；② 告警唯一索引与 `insert_firing` 残留表约束不存在的 `'acked'` 死状态谓词，且存在新告警自抑制与静默事件恢复误发通知；③ 异常检测将“当前未走完的残缺小时”与“历史完整 60 分钟”做绝对值比较每 5 分钟持续误报；④ 安全扫描单轮最多 150 次逐行 HTTP `INSERT` 且无冷却重复落库 | [mod.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/slo/mod.rs#L177-L267)、[store.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/alerting/store.rs#L133-L223)、[init.sql](file:///workspace/rust-proj/mazbri-max/init-dbs/src/sql/schemas/admin/init.sql#L413-L426)、[mod.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/anomaly/mod.rs#L86-L161)、[mod.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/security/mod.rs#L133-L260) |
| **S08-5** | High | 前端全站通用 `generateCsv`/`serializeRawCsv` 缺失公式注入转义与表头/数组引号转义；工作台 `AuthProfileFields` 明文展示密钥、`WorkbenchKvRows` 批量模式明文暴露遮罩凭据且向只读角色开放导出 `.env.json`；`/metrics/timeseries` 允许 `step=1` 单次拉取百万点，13 个直查 `request_logs` 的 OLAP 明细模板遗漏 `FINAL` | [csv.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/utils/csv.ts#L60-L147)、[AuthProfileFields.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/hopp/AuthProfileFields.vue#L25-L64)、[metrics.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/greptimedb/metrics.rs#L645-L683)、[olap.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/clickhouse/olap.rs#L1024-L1408) |
| **S08-6** | Medium | 前端证书抽屉解锁私钥及多处运维/租户面板缺少请求序号 `seq` 与 `AbortController` 守卫；登出未清理页签轨迹与 API 模块内存缓存；`tar` 归档路径超 100B 截断丢扩展名；`CreateApiKeyRequest` 前后端类型失配、`ts-rs` 生成类型未直接消费、残留模板死文件（`system-manage.ts`、`clean-dev.ts`、`art-wang-editor` 样式、`ratelimit_platform.rs`）与未用依赖（`xlsx`、`@wangeditor/*` 等）、`pnpm test` 漏扫 6 个单测文件 | [CertificateDetailDrawer.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/certificates/modules/CertificateDetailDrawer.vue#L538-L591)、[archive.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/utils/archive.ts#L20-L38)、[system-manage.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/api/system-manage.ts#L1-L35)、[package.json](file:///workspace/rust-proj/mazbri/mazbri-web/package.json#L15) |

---

## 2. 第一性原理本质归因

1. **查询语言参数与代码边界分离（Code-Data Separation）**：手工拼接转义受制于 ClickHouse C 风格反斜杠词法与驱动 `?` 占位符展开规则。唯有全量改用驱动原生 `.bind(...)` 参数绑定，才能同时根除反斜杠逃逸注入与含 `?` 查询词 `500` 报错。
2. **统计测度同构与状态约束一致**：异常检测必须在等长完整样本窗口间比较（已闭合整点小时对比历史同整点小时）；数据库唯一索引谓词与仓储层 `ON CONFLICT` 必须严格对齐表 `CHECK` 约束定义的真实状态闭集。
3. **构建可重现性（Hermetic & Immutable Release）**：正式版发版流水线的唯一代码源只能是被发布版本 Tag 所指向的不可变 Commit SHA，严禁隐式检出分支移动指针 `HEAD`。

---

## 3. 详尽可落地解决方案

### 3.1 `mazbri-analytics` 全量参数化、内部令牌鉴权与四大监控引擎校准

1. **ClickHouse 原生参数绑定、查询护栏与服务间 `internal_token` 鉴权**
   - 将 [client.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/clickhouse/client.rs)、`queries.rs` 与 `list_query` 全量改为 `clickhouse::Client::query(&sql).bind(...)` 原生参数绑定；对 `LIKE` 绑定值转义 `\`、`%`、`_`；在 `validate_filters` 拦截空 `IN` 数组并统一映射 HTTP `400`；统一 `parse_time_string` 支持小数秒并严格采用左闭右开 `[start, end)` UTC 时间窗。
   - 对 13 个直查 `ReplacingMergeTree`（`request_logs`）的 OLAP 模板及拓扑查询补齐 `FINAL`（统一上游拓扑边源节点为 `mazbri-bypass`）；`product_rank_sql` 复用 `call_semantics::SQL_IS_FAILURE`。
   - `/metrics/timeseries` 强制校验采样点数 `(end - start) / step <= 1500`；为 `ClickHouseClient` 全量查询挂载 HTTP 超时与 `with_query_timeout`；Trace 反查下推 `errors_only` 与 `min_duration_ms` 并校验 `validate_trace_id`。
   - 在 `mazbri-analytics`（`/health` 除外）与 `mazbri-mgmt` `[analytics]` 配置段启用常量时间比对的 `internal_token`，同步更新 `mazbri-max/docker/up.sh` 与 `mconfigs.tmpl/{analytics,mgmt}-bootstrap.toml` 自动注入同源随机内部令牌。
2. **校准 SLO、告警、异常检测与安全扫描四大引擎**
   - **SLO 引擎（[mod.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/slo/mod.rs)）**：
     - 遍历替换 PromQL 中所有非引号字面量内的区间向量选择器 `[duration]`（及 `{{window}}` 占位符），确保分子与分母窗口一致；
     - 区分查询异常（保留上一有效值并递增 `mazbri_slo_eval_errors_total`，严禁伪装为 `1.0`）与零流量窗口（无错误消耗预算，`burn_rate = 0`）；
     - 快慢烧告警读实时短窗，7d/30d 周期累计 `error_budget_remaining` 直接聚合每 5 分钟写入的轻量表 `slo_sli_5m`（30 天仅 8,640 行标量）；删除或改名 SLO 时同步清理孤儿 `alert_rules`；`handlers/slo.rs` 按 `slo` 分组取最新一条记录。
   - **告警引擎（[store.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/alerting/store.rs) & [init.sql](file:///workspace/rust-proj/mazbri-max/init-dbs/src/sql/schemas/admin/init.sql)）**：
     - 对齐 `alert_events` 表约束 `CHECK (status IN ('firing', 'resolved'))`（告警抑制由 `alert_silences` 独立承载）：清理 `admin/init.sql` 中 `uq_alert_events_active_fingerprint` 唯一索引及 `AlertStore::insert_firing` `ON CONFLICT` 子句里遗留的无效 `'acked'` 谓词，统一收敛为 **`WHERE status = 'firing'`**；
     - 在 `ActiveLabels` 增加 `event_id` 并在 `is_inhibited` 中排除 `src.event_id == fire.event_id` 消除新触发告警自抑制；
     - `resolve` 返回原事件静默与抑制标志，触发期未外发通知的事件在恢复时同样静默落库不外发恢复通知；使用 `Semaphore(16)` 限制并发外发通知并逐个容错解析收件人邮箱。
   - **异常检测（[mod.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/anomaly/mod.rs) & `admin/init.sql`）**：
     - 改为评估**上一个已完整结束的 UTC 整点小时** `[toStartOfHour(now(), 'UTC') - INTERVAL 1 HOUR, toStartOfHour(now(), 'UTC'))`，在 7 个历史同整点小时样本上计算中位数与 MAD；
     - 在 `admin/init.sql` 为 `anomaly_events` 增加 `(kind, entity_type, entity_id, window_hour)` 唯一索引配合 `ON CONFLICT DO NOTHING` 去重，并校验 `sigma > 0`。
   - **安全扫描（[mod.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/security/mod.rs)）**：
     - 单轮汇总后通过 `clickhouse::Client::insert("security_events")` RowBinary 单次批量提交；
     - 增加 15 分钟 TTL 内存冷却缓存（键 `(kind, tenant_code, api_key_id, client_ip)`），过滤 `tenant_code != ''`，为 `key_spike` 设置最小请求门槛 `recent >= 60`，7 天历史 IP 基线改读小时级预聚合表。

### 3.2 前端安全加固、并发请求守卫与契约/死代码清理（`mazbri-web` & `mazbri-mgmt`）

1. **全屏工作台全局守护层保留与层级校准、凭据遮罩**
   - 在 [index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/index/index.vue) 移除 `#app-global` 上的 `v-show="!isFullPage"`（仅在 `isFullPage` 下按条件跳过 `ArtFireworksEffect`），确保 `ArtWatermark`（`z-index: 3100`）、`ArtScreenLock`（`z-index: 2500`）、`MfaStepUpDialog` 与 `PersonalSecurityDrawer` 通过 Teleport 高于 `.connector-workbench-page` 正常覆盖展示；在 `userStore.persist.pick` 纳入 `isLock` 与解锁口令哈希，并在 [beforeEach.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/router/guards/beforeEach.ts) 拦截 `isLock === true` 期间的一切路由切换。
   - 为 `WorkbenchGhostField` 增加密文遮罩模式覆盖 [AuthProfileFields.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/hopp/AuthProfileFields.vue) 全部密钥与 Token 字段；`secretValues = true` 时禁用 `WorkbenchKvRows` 明文批量文本框；对 `!canWrite` 只读角色隐藏顶栏与侧栏 `.env.json` 导出入口并掩码展示环境变量预览浮层。
2. **通用 CSV 防公式注入与归档文件名净化**
   - 在 [csv.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/utils/csv.ts) 的 `generateCsv` 与 `serializeRawCsv` 内置公式注入防护（首字符命中 `=`、`+`、`@`、`\t`、`\r` 或非纯十进制负数的 `-` 时前置 `'`），补齐表头与数组元素双引号转义，并修复 `parseCsv` 映射方向。
   - 在 [archive.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/utils/archive.ts) 剥离条目名中的 `/`、`\`、`..` 与控制字符，超长前缀按 UTF-8 字符边界截断以完整保留尾部 UUID 与 `.csv` 扩展名；浏览器不支持 `CompressionStream` 时回退 `.tar` 扩展名与 `application/x-tar` MIME。
3. **请求序号取消守卫 `useLatestRequest` 与登出状态清零**
   - 新增组合式函数 `useLatestRequest(fn)`（内置递增 `seq`、自动中止上一请求的 `AbortController` 与引用计数 `loading`），统一接入 [CertificateDetailDrawer.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/certificates/modules/CertificateDetailDrawer.vue)（写入 `keyPem` 前二次校验 `visible.value && props.certId === requestedId`）、`useMetricTimeseriesPanel.ts`、`CallMonitorTable.vue`、`ops/traces/detail.vue`、`EventsPanel.vue`、`BillingPipelinePanel.vue`、`FinanceOverviewPanel.vue`、`tenants/[code]`、`config-center` 与工作台会话切换。
   - `clearUserState()` 登出闭环：同步清空 `useWorktabStore`、`searchHistory`、关闭 `personalSecurityStore` 抽屉并重置 `api/{service-names,certificate,settings}.ts` 模块级内存缓存；修复 `useTheme.ts` 使用 `classList.remove/add` 保留 `html.radius-zero` 类，将 `el-ui.scss` 硬编码色值替换为语义变量。
4. **`ts-rs` 契约贯通与模板遗留死代码彻底清除**
   - 对齐 `types/api/api-key.ts` 的 `CreateApiKeyRequest`（移除后端不存在的 `warmup_enabled`），使前端业务层直接导入消费 `types/api/generated/*`（后端安全整数范围 `i64` 标注 `#[ts(type = "number")]`）。
   - 删除后端未挂载路由的死处理器 [ratelimit_platform.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/handlers/analytics_handlers/ratelimit_platform.rs)；删除前端模板遗留文件 [system-manage.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/api/system-manage.ts)、[clean-dev.ts](file:///workspace/rust-proj/mazbri/mazbri-web/scripts/clean-dev.ts)（同步清理 [MenuProcessor.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/router/core/MenuProcessor.ts) 后端菜单死分支与 [index.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/api/index.ts) 重导出）以及未挂载的 `fetchAccountByTenant`、`fetchUsageSummary`、`updateApiKeyWarmup`。
   - 从 [package.json](file:///workspace/rust-proj/mazbri/mazbri-web/package.json) 移除零业务引用的 `xlsx`、`@wangeditor/*`、`jszip`、`xgplayer`、`vue-img-cutter`，同步删除遗留样式 [style.scss](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/forms/art-wang-editor/style.scss) 及 [dark.scss](file:///workspace/rust-proj/mazbri/mazbri-web/src/assets/styles/core/dark.scss) 中的 `--w-e-*` 规则；将 `pnpm test` 脚本改为扫描全仓 `src/**/*.test.ts`；`vite.config.ts` 直接读取 `../VERSION` 注入版本号且仅在 `development` 模式加载 `vueDevTools()`。

### 3.3 确定性 CI 门禁与双镜像仓发版加固（`.github/`）

1. **锁定不可变 Tag Commit SHA 检出（`release.yml`）**
   - 在 [release.yml](file:///workspace/rust-proj/mazbri/.github/workflows/release.yml) 的 `meta` Job 解析 `refs/tags/v$V` 对应的 peeled commit SHA 输出为 `needs.meta.outputs.target_sha`；下游 `frontend`、`plan`、`compile`、`image` 全部 Job 的 `actions/checkout` 显式绑定 `ref: ${{ needs.meta.outputs.target_sha }}`，`plan` 增量对比改为 `git diff --name-only "v$BASE" "$TARGET_SHA"`。
2. **补齐 `ci.yml` 路径过滤与双镜像仓存在性探活**
   - 在 [ci.yml](file:///workspace/rust-proj/mazbri/.github/workflows/ci.yml) 变更检测中加入根级 `mazbri-license.pub` 与 `docker/**`；当 `audit-logs/labels.ts` 变更时将 `mazbri-cert-service` 纳入 `bin_scope` 编译单测。
   - 在 `release.yml` 的 `plan` Job 同时探活 Docker Hub（`HUB_NS`）与阿里云 ACR（`ACR_NS`）双端基线镜像均存在时才走 `reuse` 重打标签，任一端缺失即走 `build` 构建推送。

---

## 4. 二阶与三阶影响深度推演

| 影响阶次 | 潜在连锁反应场景 | 架构消解与防护设计 |
| :--- | :--- | :--- |
| **二阶影响 1：全屏工作台保留 `#app-global` 后浮层遮挡与主题令牌继承** | 工作台 `.connector-workbench-page` 采用固定定位占满视口且内部弹窗默认挂载至 `pageMount`；若全局 `MfaStepUpDialog` 或 `ArtScreenLock` 层级低于工作台，会导致强制发布触发 MFA 时弹窗被遮挡无法交互。 | 校准 `.connector-workbench-page` 基础层级低于 Element Plus 全局 Dialog / Drawer 与 `ArtScreenLock`（`2500`）/ `ArtWatermark`（`3100`），使工作台内触发强制发布 MFA 提权、个人安全抽屉与锁屏均在最顶层清晰交互。 |
| **二阶影响 2：删除 `system-manage.ts` 与 `@wangeditor/*` 时的残留引用破坏构建** | `MenuProcessor.ts`、`api/index.ts` 与 `dark.scss` 原本引用了模板遗留的菜单接口与富文本变量，若仅删包与单文件会导致 `vue-tsc` 或样式打包报错。 | 在同一批次内连根清除 `MenuProcessor.ts` 未使用的后端动态菜单分支、`api/index.ts` 桶导出及 `art-wang-editor` 关联样式规则，确保 `pnpm build` 零死引用。 |
| **三阶影响：`mazbri-analytics` 启用 `internal_token` 与部署编排的一致性** | `AnalyticsConfig` 开启 `deny_unknown_fields` 且强制校验内部令牌后，配置模板与二进制必须同步。 | 统一在 `mazbri-max/docker/up.sh` 与 `mconfigs.tmpl/{analytics,mgmt}-bootstrap.toml` 自动派生注入一致的 `internal_token`，通过 `redep rerun` 一站式生成双端配置。 |

---

## 5. 验收门禁与实测清单

1. **全栈回归验证**
   - 在调用监控检索框输入含 `?a=1&b=2`、`%`、`\`、`'` 的查询词，断言返回 `200` 精确结果集；构造含反斜杠的 `supplier_code="test\\"` 查询上游汇总接口，断言参数化绑定正常返回空集无语法报错。
   - 触发一条测试告警验证 `firing` 正常入库且不自抑制，解除故障条件后验证自动流转为 `resolved`；为告警配置静默窗后再触发并解除故障，验证触发与恢复全程零外发轰炸。
   - 在无代码工作台开启全局水印、触发锁屏并测试强制发布弹出 MFA 提权框：断言水印、锁屏遮罩与 MFA 弹窗层级正常可见，刷新页面或修改地址栏 Hash 无法绕过锁屏。
   - 执行 `mazbri-web` `pnpm install --frozen-lockfile`、`pnpm build` 与全仓 `src/**/*.test.ts` 全绿，双 Rust 工作区 `--release --locked --workspace --all-targets -- -D warnings` Clippy 零警告与全量 `cargo test` 全绿，`mazbri-web/src/types/api/generated` 零未提交差异。
2. **终验闭环顺序**
   - 更新 `init-dbs` (`admin/init.sql`)、`mconfigs.tmpl`、前端静态构建产物及 `mazbri-analytics`、`mazbri-mgmt` 二进制，执行 `redep rerun` 并通过 `9223`/`9224` 浏览器完成 `1440×900` 与 `1920×1080` 双分辨率、明暗双主题及三角色全链路零缺陷验收。
