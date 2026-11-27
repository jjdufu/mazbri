# Step 02：全链路东八区时钟同构、ClickHouse 谓词防遮蔽与链路瀑布图对齐

## 1. 覆盖问题清单与源码精确定位

| 编号 | 关联审计项 | 级别 | 问题描述与全栈契约定位 | 精确源码位置 |
| :--- | :---: | :---: | :--- | :--- |
| **W02-1** | `C-01` | P0 | **输入/存储/展示三端时钟割裂**：① 展示端 `TrafficCallsPanel` 手写 `new Date(ts * 1000).getHours()` 读宿主本地时区、`TrafficTracesPanel` 散点图 `xAxis: { type: 'time' }` 缺时区格式化器与开始时间 Tooltip、`art-watermark` 手写本地时间，在 `UTC-7` 终端与下方 `Asia/Shanghai` 表格产生 15 小时撕裂；② 输入端 `SilencesPanel` 绑定 `value-format="YYYY-MM-DDTHH:mm:ss[Z]"` 把本地墙钟硬拼字面量 `Z` 导致静默窗口向未来偏移 **+8 小时**，`TimeRangePicker` / `DatePicker` 读浏览器本地时区；③ 存储端 ClickHouse 按日分桶未传 `'Asia/Shanghai'`（按北京时间 `08:00` 切日）；④ `TrafficCallsPanel` 与 `TrafficTracesPanel` 硬编码 `#409eff`/`#f56c6c`，`<ElCheckTag>` 高度 `28px` 与 `32px` 工具栏差 `4px` | [TrafficCallsPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/traffic/modules/TrafficCallsPanel.vue#L186-L267)、[TrafficTracesPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/traffic/modules/TrafficTracesPanel.vue#L579-L635)、[SilencesPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/alerts/SilencesPanel.vue#L158-L165)、[TimeRangePicker.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/ops/TimeRangePicker.vue#L10-L25)、[DatePicker.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/common/DatePicker.vue#L69-L77)、[art-watermark/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/others/art-watermark/index.vue#L91-L100)、[olap.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/clickhouse/olap.rs#L807-L812) |
| **W02-2** | `A-03`, `B-02` | P0 | **ClickHouse OLAP 谓词缺失与列别名遮蔽**：① `security_top_sql` 聚合 `security_agg_1h` 漏写 `HAVING sum(fail_count) > 0`，将 `fail_count = 0` 的正常 IP 混入鉴权失败榜并渲染红色危险胶囊，且前端 `SecurityGovSummary` 点击行跳转未携带 IP/Key 过滤参数；② **`tenant_errors_sql` 中 `SELECT if(error_code = '', 'UNKNOWN', error_code) AS error_code` 在 ClickHouse 中遮蔽了 `WHERE` 子句 `SQL_IS_FAILURE` 内的同名列 `error_code`**，导致 `100%` 成功率的健康租户（如 477 次 `200 OK`）被全量统计为 `UNKNOWN: 477` 错误；③ `top_by_dim_sql` 缺 `AND {dim} != ''` 过滤 | [call_semantics.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/call_semantics.rs#L41-L59)、[olap.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/clickhouse/olap.rs#L1001-L1017)、[olap.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/clickhouse/olap.rs#L1277-L1294)、[olap.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/clickhouse/olap.rs#L1408-L1422)、[SecurityGovSummary.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/overview/modules/SecurityGovSummary.vue#L15-L103) |
| **W02-3** | `C-02` | P2 | `链路详情` 在单 Span 场景下左栏 `.ops-trace__waterfall` 缺 `align-self: start` 被右栏属性面板通过 Grid 默认 `stretch` 拉高；顶栏 `#actions` 内 `Trace ID` 左偏至 `x=963px` 缺少左侧标题锚点；`code.file.path` 跨仓路径未剥离 CI 构建机前缀且 `<dd>` 断词生硬；底部关联卡片的「调用日志」与「计费流水」双页签缺数值列右对齐、计费金额未格式化且表头空格不统一 | [traces/detail.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/traces/detail.vue#L3-L35)、[traces/detail.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/traces/detail.vue#L350-L489)、[traces/detail.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/traces/detail.vue#L953-L973)、[traces/detail.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/traces/detail.vue#L1248-L1261) |

---

## 2. 第一性原理本质归因

1. **全链路墙钟单射同构（公理 U2）**：跨时区 SRE 值班场景下，控制台声明单一展示时区 `Asia/Shanghai`（`UTC+8`）后，不仅输出端图表轴与表格列必须同构，**输入端墙钟选择器**（避免依赖宿主 OS `new Date()` 与夏令时偏移，统一采用无时区字面串 + 固定 `+08:00` 解析）与**存储端按日聚合边界**（`toStartOfInterval(..., 'Asia/Shanghai')`）也必须闭环在同一时区坐标系内。
2. **ClickHouse 词法作用域与聚合定义域严密性**：ClickHouse SQL 解析器允许 `SELECT` 投影别名在 `WHERE` / `HAVING` 中覆盖同名物理表列。当 `SQL_NORM_ERROR_CODE` 被赋予别名 `AS error_code` 时，`WHERE` 中未加表名前缀的 `error_code = ''` 被绑定到别名值 `'UNKNOWN'`（恒为 `false`），致使 `NOT (... AND error_code = '')` 退化为恒真，把所有成功请求误判为失败。对公共 SQL 片段强制限定物理表名 `request_logs.<col>` 并在 `HAVING` 显式书写聚合函数 `sum(fail_count) > 0`，才能在编译与执行期彻底消除作用域污染。

---

## 3. 详尽可落地技术方案

### 3.1 打通东八区输入/输出原语并收敛流量监控面板（`mazbri-web`）

1. **统一墙钟格式化与无时区偏差解析函数**
   - 在 [format.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/utils/format.ts) 补齐：
     - `parseDisplayWallClockMs(wallClock: string): number`：接收 `"YYYY-MM-DD HH:mm:ss"` 或 `"YYYY-MM-DDTHH:mm:ss"` 字面串，显式拼接 `+08:00` 解析为 UTC 毫秒时间戳，彻底规避宿主浏览器本地时区与夏令时跳变；
     - `formatDisplayDateYmd(ms = Date.now()): string`：基于 `DISPLAY_TIME_ZONE` 输出东八区 `"YYYY-MM-DD"`，替换 [usage-reconciliation/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/billing/usage-reconciliation/index.vue#L1146)、`ExportDialog.vue:89`、`whitelist.vue:554`、`audit-logs/index.vue:1018` 中在每日 `00:00~08:00` 错退一天的 `toISOString().slice(0, 10)`。
   - 修复输入选择器时区：
     - [SilencesPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/alerts/SilencesPanel.vue#L158-L165)：移除错误的 `[Z]` 字面量格式，改为 `value-format="YYYY-MM-DD HH:mm:ss"` 并在提交时经 `new Date(parseDisplayWallClockMs(v)).toISOString()` 转为标准 UTC ISO 字符串（回显时用 `formatDateTime` 转为东八区墙钟）；
     - [TimeRangePicker.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/ops/TimeRangePicker.vue#L10-L25)、[DatePicker.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/common/DatePicker.vue#L69-L77) 与 `audit-logs` 自定义区间选择统一改用墙钟字面串 + `parseDisplayWallClockMs` 转换，并在 `TimeRangePicker` 旁展示轻量 `UTC+8` 时区徽标。
   - 修复展示端漏口：
     - [TrafficCallsPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/traffic/modules/TrafficCallsPanel.vue#L186-L195)：删除手写 `new Date().getHours()`，复用 [time-range.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/utils/ops/time-range.ts#L70-L79) 的 `formatAxisTime(b.ts * 1000, spanSecs)`；
     - [TrafficTracesPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/traffic/modules/TrafficTracesPanel.vue#L579-L635)：为散点图 `xAxis` 挂载 `axisLabel.formatter: (val: number) => formatAxisTime(val, end - start)`，并在 Tooltip 补齐东八区开始时间；
     - [CallMonitorTable.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/traffic/modules/CallMonitorTable.vue#L602) 毫秒时间格式化固定传入 `"sv-SE"` locale，并将左侧快捷筛选 `<ElCheckTag>` 样式统一设为 `height: 32px`；
     - [art-watermark/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/others/art-watermark/index.vue#L91-L100) 水印时间改调 `formatDateTime(Date.now())`。

2. **主题语义图表色与空窗防框选死锁**
   - 在 [useOpsChartHost.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/composables/ops/useOpsChartHost.ts#L12-L56) 的 `OpsChartThemeTokens` 扩展 `primaryColor` 与 `dangerColor`（实时读取 `--el-color-primary` / `--el-color-danger` 计算值），替换 `TrafficCallsPanel.vue` 与 `TrafficTracesPanel.vue` 中的 `#409eff` / `#f56c6c` 硬编码，并为调用趋势柱状图补充 `barMaxWidth: 32`。
   - 在 `TrafficCallsPanel.vue`：当 `histogram !== null && !histogramLoading && histogram.total === 0` 时，将图表画布折叠为紧凑单行状态条，**但始终保留卡片头部（含 `previousRange !== null` 时的「恢复缩放」按钮）**，既消除无流量时的上下双大空盒，又杜绝框选进空窗后无法恢复缩放以及首屏 `150ms` 加载延迟引起的 CLS 跳动。

### 3.2 修复 ClickHouse 列别名遮蔽、安全榜单谓词与日桶时区（`mazbri-analytics` & `mazbri-web`）

1. **根除 `call_semantics.rs` 被投影别名遮蔽引发的假错误**
   - 修改 [call_semantics.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/call_semantics.rs#L41-L59)：将 `SQL_IS_FAILURE`、`SQL_NORM_ERROR_CODE`、`SQL_COUNT_FAILURES`、`SQL_COUNT_SUCCESSES` 中的列名显式限定为 `request_logs.error_class`、`request_logs.status_code`、`request_logs.error_code`，并在 [olap.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/clickhouse/olap.rs#L1408-L1422) `tenant_errors_sql` 中确认 `WHERE` 谓词始终引用物理表列，使 `100%` 成功率租户的 `tenant_errors` 准确返回空数组 `[]`。
2. **补齐 `security_top_sql`、`top_by_dim_sql` 谓词与东八区日桶边界**
   - [olap.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/clickhouse/olap.rs#L1288-L1294) `security_top_sql`：补充 `HAVING sum(fail_count) > 0 ORDER BY fail_count DESC, req_count DESC LIMIT {limit}`（严禁简写为易触发 `Code: 215 NOT_AN_AGGREGATE` 的列别名 `HAVING fail_count > 0`）；同时在 [SecurityGovSummary.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/overview/modules/SecurityGovSummary.vue#L95-L103) 点击行时携带 `client_ip` 或 `api_key_id` 参数跳转调用监控。
   - [olap.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/clickhouse/olap.rs#L1001-L1017) `top_by_dim_sql`：增加 `AND {dim} != ''`，并在 `p.metric == Metric::Cost` 时附加 `HAVING value > 0`。
   - [olap.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/clickhouse/olap.rs#L807-L812) `bucket_expr` 与 `queries.rs:343`：对按日及以上分桶（`bucket.seconds() >= 86_400`）显式传入时区 `'Asia/Shanghai'`（`toStartOfInterval({ts}, INTERVAL {} SECOND, 'Asia/Shanghai')`），使日趋势图与北京时间自然日 `00:00~24:00` 严格对齐。

### 3.3 链路详情瀑布图防拉伸、顶栏左锚定与底部双表对齐（`mazbri-web`）

在保留 `v0.6.6` 现有树形连接件、`upstream_wire_ns` / `platform_overhead_ns` 指标及 `fold-upstream` 比例尺的基础上精修 [traces/detail.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/traces/detail.vue)：
1. 将 `.ops-trace__waterfall` 从第 958 行合并规则中单拆补充 `align-self: start;`（不设 `max-height`，完整保留右栏 `.ops-trace__panel` 的 `position: sticky` 整页联动滚动）。
2. 将 `Trace ID`（`<CopyableText>`）与入口操作名移至 `OpsPage` 左侧主标题槽位锚定（消除 `left: 963px` 右偏悬空），右侧 `#actions` 仅承载紧凑等宽指标组。
3. 新增纯函数 `formatCodeFilePath(raw: string): string`（剥离 `/^.*?\/(?=mazbri-(?:bin|libs|web)\/)/` 构建机绝对前缀，悬停与复制保留原值），并将 `.ops-trace__kv :deep(dd)` 升级为 `overflow-wrap: anywhere`。
4. 统一底部卡片「调用日志」与「计费流水」**双页签**的表头空格（`耗时 (ms)` / `上游耗时 (ms)`），将 `channel_code` 放宽至 `min-width="180"`，并为日志页签的耗时列及计费页签的 `retry_count`、`billable_units`、`billable_amount` 补齐右对齐、`tabular-nums` 与微元精度货币格式。

---

## 4. 二阶、三阶影响与边界守卫

| 边界场景 | 连锁风险分析 | 消解与防护设计 |
| :--- | :--- | :--- |
| **北美/欧洲夏令时切换日选择绝对时间** | 若对浏览器本地 `Date` 做固定小时差加减，在宿主夏令时跳变日会产生 1 小时偏移。 | 选择器输出纯墙钟字面串，统一由 `+08:00` 定点解析为 epoch，与宿主 OS 时区及夏令时完全解耦。 |
| **框选放大至无流量时间段后的回退路径** | 若整张隐藏直方图 `OpsCard`，头部的「恢复缩放」按钮会随之消失导致用户被困在空窗口。 | 仅收缩图表画布高度，始终保留卡片头部工具区与「恢复缩放」按钮可见。 |
| **ClickHouse `HAVING` 与 `WHERE` 别名解析规则** | ClickHouse 对同名别名与底层列的绑定在不同设置下易触发聚合报错或谓词反转。 | `HAVING` 强制使用 `sum(fail_count) > 0`；`call_semantics.rs` 强制加 `request_logs.` 表限定符。 |

---

## 5. 验收门禁与实测清单

1. **自动化单测必测项**
   - 前端（`node:test`）：在 `TZ=America/Denver` 下验证 `formatAxisTime`、`parseDisplayWallClockMs`、`formatDisplayDateYmd` 与 `formatCodeFilePath` 输出精确一致。
   - 后端 `mazbri-analytics`：单测断言 `security_top_sql` 含 `HAVING sum(fail_count) > 0`、`tenant_errors_sql` 含 `request_logs.error_code` 限定符、`bucket_expr` 日桶含 `'Asia/Shanghai'`。
2. **构建、热替换与 `9224` 实机核验**
   - 编译 `mazbri-web`、`mazbri-analytics`、`mazbri-mgmt`，依次原子热替换重启 `mazbri-mazbri-analytics-1` ➔ `mazbri-mazbri-mgmt-1`。
   - 连接 `127.0.0.1:9224`（`America/Denver` 时区）验证：① `/#/ops/traffic` 柱图/散点图 X 轴、Tooltip 与下方表格时间 100% 同步在东八区 `UTC+8`，框选空区间仍可点击「恢复缩放」；② `/#/dashboard` 安全治理摘要无 `0` 次失败记录，点击条目带过滤条件跳转；③ 租户 `D7XBRXV5` 在 `100%` 成功率下 `错误分布` 准确为空态（不再出现 `UNKNOWN: 477`）；④ 单 Span 与 22 Span 链路详情排版紧凑对齐。
