# Step 05：航空暗舱式全局态势首屏降噪、分层自适应 KPI 栅格与告警语义收敛

## 1. 覆盖问题清单与源码精确定位

| 编号 | 关联审计项 | 级别 | 问题描述与全栈契约定位 | 精确源码位置 |
| :--- | :---: | :---: | :--- | :--- |
| **W05-1** | `A-02` | P1 | 全局态势在零告警常态下，第 3 节 `活跃告警` 空卡片占据 `331px` 纵向视口，将核心 `业务 QPS` 与 `业务 P99` 流量质量双图推至 `top: 747px`（在 `1440×900` / `1280×800` 下挤出首屏），而页面底部的 `异常事件`（`RecentAnomalies.vue`）空态亦消耗 `273px` 高度；且 `活跃告警` 内部嵌有管理员专属 `<CertExpiryBadge>`，收敛高度时严禁丢失证书到期预警与引发自动轮询 CLS 跳动 | [overview/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/overview/index.vue#L167-L265)、[CertExpiryBadge.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/overview/modules/CertExpiryBadge.vue#L1-L26)、[RecentAnomalies.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/overview/modules/RecentAnomalies.vue#L1-L25) |
| **W05-2** | `A-04` | P2 | 全局态势首屏右上角自动刷新控件 `.ops-auto-refresh__trigger` 高度为 `30px`（与相邻 `32px` 时间窗分段器相差 `2px`），且跨页导航 `经营概览 ↗` 使用无边框裸链接，容器层级松散 | [overview/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/overview/index.vue#L18-L33)、[AutoRefresh.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/ops/AutoRefresh.vue#L180-L245) |
| **W05-3** | `A-05` | P2 | `OpsKpiRow` 基于 `ElRow` + `.el-col { margin-bottom: 16px }` 叠加父级间距形成双倍留白；第 2 排 `实时状态` 在 `R_VIEWER` 角色隐藏计费积压卡后仅剩 3 卡占 `75%` 宽（右侧悬空 `25%`），而第 1 排 `态势 KPI` 含 **6 张卡**（窄屏不可折成 `5 + 1` 孤卡）；此外 `overview/index.vue` 与 `ProductTopN.vue` 硬编码 `"zh-CN"` 数字格式，同页时延术语不一 | [OpsKpiRow.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/ops/OpsKpiRow.vue#L1-L16)、[ops-layout-tokens.scss](file:///workspace/rust-proj/mazbri/mazbri-web/src/assets/styles/custom/ops-layout-tokens.scss#L74-L81)、[overview/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/overview/index.vue#L33-L163)、[overview/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/overview/index.vue#L448)、[ProductTopN.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/overview/modules/ProductTopN.vue#L126) |
| **W05-4** | `C-03` | P2 | `告警中心 -> 告警事件` 同屏上下同时渲染截取前 10 条的 `<ElTimeline>` 卡片与完整分页 `<ArtTable>` 卡片；`告警规则` 与 `静默配置` 使用裸 `<ElTable>` 未接入 `<ArtTable>`，`新建规则` 缺失 `+` 图标，条件列将 `mazbri_slo_burn_page > 0.5`（布尔 0/1 指标）与 `for_seconds = 0` 拼接为晦涩的 `> 0.5 · 0秒` | [EventsPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/alerts/EventsPanel.vue#L38-L171)、[RulesPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/alerts/RulesPanel.vue#L3-L75)、[SilencesPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/alerts/SilencesPanel.vue#L18-L80) |

---

## 2. 第一性原理本质归因

1. **航空暗舱信噪比与零 CLS 布局稳定性（Dark Cockpit & Zero-CLS，公理 U3）**：系统在 99.9% 健康稳态下，不应以 `331px` 空白插画证明「无活跃告警」并挤走主飞行姿态区（QPS 与 P99 时延曲线）；同时状态摘要条必须保持恒定折叠高度，确保 30 秒自动轮询期间下方图表坐标零跳变。
2. **基数感知的等分栅格拓扑（Cardinality-Aware Grid）**：6 卡行与 3～4 卡行的约数结构不同——若无差别套用单一 `repeat(auto-fit, minmax(210px, 1fr))`，6 卡行在 `1440×900`（内容区 `1168px < 1340px`）下会退化为 `5 + 1` 孤卡折行，反而多吞噬一行高度。按可见卡片基数 `count` 驱动列轨道（6 卡在桌面 6 列、中屏对称 3×2；3～4 卡单行等分铺满 100%），才能在所有视口与角色下达成几何对称。
3. **基于显式元数据标签的阈值语义转译**：后端业务告警规则在 `labels.unit` 中已携带明确量纲（`"%"`、`"ms"`），而自动同步的 SLO 燃烧规则（`mazbri_slo_burn_page` / `mazbri_slo_burn_ticket`）属于布尔 `0/1` 指标（`labels.category === "slo"`）。格式化必须读取真实 `labels` 元数据，严禁凭 `0 < threshold <= 1` 数值区间盲猜百分比。

---

## 3. 详尽可落地技术方案

### 3.1 暗舱式全局态势首屏重编排与顶栏工具对齐（`mazbri-web`）

1. **固定高度告警态势摘要条与 `<CertExpiryBadge>` 无损保留（`W05-1`）**
   - 在 [overview/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/overview/index.vue#L167-L205) 将 `#active-alerts` 升级为紧凑态势横幅 `OpsAlertStatusBanner`（常态高度 `44px`）：
     - `firingAlerts.length === 0` 时渲染静默绿点状态词 `无活跃告警`，右侧保留 `<RoleGate gate="admin"><CertExpiryBadge :days="realtime?.cert_min_expiry_days" /></RoleGate>` 与轻量 `告警事件` 跳转按钮；
     - `firingAlerts.length > 0` 时渲染醒目危险/警告摘要条（展示触发数量与最高级别），支持按需展开明细抽屉或内联折叠层（轮询刷新不自动撑开改变主图纵坐标），使下方 `业务 QPS` 与 `业务 P99` 双主图在 `1440×900` 首屏直接完整露出。
   - 在 [RecentAnomalies.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/overview/modules/RecentAnomalies.vue) 无异常事件时渲染紧凑单行状态态（高度 `48px`），回收底部多余空白。
2. **首屏右上角 `32px` 严丝合缝对齐（`W05-2`）**
   - 在 [AutoRefresh.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/ops/AutoRefresh.vue#L180-L245) 将触发按钮与下拉箭头高度由 `30px` 校准为标准 `32px`。
   - 在 [overview/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/overview/index.vue#L18-L33) 将裸链接 `经营概览 ↗` 升级为 `32px` 描边次按钮 `<ElButton plain>`（附 `ri:external-link-line` 图标）。

### 3.2 重构基数感知 `OpsKpiRow` 原生栅格与图表固定量纲保护（`W05-3`）

1. **原生 CSS Grid 替代 `ElRow + ElCol` 负边距（严禁在非 scoped 全局 SCSS 中写无效 `:deep()`）**
   - 重构 [OpsKpiRow.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/ops/OpsKpiRow.vue) 渲染语义化 `<div class="ops-kpi-row" :data-cols="columns">`（支持可选 `columns?: 3 | 4 | 6`，未传时按子项数自动派生），并在 [ops-layout-tokens.scss](file:///workspace/rust-proj/mazbri/mazbri-web/src/assets/styles/custom/ops-layout-tokens.scss#L74-L81) 使用标准原生选择器清零内部 `.el-col` 历史包裹层的 `max-width`、`padding` 与 `margin-bottom`（同步在 `overview/index.vue` 与 `FinanceOverviewPanel.vue` 移除冗余 `<ElCol>` 包裹）：
     - `6` 卡模式（`#kpi`）：`>= 1280px` 单行 `repeat(6, minmax(0, 1fr))`，`< 1280px` 对称折为 `repeat(3, minmax(0, 1fr))`（`768px` 以下 `repeat(2, minmax(0, 1fr))`），彻底杜绝 `5 + 1` 孤卡；
     - `3` / `4` 卡模式（`#realtime`、`FinanceOverviewPanel`）：桌面端按实际可见卡片数 `repeat(auto-fit, minmax(200px, 1fr))` 自动等分铺满 100% 行宽（消除 `R_VIEWER` 右侧 25% 缺口与底部双倍 `32px` 间距）。
2. **统一时延术语、区域数字格式与 Y 轴量纲锁定**
   - 将 [overview/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/overview/index.vue#L448) 与 [ProductTopN.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/overview/modules/ProductTopN.vue#L126) 中硬编码的 `new Intl.NumberFormat("zh-CN")` 改为跟随当前激活 `locale.value`；统一同页网关侧术语为「时延」。
   - 在 [MultiSeriesChart.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/ops/MultiSeriesChart.vue) 与 [useOpsChartHost.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/composables/ops/useOpsChartHost.ts#L106-L151) 中，当图表标题或 `yAxis.name` 标注固定单位（如 `ms`）时，若省去刻度行内重复的 ` ms` 后缀，必须将刻度数值锁定在该同一单位量纲下（如 `1500` 保持输出 `"1,500"`，绝不在 `(ms)` 轴下进位输出无单位的 `"1.5"`）。

### 3.3 告警中心去重、`ArtTable` 底座统一与规则条件精准转译（`W05-4`）

1. **消除 `EventsPanel.vue` 同屏双重渲染并统一表格底座**
   - 修改 [EventsPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/alerts/EventsPanel.vue#L38-L171)：移除上方仅截取 `rows.slice(0, 10)` 且无分页/无下钻能力的原型冗余 `<ElTimeline>` 卡片，统一保留功能完整的 `<ArtTable>` 主视图与行选中联动的底部「关联变更」面板（保障翻页、链路跳转与变更下钻 100% 可达）。
   - 将 [RulesPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/alerts/RulesPanel.vue#L13) 与 [SilencesPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/alerts/SilencesPanel.vue#L18) 的裸 `<ElTable>` 迁移至 `<ArtTable>`，继承统一空态与右对齐数值列。
2. **补齐新建图标与元数据感知的 `formatRuleCondition`**
   - 在 `RulesPanel.vue` 为「新建规则」补齐前置 `ri:add-line` 图标，放宽条件列 `minWidth: 160`。
   - 提取纯函数 `formatRuleCondition(row: AlertRuleDto)`：
     - 持续时间：`row.for_seconds === 0` 输出 `立即触发` / `Immediate`，`> 0` 调用 `formatIntervalHuman`（如 `持续 2m` / `持续 5m`）；
     - 阈值与操作符：若 `row.expr.includes("mazbri_slo_burn_")`（SLO 快慢烧布尔指标），渲染为 `燃烧触发 · 立即触发`（绝不渲染 `> 0.5` 或误转 `50%`）；若 `row.labels?.unit` 存在（如 `"%"`、`"ms"`），输出 `${row.op} ${row.threshold}${row.labels.unit} · ${durationText}`。

---

## 4. 二阶、三阶影响与边界守卫

| 边界场景 | 连锁风险分析 | 消解与防护设计 |
| :--- | :--- | :--- |
| **`1440×900` 视口下首排 6 张 KPI 卡折行** | 若采用固定像素 `minmax(210px, 1fr)`，6 卡需 `1340px > 1168px`，会产生难看的 `5 + 1` 孤卡并撑高首屏。 | 6 卡模式显式绑定 `repeat(6, minmax(0, 1fr))`（`< 1280px` 对称 `repeat(3, 1fr)`），单行紧凑容纳全部 6 卡。 |
| **高负载时延突破 `1000ms` 时的刻度读数** | 若刻度在 `>= 1000ms` 自动转秒又剥离后缀 `s`，在 `(ms)` 标题下会把 `1.5s` 读成 `1.5ms`。 | 轴标题带静态单位 `(ms)` 时，刻度格式化强制禁用跨级进位（固定以毫秒标度输出）。 |

---

## 5. 验收门禁与实测清单

1. **自动化单测必测项**
   - 新增 `formatRuleCondition` 单测：① `expr = 'mazbri_slo_burn_page{...}', threshold = 0.5, for_seconds = 0` 输出布尔燃烧触发文案且不含 `50%`；② `labels = { unit: '%' }, threshold = 5, for_seconds = 120` 输出 `> 5% · 持续 2m`。
2. **构建、热替换与 `9224` 实机核验**
   - 执行 `mazbri-web` `pnpm build && pnpm test` 与 `mazbri-bin -p mazbri-mgmt` 门禁，原子热替换重启 `mazbri-mazbri-mgmt-1`。
   - 连接 `127.0.0.1:9224` 分别以 `R_ADMIN` 与 `R_VIEWER` 验证：`1440×900` 下 `/#/dashboard` 首屏完整呈现两排 KPI（6 卡单行 + `R_VIEWER` 3 卡 100% 等宽铺满）+ 紧凑态势条（含超管证书临期徽标）+ QPS/P99 双主图，右上角全部按钮严格 `32px` 等高；`/#/ops/alerts` 无同屏重复列表且 `SLO 快烧` 规则条件清晰准确。
