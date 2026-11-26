# Step 05：航空暗舱式全局态势首屏降噪、KPI 自适应栅格与告警中心视图收敛

## 1. 覆盖问题清单与源码精确定位

| 编号 | 关联审计项 | 级别 | 问题描述与核验纠偏 | 精确源码位置 |
| :--- | :---: | :---: | :--- | :--- |
| **W05-1** | `A-02` | P1 | **防 CLS 布局跳动守卫**：当网关处于零告警、零异常检测的健康常态时，`需要关注` 与 `近期异常检测` 渲染两块高约 `280px` 的巨幅插画空卡片，合计吞噬近 `580px` 纵向视口，将核心请求量/错误率与时延分位曲线挤出首屏；且在自动轮询刷新场景下，若简单在 `44px` 与 `280px` 间直接伸缩，突发告警会导致下方正在框选图表的 SRE 发生剧烈画布跳动误触 | [overview/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/overview/index.vue#L189-L245)、[RecentAnomalies.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/overview/modules/RecentAnomalies.vue#L1-L25)、[ops-layout-tokens.scss](file:///workspace/rust-proj/mazbri/mazbri-web/src/assets/styles/custom/ops-layout-tokens.scss#L16-L17) |
| **W05-2** | `A-04` | P1 | 全局态势 `产品 TopN` 卡片头部 `ElSegmented`（`28px`）与相邻 `ElSelect`（`32px`）产生 `4px` 边缘错位；首屏右上角跨页导航 `经营概览 ↗` 使用无边框裸链接夹在时间窗胶囊与刷新按钮之间，交互层级松散 | [ProductTopN.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/overview/modules/ProductTopN.vue#L9-L40)、[overview/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/overview/index.vue#L18-L35) |
| **W05-3** | `A-05` | P2 | `.ops-kpi-row > .el-col` 无条件设置 `margin-bottom: 16px`，与父容器 `.ops-page-stack` 的 `gap: 16px` 叠加形成 `32px` 双倍断层；`R_VIEWER` 角色隐藏财务 KPI 卡后剩余 3 卡仅占 `75%` 宽度导致右侧悬空 `25%`；同页时延术语混用三种叫法，Y 轴逐行重复渲染 `.00 ms` 冗余精度，且 `fmt()` 硬编码 `"zh-CN"` | [ops-layout-tokens.scss](file:///workspace/rust-proj/mazbri/mazbri-web/src/assets/styles/custom/ops-layout-tokens.scss#L23-L35)、[overview/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/overview/index.vue#L170-L188)、[MultiSeriesChart.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/ops/MultiSeriesChart.vue#L188-L211) |
| **W05-4** | `C-03` | P2 | `告警中心 -> 告警事件` 在同一屏内上下同时渲染 `<ElTimeline>` 卡片与 `<ArtTable>` 卡片，完整重复展示同一批告警记录；`告警规则` 条件列将瞬时触发渲染为晦涩的 `> 0.5 · 0秒`；左上角 `新建规则` 缺失全站统一的 `+` 前置图标 | [EventsPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/alerts/EventsPanel.vue#L38-L76)、[RulesPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/alerts/RulesPanel.vue#L3-L10)、[RulesPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/alerts/RulesPanel.vue#L63-L71) |

---

## 2. 第一性原理本质归因

1. **航空暗舱信噪比与布局稳定性公理（Dark Cockpit & Zero-CLS，公理 U3）**：监控首屏的纵向黄金像素属于稀缺注意力带宽。在系统 99.9% 时间所处的健康稳态下，用大尺寸空白插画证明“没有坏事发生”是典型的负信息密度设计；同时，主飞行姿态区（核心 QPS、错误率、P95/P99 时延趋势）的屏幕纵坐标必须保持稳定，不能受偶发次级面板高度突变干扰。
2. **一维/二维流式栅格自洽性**：`ElRow` + `ElCol :span="6"` 属于静态 24 栅格分片，既需要靠子项负边距补偿折行间距（引发底部多余 `16px` 叠加），又无法在 RBAC 动态裁剪列数（4 卡变 3 卡）时自动重算等分权重。原生 CSS Grid `repeat(auto-fit, minmax(220px, 1fr))` 在数学上天然具备“列数自适应铺满 100% + 仅行列间产生 `gap`、外缘零冗余边距”的性质。

---

## 3. 详尽可落地技术方案

### 3.1 暗舱式全局态势首屏重编排与紧凑状态条（`mazbri-web`）

1. **核心黄金指标置顶与固定高度状态摘要条（`W05-1`）**
   - 在 [overview/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/overview/index.vue) 调整首屏信息流优先级：
     - 第 1 层：`实时状态` KPI 卡片行 + 顶栏时间窗工具簇；
     - 第 2 层：固定 `44px` 高度的**告警态势状态条（`OpsAlertStatusBanner`）**——当 `firingCount === 0` 时呈现静默绿点状态条（`● 无活跃告警` + 右侧轻量 `历史告警` 入口）；当 `firingCount > 0` 时转为醒目琥珀/红色摘要条（`● 活跃告警 {n} 项 · 最高级别 {severity}` + 默认内联展示 Top 3 紧凑单行或一键展开抽屉/折叠层，不自动将主图顶出首屏）；
     - 第 3 层：直接紧接核心双主图（左：`请求量与错误率`，右：`网关时延分位 (ms)`），确保在 `1920×1080` 与 `1440×900` 视口下无需滚动即可完整观测流量与时延曲线；
     - 第 4 层：`RecentAnomalies.vue` 在无异常事件时移除 `--ops-card-min-height: 180px` 约束，渲染单行紧凑态（`height: 48px`，状态词 `无异常事件`），彻底回收 `~500px` 首屏留白。

2. **对齐 `ProductTopN` 与顶栏工具簇 `32px` 高度规范（`W05-2`）**
   - 在 [ProductTopN.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/overview/modules/ProductTopN.vue#L10-L25) 为 `<ElSegmented>` 显式设置 `size="default"` 并通过局部样式对齐 `--el-component-size: 32px`，消除与旁侧 `<ElSelect>` 的 `4px` 高度差。
   - 在 [overview/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/overview/index.vue#L18-L35) 将裸文字链接 `经营概览 ↗` 替换为 `32px` 标准描边次按钮 `<ElButton plain>`（附 `ri:external-link-line` 后置图标），与时间窗选择器及刷新按钮保持统一容器高度。

### 3.2 重构 `.ops-kpi-row` 自适应 CSS Grid 与图表刻度降噪（`W05-3`）

1. **CSS Grid 消除双倍间距与 `R_VIEWER` 空缺**
   - 修改 [ops-layout-tokens.scss](file:///workspace/rust-proj/mazbri/mazbri-web/src/assets/styles/custom/ops-layout-tokens.scss#L23-L35) 与 `<OpsKpiRow>` 组件：
     - 将 `.ops-kpi-row` 定义为 `display: grid; grid-template-columns: repeat(auto-fit, minmax(210px, 1fr)); gap: var(--ops-kpi-gutter, 16px);`；
     - 若内部子元素仍包裹 `<ElCol>`，在 `.ops-kpi-row > .el-col` 上覆盖 `max-width: 100%; flex: none; width: 100%; padding: 0 !important; margin-bottom: 0 !important;`（或渐进移除外层冗余 `ElCol`），使 4 卡（`R_ADMIN` / `R_FINANCE`）与 3 卡（`R_VIEWER`）均自动等宽撑满 100% 行宽，且与下方区块严格保持单一 `16px` 间距。
2. **统一时延术语与精简 Y 轴刻度**
   - 统一 `zh.json` 中同页混用的「平均延迟 / 时延 / 耗时」网关侧术语为「时延」；修改 [overview/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/overview/index.vue#L315) 中硬编码 `"zh-CN"` 的 `fmt()` 读取当前 `locale.value`。
   - 在 [MultiSeriesChart.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/ops/MultiSeriesChart.vue#L188-L211) 优化 Y 轴 `axisLabel.formatter`：当卡片标题或 `yAxis.name` 已标明单位 `(ms)` 时，刻度标签不再逐行重复拼接 ` ms` 后缀，且对整数刻度去除无效 `.00` 尾数（仅在 `< 10` 小值区间按需保留小数），释放绘图区横向空间。

### 3.3 告警中心同屏去重与规则条件可读性转译（`W05-4`）

1. **消除 `EventsPanel.vue` 双重列表堆叠**
   - 修改 [EventsPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/alerts/EventsPanel.vue#L35-L76)：在顶部 `OpsPanelToolbar` 右侧提供视图分段器 `<ElSegmented v-model="viewMode" :options="[{ label: t('ops.alerts.viewTable'), value: 'table' }, { label: t('ops.alerts.viewTimeline'), value: 'timeline' }]" />`（默认 `'table'` 表格主视图，切换 `'timeline'` 时展示时间线视图），消除同屏上下渲染两份相同数据的冗余。
2. **人性化格式化告警规则条件与补齐图标**
   - 在 [RulesPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/alerts/RulesPanel.vue#L4-L71)：
     - 为「新建规则」按钮补齐 `<ArtSvgIcon icon="ri:add-line" />` 前置图标；
     - 提取纯函数 `formatRuleCondition(row)`：当 `row.for_seconds === 0` 时转译为 `立即触发` / `Immediate`（非零值使用 `formatIntervalHuman` 输出如 `持续 5m`）；当指标类型为错误率比率（`0 < threshold <= 1` 且表达式属比率类）时在旁侧或主值清晰展示百分比语义，并将条件列放宽至 `minWidth: 156`。

---

## 4. 二阶、三阶影响与红队边界守卫

| 风险场景与反例 | 连锁影响分析 | 防护与消解措施 |
| :--- | :--- | :--- |
| **反例 1：全站其他复用 `.ops-kpi-row` 的页面（如 `FinanceOverviewPanel`）栅格兼容性** | `FinanceOverviewPanel` 等多处页面在 `<OpsKpiRow>` 内包裹了 `<ElCol :xs="12" :lg="6">`，直接改父容器为 CSS Grid 时，若不覆盖 `.el-col` 的 `max-width: 25%`，卡片宽度会被压缩成 1/4 × 1/4 = 1/16！ | 在 `.ops-kpi-row` 样式块内显式声明 `:deep(> .el-col) { max-width: 100% !important; width: 100% !important; padding-left: 0 !important; padding-right: 0 !important; margin-bottom: 0 !important; }` 并清零 `ElRow` 负左右 `margin`，兼容存量 `<ElCol>` 结构零崩坏。 |
| **反例 2：网关时延极低（如 `0.45 ms`）时去 `.00` 导致 Y 轴刻度重复显示 `0`、`1`** | 当全部数据点在 `0 ~ 1.5 ms` 之间时，若粗暴 `Math.round` 会导致多个刻度同为 `0` 或 `1`。 | 采用有效小数格式 `Number(v.toFixed(2)).toString()`：`250.00 -> "250"`，`0.50 -> "0.5"`，`0.25 -> "0.25"`，兼顾整数清爽与亚毫秒精度。 |

---

## 5. 验收门禁与实测清单

1. **自动化单测必测项**
   - 新增 `formatRuleCondition` 单测：覆盖 `for_seconds = 0`（输出含 `立即触发` / `Immediate`）与 `for_seconds = 300` 分支。
   - 新增 Y 轴数值紧凑格式化单测：验证 `0`、`0.25`、`10.50`、`250.00` 的去冗余零输出。
2. **构建、热替换与实机核验**
   - 执行 `mazbri-web` `pnpm build && pnpm test` 与 `mazbri-bin -p mazbri-mgmt` 门禁，热替换重启 `mazbri-mazbri-mgmt-1`。
   - 浏览器分别以 `R_ADMIN` 与 `R_VIEWER` 登录访问 `/#/dashboard`（`1440×900` 与 `1920×1080`）：验证首屏直接完整呈现 KPI 行 + 流量趋势图 + 时延分位图，`R_VIEWER` 3 张 KPI 卡等宽铺满无右侧 25% 缺口，KPI 下方间距严格为 `16px`，`ProductTopN` 头部控件严丝合缝 `32px` 等高；访问 `/#/ops/alerts` 验证表格与时间线单视图切换无重复渲染。
