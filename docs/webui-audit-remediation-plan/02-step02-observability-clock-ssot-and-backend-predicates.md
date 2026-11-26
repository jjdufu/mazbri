# Step 02：全站单一展示时钟源（`UTC+8`）、安全榜单后端谓词同构与链路瀑布图对齐

## 1. 覆盖问题清单与源码精确定位

| 编号 | 关联审计项 | 级别 | 问题描述与核验纠偏 | 精确源码位置 |
| :--- | :---: | :---: | :--- | :--- |
| **W02-1** | `C-01` | P0 | `调用链路` 直方图手写 `new Date(ts * 1000).getHours()` 读取浏览器本地时区，`分布式链路` 散点图 `xAxis: { type: 'time' }` 未配置时区格式化器，而同屏下方明细表强制使用 `Asia/Shanghai`，在非东八区终端产生最高 15 小时时空撕裂；同时直方图缺 `barMaxWidth`、硬编码 `#409eff`/`#f56c6c`，快捷筛选 `<ElCheckTag>` 与 `32px` 工具栏高低差 `6px`，无数据时图表与表格重复渲染双空态 | [TrafficCallsPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/traffic/modules/TrafficCallsPanel.vue#L186-L267)、[TrafficTracesPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/traffic/modules/TrafficTracesPanel.vue#L598-L606)、[format.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/utils/format.ts#L1-L45) |
| **W02-2** | `A-03` | P1 | **后端谓词缺失（纠偏原审计仅前端过滤的假性修改）**：全局态势「安全治理摘要 -> 鉴权失败 Top IP / Top Key」混入 `fail_count = 0` 的正常流量记录并套用红色危险徽标；根因在于后端 ClickHouse `security_top_sql` 聚合 `security_agg_1h` 宽表时漏写 `HAVING sum(fail_count) > 0` | [olap.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/clickhouse/olap.rs#L1277-L1294)、[SecurityGovSummary.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/overview/modules/SecurityGovSummary.vue#L15-L60) |
| **W02-3** | `C-02` | P2 | **CSS Grid 拉伸根因纠偏**：`链路详情` 在单 Span 场景下左栏 `.ops-trace__waterfall` 被右栏 15 行属性面板通过 Grid 默认 `stretch` 强行拉高至 `420px+` 纯白空壳（并非原审计猜测的固定 `min-height`，且不能加 `max-height` 破坏右栏 `position: sticky`）；此外顶栏 Trace 摘要右偏缺少左锚点、Span 属性 `code.file.path` 裸露构建机绝对路径 `/home/runner/work/...` 并生硬断词、底部表头 `耗时(ms)` 与 `上游耗时 (ms)` 空格不一且渠道列过窄 | [traces/detail.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/traces/detail.vue#L3-L35)、[traces/detail.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/traces/detail.vue#L953-L973)、[traces/detail.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/traces/detail.vue#L1248-L1261) |

---

## 2. 第一性原理本质归因

1. **可观测墙钟同构性（公理 U2）**：同一控制面内的所有时间投影必须构成单射同构映射 $T_{\text{epoch}} \mapsto \text{WallClock}(TZ_{\text{display}})$。当表格经 `Intl.DateTimeFormat(..., { timeZone: 'Asia/Shanghai' })` 投影而紧邻的 ECharts 图表调用依赖宿主 OS 环境变量的 `Date.prototype.getHours()` 时，同一物理事件在同屏裂变为两个互不相交的墙钟坐标，彻底摧毁跨时区 SRE 的因果排障基准。
2. **前后端聚合语义闭环（AGENTS.md 铁律 #2）**：「鉴权失败 TopN」在领域数学上的定义域是 $\{x \mid \sum \text{fail\_count}(x) > 0\}$。在存储层（ClickHouse `security_agg_1h`）为了兼顾失败率分母而写入 `fail_count = 0, req_count > 0` 的行属于物理表设计，查询接口必须在 SQL `HAVING` 子句完成谓词收敛，否则不仅浪费序列化带宽，更会在 `LIMIT N` 截断下返回语义非法的正常实体。
3. **双栏非对称滚动拓扑守卫**：链路瀑布图行数跨度极大（1 个异步 Span 至数百个嵌套阶段 Span）。页面采用「左栏随文档流自然伸展 + 右栏 `position: sticky` 吸顶限高自滚」是标准 APM 拓扑；只需为左栏补齐 `align-self: start` 解除单行时的反向拉伸即可，若误加内层 `max-height` 反而会制造嵌套滚动条死区。

---

## 3. 详尽可落地技术方案

### 3.1 建立 ECharts 统一东八区时间轴原语并修复流量监控面板（`mazbri-web`）

1. **在 [format.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/utils/format.ts) 补齐自适应粒度墙钟轴函数**
   - 新增并导出纯函数 `formatAxisWallClock(epochMsOrSec: number, spanOrBucketSeconds?: number): string`，内部统一调用现有的 `wallClockParts(ms)`（固定绑定 `DISPLAY_TIME_ZONE = "Asia/Shanghai"`）：
     - `span >= 86400`：输出 `${g.month}-${g.day}`（或跨小时桶输出 `${g.month}-${g.day} ${g.hour}:${g.minute}`）；
     - `span < 60`：输出 `${g.hour}:${g.minute}:${g.second}`；
     - 默认分/小时级：输出 `${g.hour}:${g.minute}`。
   - 新增 Tooltip 完整墙钟函数 `formatTooltipWallClock(epochMsOrSec: number): string`（输出 `YYYY-MM-DD HH:mm:ss (UTC+8)`）。
   - 在公共时间选择器 [TimeRangePicker.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/ops/TimeRangePicker.vue) 旁侧固定展示轻量时区语义胶囊 `UTC+8`，向所有跨时区值班终端显式声明控制台墙钟基准。

2. **重构 `TrafficCallsPanel.vue` 与 `TrafficTracesPanel.vue`**
   - [TrafficCallsPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/traffic/modules/TrafficCallsPanel.vue)：
     - 删除第 186-195 行手写的 `bucketLabel` 本地 `Date` 拼接，全面替换为 `formatAxisWallClock(b.ts * 1000, data.bucket_seconds)`；
     - 为两个堆叠 `bar` series 补充 `barMaxWidth: 32`，并将硬编码 `#409eff` / `#f56c6c` 替换为 `tokens` 导出的主题语义色（读取 `--el-color-primary` / `--el-color-danger` 实时计算值）；
     - 当 `!histogramLoading && (!histogram || histogram.total === 0)` 时，自动折叠隐藏上方空图表 `OpsCard`（仅保留下方主表格区的单一空态），消除上下双空盒堆叠。
   - [TrafficTracesPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/traffic/modules/TrafficTracesPanel.vue)：
     - 在 `xAxis: { type: "time" }` 的 `axisLabel` 中挂载 `formatter: (val: number) => formatAxisWallClock(val, end - start)`，并在散点 Tooltip 中使用 `formatDateTime` 输出东八区时间，彻底对齐下方链路表。
   - 在 [CallMonitorTable.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/traffic/modules/CallMonitorTable.vue) 将左侧快捷筛选 `<ElCheckTag>` 统一设为 `height: 32px; display: inline-flex; align-items: center`，消除与右侧搜索栏的 `6px` 台阶差。
   - 全局排查其余所有 `views/mazbri/ops/**` 与 `components/ops/**` 中的 ECharts 时间轴（如 `MultiSeriesChart.vue`），确认 100% 经由 `DISPLAY_TIME_ZONE` 格式化。

### 3.2 后端 ClickHouse 安全治理 TopN 谓词收敛与前端防御（`mazbri-analytics` & `mazbri-web`）

1. **后端 SQL 收敛（[olap.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/clickhouse/olap.rs#L1277-L1294)）**
   - 将 `security_top_sql` 更新为：
     ```rust
     format!(
         "SELECT {dim} AS dim_value, sum(fail_count) AS fail_count, sum(req_count) AS req_count \
          FROM security_agg_1h WHERE {win} AND {dim} != '' \
          GROUP BY dim_value HAVING fail_count > 0 \
          ORDER BY fail_count DESC, req_count DESC LIMIT {limit}",
         limit = clamp_limit(p.limit, 20),
     )
     ```
   - 在 `olap.rs` 单元测试中断言生成的 SQL 包含 `HAVING fail_count > 0`。
2. **前端防御性守卫与空态语义（[SecurityGovSummary.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/overview/modules/SecurityGovSummary.vue)）**
   - 前端渲染层同步保留防御性过滤 `rows.filter((r) => r.fail_count > 0)`，并为计数徽标补齐 `tabular-nums`；空列表时展示正向无威胁状态文案（如 `无鉴权失败记录`）。

### 3.3 链路详情瀑布图弹性布局、路径脱敏美化与表头统一（`mazbri-web`）

1. **单行 CSS 修复瀑布图拉伸（[traces/detail.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/traces/detail.vue#L958)）**
   - 为 `.ops-trace__waterfall` 补充 `align-self: start;`（**严禁添加 `max-height` 或 `overflow: auto`**）：单 Span 链路时瀑布卡片自然贴合内容高度（约 `92px`），多 Span 长链路时维持整页滚动驱动右侧 `.ops-trace__panel` 粘性吸顶。
2. **顶栏锚点左对齐与属性长路径格式化**
   - 将 `OpsPage` 头部 `#actions` 内的单侧拥挤元信息拆分为：左侧标题区紧跟可复制 `Trace ID` 与根入口 Span 名称，右侧展示等宽统计指标组（开始时间、总耗时、上游线耗时、网关开销、跨度数、错误数）。
   - 新增源码路径展示纯函数 `formatCodeFilePath(raw: string): string`：剥离 CI 构建机工作目录前缀（匹配 `/^.*?\/(?:mazbri-(?:bin|libs|web)\/)/` 保留仓库相对路径如 `mazbri-libs/mazbri-billing/src/usage_outbox_worker.rs`），完整原始值保留在 `title` 悬停提示与复制值中；属性面板 `<dd>` 改用 `overflow-wrap: anywhere` 配合斜杠处优先软换行，消除单词腰斩。
   - 统一底部关联表词条 `耗时 (ms)` 与 `上游耗时 (ms)` 的空格规范，加宽 `渠道` 列 `minWidth: 180` 并为耗时列注入右对齐与 `tabular-nums`。

---

## 4. 二阶、三阶影响与红队边界守卫

| 风险场景与反例 | 连锁影响分析 | 防护与消解措施 |
| :--- | :--- | :--- |
| **反例 1：跨时区框选下钻（Brush Zoom）坐标回传偏差** | 若把时间戳先转成东八区字符串再反解析回 `Date` 计算框选区间，在北美/欧洲浏览器下会产生时区二次偏移导致下钻查空。 | [TrafficCallsPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/traffic/modules/TrafficCallsPanel.vue#L274-L302) 的 `onBrushEnd` 直接通过类目索引 `data.buckets[lo].ts` 读取原始 Unix 秒级时间戳，字符串仅用于视觉 `axisLabel`，框选数学计算完全在 UTC epoch 域闭合，零回传偏差。 |
| **反例 2：`TrafficTracesPanel` 使用 ECharts `type: 'time'` 时的刻度整点吸附** | ECharts `type: 'time'` 内部仍按浏览器本地时区决定“午夜 `00:00` 主刻度线”位置，仅改 `axisLabel.formatter` 在跨日窗口（如 7d）可能使刻度落在东八区 `15:00` 而非 `00:00`。 | 对 ≤24h 窗口 `axisLabel.formatter` 精确映射东八区 `HH:mm`；对 >24h 跨日窗口显式输出 `MM-DD HH:mm` 完整月日时分，确保任意刻度点读数绝对自洽无歧义。 |
| **反例 3：ClickHouse `HAVING fail_count > 0` 列别名兼容性** | ClickHouse SQL 中 `SELECT sum(fail_count) AS fail_count ... HAVING fail_count > 0` 会绑定投影别名，若表内存在同名列在不同 ClickHouse 版本可能产生歧义。 | SQL 显式写作 `HAVING sum(fail_count) > 0 ORDER BY fail_count DESC, req_count DESC`，消除任何列别名解析歧义。 |

---

## 5. 验收门禁与实测清单

1. **自动化单测必测项**
   - 在 `mazbri-web` 新增时区隔离单测（通过 `TZ=America/Denver` 或模拟跨时区 epoch 输入运行 `vitest`）：断言 `1795680010` 在 `formatDateTime` 与 `formatAxisWallClock` 下输出一致的 `16:00` 东八区墙钟，绝不随宿主 `TZ` 漂移。
   - 新增 `formatCodeFilePath` 单测：断言 `/home/runner/work/mazbri/mazbri/mazbri-libs/mazbri-billing/src/lib.rs` 净化为 `mazbri-libs/mazbri-billing/src/lib.rs`，第三方库路径安全回退不丢信息。
   - 运行 `mazbri-analytics` 全部单测与 Clippy 门禁，确认 `security_top_sql` 含 `HAVING sum(fail_count) > 0`。
2. **构建、热替换与实机核验**
   - 编译 `mazbri-web`、`mazbri-analytics` 与 `mazbri-mgmt`，依次热替换重启 `mazbri-mazbri-analytics-1` ➔ `mazbri-mazbri-mgmt-1`。
   - 浏览器通过 Chrome DevTools Emulation 将浏览器时区模拟切换为 `America/New_York`（`UTC-5`）与 `UTC`：访问 `/#/ops/traffic`（客户调用、上游调用、分布式链路三页签），验证图表 X 轴、Tooltip 与下方表格时间列 100% 同步在东八区 `UTC+8`；访问 `/#/dashboard` 验证安全治理摘要不再出现 `0` 次失败记录；访问单 Span 与 20+ Span 两条 `/#/ops/traces/:traceId` 链路，验证单 Span 无空白撑高、多 Span 右侧面板粘性吸顶正常。
