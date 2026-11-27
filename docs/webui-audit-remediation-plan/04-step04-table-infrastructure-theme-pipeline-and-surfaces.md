# Step 04：表格自适应列宽解锁、分页契约闭环、唯一同步主题内核与行内降噪

## 1. 覆盖问题清单与源码精确定位

| 编号 | 关联审计项 | 级别 | 问题描述与全栈契约定位 | 精确源码位置 |
| :--- | :---: | :---: | :--- | :--- |
| **W04-1** | `B-07`, `B-01`, `D-01` | P1 | **自适应列宽启发式硬钳制**：[useAdaptiveTableColumns.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/composables/useAdaptiveTableColumns.ts#L221-L301) 对所有 `*_code`、`*_enabled`、`*status*` 列硬编码 `defaultMaxForKey = 110`，并在 `resolveEffectiveMin` 中执行 `Math.min(floored, effectiveMax)` 强行把业务页声明的 `minWidth` 压回 **`110px`**，导致 `1920×1080` 宽屏下供应商编码（`UPUPT-ENV-SC-SM4`）与租户编码全部被卡在 `110px` 截断，而旁侧无上限状态列空占 `634px`～`726px`；用户列表短枚举列均摊留白挤压邮箱列 | [useAdaptiveTableColumns.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/composables/useAdaptiveTableColumns.ts#L220-L305)、[suppliers/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/suppliers/index.vue#L186-L195)、[tenants/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/tenants/index.vue#L50-L54)、[callbacks/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/operations/callbacks/index.vue#L80-L85)、[users/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/users/index.vue#L258-L300) |
| **W04-2** | `G-01` | P1 | `ResourceTablePageShell` 移除卡片头计数后约定底部分页栏承担总条数展示，但 `ArtTable` 剥离了 `"total"`，且中间层 `ResourceTableShell` 声明了 `showTotal?: boolean` 却漏向 `<ArtTable>` 透传 `:show-total="props.showTotal"` | [constants.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/tables/art-table/constants.ts#L12-L25)、[art-table/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/tables/art-table/index.vue#L70-L145)、[art-table/types.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/tables/art-table/types.ts#L17-L25)、[resource-table-shell/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/common/resource-table-shell/index.vue#L67-L86) |
| **W04-3** | `G-04` | P1 | **双轨主题管线分裂**：`themeStore.applyThemeToDom()` 同步维护 `<html class="dark" data-dark-theme="obsidian">` 与衍生变量，而主控台顶栏调用的旧管线 `useTheme().switchThemeStyles()` 独立重写了一套包裹在异步 `requestAnimationFrame` 中的 DOM 操作——**实机切暗色后直接把 `data-dark-theme` 抹为 `null`**、不清理 `"light"` 类（残留 `<html class="light dark">`），且 `el-ui.scss:4` 误写 `--el-color-black: white !important;`、`colors.ts:148` 暗色下仍与 `#ffffff` 混色 | [useTheme.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/hooks/core/useTheme.ts#L9-L152)、[theme.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/store/modules/theme.ts#L63-L88)、[colors.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/utils/ui/colors.ts#L141-L151)、[el-ui.scss](file:///workspace/rust-proj/mazbri/mazbri-web/src/assets/styles/core/el-ui.scss#L1-L5) |
| **W04-4** | `B-01`, `B-11` | P1/P2 | `el-ui.scss:763-855` 用 `!important` 将表格行内 `.el-button.is-link` 强行覆写为实底彩色胶囊且漏写 `.is-disabled` 衰减；全局 `.el-button--default.is-disabled` 字色为 `#475569`、`opacity: 1` 无置灰反馈，另有 4 个页面直用 `TableExportDropdown` 漏传空表 `:disabled`；`ArtTable` 默认 `#empty` 未接入 `<EmptyState compact>` | [el-ui.scss](file:///workspace/rust-proj/mazbri/mazbri-web/src/assets/styles/core/el-ui.scss#L52-L68)、[el-ui.scss](file:///workspace/rust-proj/mazbri/mazbri-web/src/assets/styles/core/el-ui.scss#L763-L855)、[art-table/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/tables/art-table/index.vue#L63-L66) |
| **W04-5** | `G-05`, `D-02` | P1/P2 | 全站 50+ 张表的金额/数量/耗时列缺少统一的声明式右对齐与等宽数字机制；`certificates/index.vue` 与 `CertificateDetailDrawer.vue` 指纹复制手写独立按钮，未复用 `<CopyableText>` | [useTableComposable.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/tables/art-table/useTableComposable.ts#L88-L95)、[certificates/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/certificates/index.vue#L74-L95)、[CertificateDetailDrawer.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/certificates/modules/CertificateDetailDrawer.vue#L136-L147) |

---

## 2. 第一性原理本质归因

1. **显式契约优先于启发式猜测（Explicit Contract over Heuristics，公理 U3）**：自适应列宽辅助函数的职责是在未指定宽度时提供兜底测量，绝不能用基于字段名后缀（`endsWith("_code")`）的 `110px` 死上限去反向腰斩业务列显式声明的 `minWidth`。同时，保持 `ListQueryBar` 既有的 `240px × 32px` `[不可更改]` 固定展开规范，才能确保窄屏工具栏右侧动作簇永不因聚焦动画发生折行抖动。
2. **单一同步主题投影函数（Single Synchronous Theme Projection）**：DOM 根节点的类名（`dark` / `light`）、属性（`data-dark-theme="obsidian"`）与 Element Plus 衍生 CSS 变量必须由同一个同步幂等函数 `themeStore.applyThemeToDom()` 原子写入。`useTheme()` 仅作为附加过渡动画控制的外观门面委托调用 `themeStore`，即可根除双管线状态漂移。

---

## 3. 详尽可落地技术方案

### 3.1 解锁 `useAdaptiveTableColumns` 列宽钳制并落地声明式数值列（`mazbri-web`）

1. **修复列宽上限钳制与各主表列宽平衡（`W04-1`）**
   - 修改 [useAdaptiveTableColumns.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/composables/useAdaptiveTableColumns.ts#L220-L305)：
     - 从 `defaultMaxForKey` 中移除 `key === "code" || key.endsWith("_code")` 的 `110px` 窄上限（提升业务编码默认上限至 `240px`）；
     - 在 `resolveEffectiveMin` 中，当业务列显式传入 `column.width` 或 `column.minWidth` 时，严格保证 `effectiveMax >= declaredMin`（禁止未显式传 `maxWidth` 的猜测值向下吞没 `minWidth`）。
   - 校准主表列宽与截断：
     - [suppliers/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/suppliers/index.vue)：`supplier_code` 设 `minWidth: 200`，为短枚举 `status` 列设定固定 `width: 110`（释放 `634px` 冗余留白给编码、名称与地址列）；
     - [tenants/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/tenants/index.vue) 与 [callbacks/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/operations/callbacks/index.vue#L80-L85)：放宽 `tenant_code`（`minWidth: 140`）与 `tenant_id`（`minWidth: 190`），移除 18 字符可读账号上的 `compact` 中间省略，为 `status` 列设固定 `width: 104`；
     - [users/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/users/index.vue)：为 `status`、`mfa_enabled`、`mfa_required` 设精确 `width: 100`～`112`，将 `email` 提升至 `minWidth: 220`。

2. **声明式 `numeric: true` 列属性与指纹复制统一（`W04-5`）**
   - 在 [types/component/index.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/types/component/index.ts#L75-L100) 的 `ColumnOption` 新增 `numeric?: boolean`。
   - 在 [useTableComposable.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/tables/art-table/useTableComposable.ts#L88-L95) 的 `cleanColumnProps(col)` 中：若 `col.numeric === true`，执行 `delete columnProps.numeric`，注入 `columnProps.align = col.align ?? "right"`、`columnProps.headerAlign = col.headerAlign ?? "right"`，并合并 `columnProps.className = [col.className, "tabular-nums"].filter(Boolean).join(" ")`。
   - 为全站核心表（产品排行、租户流水、产品授权单价、通道成本价与限流、账单列表、对账差异、调用监控耗时与重试列）批量标注 `numeric: true`。
   - 将 [certificates/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/certificates/index.vue#L74-L95) 与 [CertificateDetailDrawer.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/certificates/modules/CertificateDetailDrawer.vue#L136-L147) 中手写的指纹复制按钮统一替换为 `<CopyableText>`。

### 3.2 打通底部分页 `total` 透传链路与 `<EmptyState compact>`（`W04-2`, `W04-4`）

1. **闭环 `ResourceTableShell` ➔ `ArtTable` 总条数展示**
   - 在 [art-table/types.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/tables/art-table/types.ts#L17-L25) 声明 `showTotal?: boolean`（默认 `true`），并在 [resource-table-shell/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/common/resource-table-shell/index.vue#L67-L86) 向 `<ArtTable>` 补传 `:show-total="props.showTotal"`。
   - 在 `PaginationConfig` 支持可选 `queryMode?: "full" | "latest"`：在 [art-table/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/tables/art-table/index.vue#L75-L87) `.pagination-bar` 左侧当 `props.showTotal !== false` 时渲染 `<span class="pagination-total tabular-nums">`——常规精确分页渲染 `共 {total} 条` / `Total {total}`；当调用监控传入后端 `query_mode === "latest"` 且触顶 `universe_size`（`10,000`）时渲染 `共 10,000+ 条` / `Total 10,000+`（并在 `CallMonitorTable.vue` 开启总数展示）。
2. **统一默认空态与导出禁用态**
   - 将 [art-table/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/tables/art-table/index.vue#L63-L66) 默认 `#empty` 升级为 `<EmptyState compact :description="emptyText" />`。
   - 为直用 `<TableExportDropdown>` 的 [audit-logs/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/audit-logs/index.vue#L40-L44)、[CallMonitorTable.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/traffic/modules/CallMonitorTable.vue#L42-L49)、[user-routes/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/operations/user-routes/index.vue#L29) 与 [whitelist.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/tenants/%5Bcode%5D/tabs/whitelist.vue#L13) 补齐空数据禁用条件 `:disabled="exporting || rows.length === 0"`。

### 3.3 收敛唯一同步主题内核与行内/禁用按钮令牌（`W04-3`, `W04-4`）

1. **收口 `useTheme.ts` 至 `themeStore.applyThemeToDom()`**
   - 修改 [colors.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/utils/ui/colors.ts#L141-L151)：暗色模式下 `--el-color-primary-custom-1..15` 改为与深色基底 `#141414` 混色（替代无条件与 `#ffffff` 混色）；修正 [el-ui.scss](file:///workspace/rust-proj/mazbri/mazbri-web/src/assets/styles/core/el-ui.scss#L4) 中错误的 `--el-color-black: white !important;` 为 `#000000`。
   - 重构 [useTheme.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/hooks/core/useTheme.ts)：删除内部重复且漏设 `data-dark-theme="obsidian"` 与残留 `"light"` 类的异步 `requestAnimationFrame` 色阶循环，令 `setSystemTheme` / `switchThemeStyles` 直接同步调用 `settingStore.setGlopTheme(theme, themeMode)` + `settingStore.applyToDom()`（仅用 `disableTransitions` / `requestAnimationFrame(enableTransitions)` 包裹 CSS 过渡抑制），并将 `prefersDark` 监听改为常驻生效；`WorkbenchTopBar.vue` 同步调用 `useTheme().switchThemeStyles(next)`，实现全站唯一同步主题管线。
2. **行内链接按钮降噪与默认按钮禁用对比度加固**
   - 修改 [el-ui.scss](file:///workspace/rust-proj/mazbri/mazbri-web/src/assets/styles/core/el-ui.scss#L763-L855)：移除 `.el-table .el-button.is-link` 常态下的高饱和实底与描边，恢复透明背景文字链接态（仅 `:hover:not(.is-disabled)` 浮现柔和底纹），显式补齐 `&.is-disabled` 置灰与 `cursor: not-allowed`；在 `TableActionRow` 中为行尾高危 `删除` 拉开间距或轻量竖线分隔。
   - 在 [el-ui.scss](file:///workspace/rust-proj/mazbri/mazbri-web/src/assets/styles/core/el-ui.scss#L52-L68) 为 `.el-button--default.is-disabled:not(.is-link):not(.is-text)` 补齐 `background-color: var(--el-fill-color-light) !important; border-color: var(--el-border-color-lighter) !important; color: var(--el-text-color-disabled) !important; opacity: 0.65;`，使空表时禁用的「导出」按钮一目了然。

---

## 4. 二阶、三阶影响与边界守卫

| 边界场景 | 连锁风险分析 | 消解与防护设计 |
| :--- | :--- | :--- |
| **Node 原生测试环境无浏览器 `document` 对象** | `pnpm test` 运行于 Node `tsx`（无 `jsdom`），直接测 DOM 会抛 `ReferenceError`。 | 将主题类名/色阶计算与列属性转换提取为可注入桩对象或纯函数断言，保证 Node 原生单测 100% 纯净可跑。 |
| **内嵌无分页迷你表（如 Top 5 卡片）空态高度** | 若空插画过高会撑高 Dashboard 局部小卡片。 | `ArtTable` 默认空态强制使用 `<EmptyState compact>`（紧凑高度），无 `pagination` 对象时不渲染底部分页栏。 |

---

## 5. 验收门禁与实测清单

1. **自动化单测必测项**
   - 新增列宽与 `cleanColumnProps` 单测：① 断言 `supplier_code` 传入 `minWidth: 200` 时经 `useAdaptiveTableColumns` 计算后最小宽度严格 `>= 200`（不再被压回 `110`）；② 断言 `numeric: true` 自动生成 `align: "right"`、`tabular-nums` 且剥离 `numeric` 键。
   - 新增主题同步单测：使用轻量 DOM 桩验证通过 `switchThemeStyles(DARK)` 切换后同步写入 `class="dark"`、`data-dark-theme="obsidian"` 与暗色 `--el-color-primary-light-9`。
2. **构建、热替换与 `9224` 实机核验**
   - 执行 `mazbri-web` `pnpm build && pnpm test` 与 `mazbri-bin -p mazbri-mgmt` 门禁，原子热替换重启 `mazbri-mazbri-mgmt-1`。
   - 连接 `127.0.0.1:9224` 验证：① 在主控台顶栏与 `/#/operations/upstream-supply/workbench` 双向切换亮/暗主题，确认两侧均完整保留 `data-dark-theme="obsidian"` 且表格行内链接按钮清爽无眩光色块；② 供应商列表 `UPUPT-ENV-SC-SM4` 完整展示无截断，各列表底部分页栏准确渲染真实总条数；③ `/#/operations/callbacks` 空表呈现 `<EmptyState compact>` 且「导出」按钮置灰清晰。
