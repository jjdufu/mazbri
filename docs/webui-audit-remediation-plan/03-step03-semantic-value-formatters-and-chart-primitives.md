# Step 03：租户账务充退量纲同构、十进制微元原语、SLO 语义校准与分类柱图限宽

## 1. 覆盖问题清单与源码精确定位

| 编号 | 关联审计项 | 级别 | 问题描述与全栈契约定位 | 精确源码位置 |
| :--- | :---: | :---: | :--- | :--- |
| **W03-1** | `B-04`（扩展） | P0 | **租户账户充值/退款 100 倍分元量纲错位与小额退款锁死**：后端 `recharge_tenant_account` / `refund_tenant_account` 按**整数「分」（`i64` cents，`100 = ¥1.00`）**解析 `req.amount`，而前端 `account.vue` 输入框后缀渲染 `元` / `CNY` 且未乘 `100` 原样提交（输入 `100 元` 仅入账 `¥1.00`）；同时退款禁用守卫 `refundForm.amount > currentBalance` 直接拿「分」（默认 `100`）对比「元」（如余额 `10.00`），导致余额 `< ¥100.00` 的租户打开弹窗即锁死无法退款 | [account.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/tenants/%5Bcode%5D/tabs/account.vue#L211-L353)、[account.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/tenants/%5Bcode%5D/tabs/account.vue#L547-L556)、[account.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/tenants/%5Bcode%5D/tabs/account.vue#L660-L725)、[product.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/product.rs#L650-L689) |
| **W03-2** | `A-01` | P0 | 全局态势 `SLO 与错误预算` 表格在 `buildSloTableColumns` 中漏挂 `formatter` 裸露原始比率小数 `0.99`、`0.999` 与超长浮点数；首列无节制占据 `792px`，右侧 `短期快速消耗` / `长期缓慢消耗` 锁死 `width: 80` 在 `1920×1080` 下双双截断，且这两列在后端实为 `0/1` 布尔告警触发标志（非燃烧倍数） | [slo.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/utils/ops/slo.ts#L28-L39)、[slo.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/utils/ops/slo.ts#L116-L144)、[mod.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/slo/mod.rs#L420-L442) |
| **W03-3** | `B-12`, `B-05` | P1 | `账单结算 -> 经营概览` 公共组件 `CategoryBarChart` 缺 `barMaxWidth` 致单时间桶膨胀为 `420px` 色砖；财务 `StatCard` 缺币种单位、共用同一图标且硬编码 `:digits="2"`；`formatMoney` 锁死 2 位小数截断 `DECIMAL(18,6)` 微元单价（`BillListPanel`），而 `authorizations` 单价裸出 `"0.700000"` | [CategoryBarChart.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/ops/CategoryBarChart.vue#L135-L190)、[FinanceOverviewPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/billing/bills/FinanceOverviewPanel.vue#L31-L69)、[FinanceOverviewPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/billing/bills/FinanceOverviewPanel.vue#L206)、[BillListPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/billing/bills/BillListPanel.vue#L208)、[authorizations/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/operations/authorizations/index.vue#L55)、[costPrice.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/channels/costPrice.ts#L1-L41) |
| **W03-4** | `C-04`, `C-05` | P1/P2 | `运行指标 -> 应用节点` 实例健康表头写死 `CPU（核）`，而单元格 `formatCpuCoresDisplay` 在 `< 0.1` 核时自适应输出 `1.8 mCPU`（量纲相差 1000 倍），服务名缺少管理员下钻视觉线索，`ServiceHexagonGrid` 缺图例与紧凑咬合；子监控页整组 Exporter 无样本时连续平铺 4 个空图表框 | [HealthPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/services/HealthPanel.vue#L410-L470)、[ServiceHexagonGrid.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/mazbri/ServiceHexagonGrid.vue#L1-L150)、[OpsMetricChartGrid.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/ops/OpsMetricChartGrid.vue#L1-L24) |

---

## 2. 第一性原理本质归因

1. **账务输入/传输/校验单一量纲边界（公理 U2）**：当后端接口协议定义为整数「分」（`i64` cents）而账户余额返回十进制「元」（`"1737.81"`）时，前端视图层的表单模型、上限校验、余额扣减预览必须全部锚定在用户可见的「元」域内，仅在构造 API 请求 DTO 的边界纯函数 `yuanToCentsExact` 中做一次无损定点整数转换，严禁在业务组件内混存两套量纲变量。
2. **微元账本纯十进制字符串同构（`DECIMAL(18,6)`）**：平台计费支持最高 6 位小数的微元单价（如 `¥0.000015 / 次`）。经由 JS `Number` 与固定 2 位/4 位小数格式化会产生 IEEE-754 舍入误差或微额抹零。统一采用纯十进制字符串解析（整数千分位 + 保底 2 位小数 + 保留非零有效尾数至最高 6 位），可同时满足月度汇总账单的 `.00` 财务对齐与按次微额计费的零精度损失。
3. **布尔告警指标与物理自适应词头解耦**：`mazbri_slo_burn_page` / `mazbri_slo_burn_ticket` 的值域是离散集合 $\{0, 1\}$（告警是否触发），而非连续实数燃烧率；单元格自带 `mCPU / 核` 动态词头时，表头只能声明物理量名称 `CPU 用量`。

---

## 3. 详尽可落地技术方案

### 3.1 修复 `account.vue` 充退量纲错位并建立纯十进制货币格式化原语（`mazbri-web`）

1. **统一 `account.vue`「元」域表单模型与定点「分」换算（`W03-1`）**
   - 在 [format.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/utils/format.ts) 新增并导出纯函数 `yuanToCentsExact(input: string | number): number | null`（校验正数且小数位 `<= 2`，通过字符串拆分整数元与两位角分拼合为精确安全整数分，杜绝 `19.99 * 100 = 1998.9999999999998` 浮点误差）与 `centsToYuanString(cents: number): string`。
   - 重构 [account.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/tenants/%5Bcode%5D/tabs/account.vue)（注：严禁触碰 [usage-reconciliation/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/billing/usage-reconciliation/index.vue#L213-L301) 对账策略弹窗内的正常表单间距）：
     - `rechargeForm.amount` 与 `refundForm.amount` 明确定义为**「元」**（支持 `0.01` 最小步长，默认充值 `100` 元、默认退款 `Math.min(100, currentBalance)` 元，`currentBalance <= 0` 时禁用退款入口）；
     - 统一以 `yuanToCentsExact(refundForm.amount)` 与 `maxRefundCents`（基于 `account.balance` 字符串定点解析出的精确整数分）执行比较：`:confirm-disabled="refundCents === null || refundCents <= 0 || refundCents > maxRefundCents"`，修正 `balanceAfterRefund` 为 `(maxRefundCents - refundCents) / 100`；
     - 提交 `rechargeTenantAccount` 与 `refundTenantAccount` 时统一传入 `amount: rechargeCents` / `amount: refundCents`（整数分）。

2. **升级 `2..6` 位自适应纯十进制货币格式化器与全站账务展示（`W03-3`）**
   - 在 [format.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/utils/format.ts#L99-L107) 基于纯十进制字符串算法升级 `formatMoney(value, options?: { minDecimals?: number; maxDecimals?: number })`（默认 `minDecimals = 2, maxDecimals = 6`）：
     - `"5257"` ➔ `"5,257.00"`，`"2750.85"` ➔ `"2,750.85"`，`"0.700000"` ➔ `"0.70"`，`"0.000015"` ➔ `"0.000015"`；同步让 `channels/costPrice.ts` 复用同一底层实现。
   - 在 [StatCard.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/ops/StatCard.vue) 保持默认整数友好行为不变的前提下，支持传入格式化字符串 `value` 与 `unit`；更新 [FinanceOverviewPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/billing/bills/FinanceOverviewPanel.vue#L31-L69)：
     - 移除前三张财务卡上的 `:digits="2"` 数字截断，直接传入 `formatMoney(overview?.revenue_total)` 等字符串，补齐 `:unit="$t('common.currencies.cny')"` 与差异化图标（收入 `ri:money-cny-circle-line`、成本 `ri:shopping-bag-3-line`、毛利 `ri:funds-box-line`、毛利率 `ri:percent-line`）；
     - 确认 `FinanceOverviewPanel.vue:206`（`moneyCell`）、`useOpsChartHost.ts:159`（`amount` 图表轴）、`BillListPanel.vue:208`（账单明细单价）自动继承 `2..6` 位无损精度，并在 [authorizations/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/operations/authorizations/index.vue#L55) 将裸 `{{ row.unit_price }}` 替换为 `formatMoney(row.unit_price)` 且右对齐。

### 3.2 校准 `SLO 与错误预算` 比率/布尔语义与列宽分配（`W03-2`）

修改 [slo.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/utils/ops/slo.ts)：
- 保持 `formatBudgetRemaining(remaining)` 返回纯字符串 `"88.43%"`（保障 [SloBudgetPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/overview/modules/SloBudgetPanel.vue#L10-L15) 摘要文案插值安全）；
- 新增 `formatSloRatio(val: number | null | undefined)`（如 `0.99 -> "99.00%"`、`0.999 -> "99.90%"`、`1 -> "100.00%"`）与 `formatBurnRate(val)`（`0 -> "0x"`、`1.25 -> "1.25x"`）；
- 在 `buildSloTableColumns` 中完整挂载格式化器与对齐：
  - `name`：`minWidth: 180`（释放多余横向空白）；
  - `objective` / `sli`：右对齐 + `tabular-nums` + `formatSloRatio`；
  - `window_days`：右对齐 + `tabular-nums`；
  - `remaining`：`minWidth: 116`，右对齐 + `tabular-nums`，调用 `formatBudgetRemaining` 并在 `<= 0` 时通过 VNode 施加危险色；
  - `burn_5m`：`minWidth: 104`，右对齐 + `tabular-nums` + `formatBurnRate`；
  - **`burn_page` / `burn_ticket`**：由死锁 `width: 80` 提升为 `minWidth: 124`，按后端真实 `0 / 1` 布尔告警触发语义渲染状态（`> 0` 渲染危险态 `触发中` / `Firing`，`=== 0` 渲染次级态 `正常` / `Normal`）。

### 3.3 公共分类柱图限宽、运行指标量纲去倍率与空图表组折叠（`W03-3`, `W03-4`）

1. **`CategoryBarChart.vue` 限宽与响应式重绘依赖**
   - 在 [CategoryBarChart.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/ops/CategoryBarChart.vue#L135-L190) 新增可选 `barMaxWidth?: number` prop，series 默认注入 `barMaxWidth: props.barMaxWidth ?? (props.horizontal ? 20 : 32)` 与 `barGap: "20%"`，并将 `() => props.horizontal`、`() => props.barMaxWidth` 加入第 177 行的 `watch` 监听源数组。
2. **`HealthPanel.vue` 量纲校准、下钻线索与 `OpsMetricChartGrid` 空态合并**
   - 将 `zh.json` / `en.json` 中 `ops.services.columns.cpu` 更新为 **`"CPU 用量"`** / **`"CPU Usage"`**，为数值列声明右对齐与 `tabular-nums`。
   - 在 [HealthPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/services/HealthPanel.vue#L410-L470) 对 `isAdmin` 角色将服务名渲染为主色可交互链接态（附细箭头图标，绑定 `@click.stop="openDetail(row)"` 避免与整行 `@row-click` 重复跳转，`R_VIEWER` 渲染纯文本）；紧凑优化 `ServiceHexagonGrid.vue` 奇偶行交错排布并补充状态色图例。
   - 在 [OpsMetricChartGrid.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/ops/OpsMetricChartGrid.vue) 当 `!loading && charts.every(...)` 全部序列均无样本点时，折叠多宫格空框为单张紧凑 `<EmptyState compact>`（状态词：`当前时间窗无样本` / `No samples in range`）。

---

## 4. 二阶、三阶影响与边界守卫

| 边界场景 | 连锁风险分析 | 消解与防护设计 |
| :--- | :--- | :--- |
| **浮点乘法 `19.99 * 100` 误差导致少充 1 分钱** | JS IEEE-754 中 `Math.floor(19.99 * 100)` 等于 `1998` 而非 `1999`。 | `yuanToCentsExact` 完全基于正则校验与整数/小数段字符串补零拼接转为 `BigInt`/`Number`，零浮点运算误差。 |
| **`StatCard` 全局默认小数位污染整数卡片** | 全站 30+ 处监控计数卡依赖 `StatCard` 整数输出。 | 不改 `StatCard` 默认整数格式化逻辑，财务卡直接传入经 `formatMoney` 格式化后的千分位字符串。 |
| **`R_VIEWER` 点击服务名触发 `403`** | 单服务详情页仅对 `R_ADMIN` 开放。 | 服务名链接样式与点击下钻严格受 `isAdmin` 守卫，`R_VIEWER` 仅展示纯文本。 |

---

## 5. 验收门禁与实测清单

1. **自动化单测必测项**
   - `format.test.ts`：覆盖 `yuanToCentsExact("100") === 10000`、`yuanToCentsExact("19.99") === 1999`、`yuanToCentsExact("0.01") === 1`、`yuanToCentsExact("1.999") === null`；覆盖 `formatMoney` 对 `"5257" -> "5,257.00"`、`"0.700000" -> "0.70"`、`"0.000015" -> "0.000015"`。
   - `slo.test.ts`：断言 `buildSloTableColumns` 挂载完整格式化器，`0.999 -> "99.90%"`，`burn_page = 0 / 1` 映射为正常/触发态而非倍数。
2. **构建、热替换与 `9224` 实机核验**
   - 执行 `mazbri-web` `pnpm build && pnpm test` 与 `mazbri-bin -p mazbri-mgmt` 门禁，原子热替换重启 `mazbri-mazbri-mgmt-1`。
   - 连接 `127.0.0.1:9224` 验证：① 租户 `账户余额` 打开退款框输入 `20` 元（余额 `1,737.81` 元），预览准确显示 `1,717.81` 元且按钮可正常提交，余额 `< ¥100` 租户可正常退小额款项；② `/#/dashboard` SLO 表无原始小数、无表头截断；③ `/#/operations/bills` 柱宽适中、财务卡带 `元` 单位与精确小数；④ `/#/ops/runtime` 表头显示 `CPU 用量`。
