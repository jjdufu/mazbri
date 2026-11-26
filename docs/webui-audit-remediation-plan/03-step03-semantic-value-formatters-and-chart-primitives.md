# Step 03：带值域与亚分精度守卫的语义格式化器、分类柱图限宽与资源量纲校准

## 1. 覆盖问题清单与源码精确定位

| 编号 | 关联审计项 | 级别 | 问题描述与核验纠偏 | 精确源码位置 |
| :--- | :---: | :---: | :--- | :--- |
| **W03-1** | `A-01` | P0 | 全局态势 `SLO 与错误预算` 表格在 `buildSloTableColumns` 中漏传 `formatter`，直接裸露原始比率小数 `0.999` 与超长浮点数 `0.884259259...`（同文件第 141 行已实现 `formatBudgetRemaining` 却未调用）；且首列无上限吞噬 `780px` 空白，右侧 `短期快速燃烧` / `长期缓慢燃烧` 死锁 `width: 80` 导致 1920 宽屏下表头双双截断 | [slo.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/utils/ops/slo.ts#L28-L39)、[slo.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/utils/ops/slo.ts#L116-L144)、[SloBudgetPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/overview/modules/SloBudgetPanel.vue#L15-L35) |
| **W03-2** | `B-12` | P1 | **亚分精度红队守卫（纠偏原审计死锁 `.toFixed(2)` 的建议）**：`账单结算 -> 经营概览` 公共组件 `CategoryBarChart` 未限制 `barMaxWidth` 导致单日账期渲染出 `420px` 宽的巨型色砖；顶部财务 `StatCard` 缺失货币单位且小数位不一；同时 `用量对账` 页签内联筛选控件行高与外边距存在偏移 | [CategoryBarChart.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/ops/CategoryBarChart.vue#L135-L142)、[FinanceOverviewPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/billing/bills/FinanceOverviewPanel.vue#L31-L69)、[usage-reconciliation/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/billing/usage-reconciliation/index.vue#L238-L260) |
| **W03-3** | `C-04` | P1 | `运行指标 -> 应用节点` 右侧实例健康表第 3 列表头写死 `CPU (核)`，而单元格 `formatCpuCoresDisplay` 在 `< 1` 核时自适应输出 `20 mCPU`（毫核），表头与单元格量纲相差 1000 倍；左侧 `ServiceHexagonGrid` 实为普通矩形水平平铺未交错咬合；右侧服务名缺少可进入 `/#/ops/services/:service` 详情页的链接视觉线索 | [HealthPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/services/HealthPanel.vue#L430-L470)、[ServiceHexagonGrid.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/mazbri/ServiceHexagonGrid.vue#L120-L150) |
| **W03-4** | `C-05` | P2 | `运行指标` 子页签（中间件 / 数据与管道 / 主机容器）当某 Exporter 分组在当前时间窗无样本时，`OpsMetricChartGrid` 连续渲染多个等高空白图表框，缺乏分组级聚合空态提示 | [runtime/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/runtime/index.vue)、[OpsMetricChartGrid.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/ops/OpsMetricChartGrid.vue) |

---

## 2. 第一性原理本质归因

1. **物理量纲自携带原则（Self-Describing Quantities，公理 U2）**：当单元格数值跨越多个数量级并采用自适应词头（如 `mCPU` vs `Cores`、`MiB` vs `GiB`、`µs` vs `ms`）时，单位属于**值的一部分**而非列的静态常量。此时表头只能声明物理量名称（`CPU 用量`、`内存用量`），若在表头附加静态单位 `(核)`，必然在词头切换时构成千倍量纲冲突。
2. **网关计费双精度域同构（Ledger vs. Sub-cent Micro-billing）**：Mazbri 后端计费账本统一采用 `DECIMAL(18,6)` 微元精度，月度汇总账单以“分（2 位小数）”为主，而按次 API 调用单价与实时消费增量常处于 `¥0.0005`～`¥0.05` 亚分区间。统一货币格式化器必须采用**保底两位、保留有效亚分尾数至四位（或六位）**的自适应有效数字规则，兼顾财务报表整齐度与微额扣费真实性。
3. **几何视觉编码比例约束**：柱状图的视觉语义通过**高度比**传递数量关系；当类目轴仅有 1～2 个时间桶且未设 `barMaxWidth` 时，ECharts 按轴宽比例把柱宽放大十余倍，使观察者注意力从纵向高度比转移到巨大的二维色块面积上，造成视觉失真。

---

## 3. 详尽可落地技术方案

### 3.1 建立自适应精度货币与比率语义格式化函数（`mazbri-web`）

1. **在 [format.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/utils/format.ts) 扩展语义数值工具集**
   - 新增 `formatCurrencyAmount(amount: string | number | null | undefined, options?: { maxFractionDigits?: number; locale?: string }): string`：
     - 默认 `minimumFractionDigits: 2, maximumFractionDigits: options?.maxFractionDigits ?? 4`；
     - 对 `1807` 渲染为 `1,807.00`，对 `853.4` 渲染为 `853.40`，对亚分消费 `0.0045` 精确渲染为 `0.0045`（绝不抹零为 `0.00`）；读取当前激活语言环境或标准千分位格式。
   - 新增 `formatRatioPercent(ratio: number | null | undefined, digits = 2): string`：
     - 空值或非有限数返回 `—`；
     - 支持尾部冗余零智能收敛或固定精度（如 SLO 目标 `0.999` 渲染为 `99.90%`、`0.9995` 渲染为 `99.95%`，当前 SLI `1` 渲染为 `100.00%`）。

2. **修复 `SLO 与错误预算` 表格格式化与列宽平衡（`W03-1`）**
   - 修改 [slo.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/utils/ops/slo.ts)：
     - 新增行级感知格式化函数 `formatSloMetricValue(value: number | null | undefined, sliType: string): string`：当 `sliType` 属于达成率语义（`availability`、`latency`、`quality` 或 `0 <= value <= 1` 比率定义）时调用 `formatRatioPercent(value, 2)`；若未来扩展绝对数值型 SLI 则按原值与对应单位输出，防止出现 `15000%` 越界。
     - 在 `buildSloTableColumns` 中完整绑定：
       - `objective`：`align: "right"`, `headerAlign: "right"`, `formatter: (row) => formatSloMetricValue(row.objective, row.sli_type)`；
       - `window_days`：`align: "right"`, `headerAlign: "right"`；
       - `sli`：`align: "right"`, `headerAlign: "right"`, `formatter: (row) => formatSloMetricValue(row.sli, row.sli_type)`；
       - `remaining`：`minWidth: 120`, `align: "right"`, `headerAlign: "right"`, `formatter: (row) => formatBudgetRemaining(row.remaining)`；
       - `burn_5m`：`minWidth: 108`, `align: "right"`, `headerAlign: "right"`, `formatter: (row) => formatBurnRate(row.burn_5m)`（保留至多 2 位小数如 `0` 或 `1.25x`）；
       - `burn_page` / `burn_ticket`：将死锁的 `width: 80` 改为 `minWidth: 128`, `align: "right"`, `headerAlign: "right"`, `formatter: ...`；
       - `name`：设置合理 `minWidth: 180`，释放多余横向空间给右侧数值列。

### 3.2 公共柱状图限宽与财务概览/对账面板规范化（`W03-2`）

1. **收敛 `CategoryBarChart.vue` 柱宽上限**
   - 在 [CategoryBarChart.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/ops/CategoryBarChart.vue#L135-L141) 的每个 `bar` series 配置中默认注入 `barMaxWidth: props.horizontal ? 24 : 32` 与 `barGap: "20%"`，一处修改全局消除单时间桶巨柱失真。
2. **补齐财务 `StatCard` 币种单位、图标与精度**
   - 修改 [StatCard.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/ops/StatCard.vue) 与 [FinanceOverviewPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/billing/bills/FinanceOverviewPanel.vue)：
     - 支持在 `StatCard` 传入可选 `prefix`（如 `¥`）或 `unit`（如 `CNY` / `元`）与语义化 `icon`，并支持 `minDigits` / `maxDigits`（默认货币卡 `minDigits: 2, maxDigits: 4`）；
     - 为总收入、总成本、毛利、毛利率配置差异化财务图标与币种单位。
   - 在 [usage-reconciliation/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/billing/usage-reconciliation/index.vue) 重置内联筛选区 `.el-form-item { margin-bottom: 0 }` 并对齐 `32px` 控件高度。

### 3.3 校准运行指标表头量纲、服务下钻线索与子页聚合空态（`W03-3`, `W03-4`）

1. **修正 `HealthPanel.vue` 量纲与可发现性**
   - 将 `zh.json` / `en.json` 中 `ops.services.columns.cpu` 由 `"CPU (核)"` / `"CPU (cores)"` 更新为无固定倍率词头的 **`"CPU 用量"`** / **`"CPU Usage"`**；并为 `cpu_cores`、`memory_bytes`、`restarts_1h` 声明 `align: "right"` 与 `tabular-nums`。
   - 对管理员角色（`isAdmin`），在右侧实例健康概览表的 `服务` 列渲染可交互主色链接样式（悬浮下划线 + 右侧细箭头图标 `ri:arrow-right-up-line`），触发 `openDetail(row)` 直达 `/#/ops/services/:service`，同时保留整行选中过滤联动。
   - 优化 [ServiceHexagonGrid.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/mazbri/ServiceHexagonGrid.vue)：改为紧凑自适应网格或标准蜂窝奇偶行错位排布，移除卡片底部多余拉伸空白。
2. **`OpsMetricChartGrid.vue` 整组无样本紧凑折叠**
   - 在 [OpsMetricChartGrid.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/ops/OpsMetricChartGrid.vue) 计算当前面板组内全部图表序列是否均无有效样本点（`allEmpty && !anyLoading`）：若整组均为空，不再重复渲染 4～6 个等高空图表框，而是合并展示单张紧凑 `EmptyState` 横幅（说明当前时间窗未采集到该分组指标），将纵向无效占位压缩 75%。

---

## 4. 二阶、三阶影响与红队边界守卫

| 风险场景与反例 | 连锁影响分析 | 防护与消解措施 |
| :--- | :--- | :--- |
| **反例 1：横向条形图（`horizontal = true`）被纵向 `barMaxWidth: 32` 撑得过粗** | `CategoryBarChart` 同时服务于纵向时间趋势图与横向 TopN 排行条形图，横向条形图行距较密，`32px` 柱高会显得拥挤。 | 按 `props.horizontal` 区分默认上限：纵向柱图 `barMaxWidth: 32`，横向条形图 `barMaxWidth: 20`，并允许调用方通过可选 `barMaxWidth` prop 局部覆盖。 |
| **反例 2：`R_VIEWER` 角色点击运行指标服务名链接触发 `/#/403`** | [HealthPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/services/HealthPanel.vue#L466-L469) 中单服务详情路由仅对 `isAdmin` 开放。 | 服务名链接样式与跳转点击事件严格受 `v-if="isAdmin"` 门控：管理员展示可下钻链接态，只读观察员渲染纯文本标签，恪守三角色零越权 `403` 规范。 |
| **反例 3：SLO 剩余预算为负数（预算已耗尽透支）时的展示** | 当错误率严重超标时，`error_budget_remaining` 可能 `< 0`（或被后端钳制为 `0`）。 | `formatBudgetRemaining` 对 `<= 0` 值统一高亮危险色（`var(--el-color-danger)`），清晰警示错误预算耗尽。 |

---

## 5. 验收门禁与实测清单

1. **自动化单测必测项**
   - 在 `mazbri-web` 为 `slo.ts` 新增单测：断言 `buildSloTableColumns` 返回的目标、当前 SLI、剩余预算列均挂载有效 `formatter`，输入 `0.884259259259` 输出 `"88.43%"`，输入 `0.999` 输出 `"99.90%"`。
   - 为 `formatCurrencyAmount` 新增单测：覆盖 `1807 -> "1,807.00"`、`853.4 -> "853.40"`、`0.0045 -> "0.0045"`、`null -> "—"`。
2. **构建、热替换与实机核验**
   - 执行 `mazbri-web` `pnpm build && pnpm test` 与 `mazbri-bin -p mazbri-mgmt` Clippy 及测试，热替换重启 `mazbri-mazbri-mgmt-1`。
   - 浏览器在 `1920×1080` 与 `1280×800` 下巡检：① `/#/dashboard` 底部 SLO 表无原始长小数、无表头省略号截断；② `/#/operations/bills` 单日柱状图宽度克制美观、财务卡片带币种单位与精确小数；③ `/#/ops/runtime` 表头显示 `CPU 用量`，`R_ADMIN` 可直观识别并进入单服务详情而 `R_VIEWER` 不展示越权链接。
