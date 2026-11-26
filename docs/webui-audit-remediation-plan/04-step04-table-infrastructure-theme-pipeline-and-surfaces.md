# Step 04：表格底座契约闭环（`total`、数值右对齐、空态、行内动作降噪）与唯一主题管线

## 1. 覆盖问题清单与源码精确定位

| 编号 | 关联审计项 | 级别 | 问题描述与核验纠偏 | 精确源码位置 |
| :--- | :---: | :---: | :--- | :--- |
| **W04-1** | `G-01` | P1 | `ResourceTablePageShell` 规范强制移除卡片头记录总数标签并约定“总条数统一由底部分页栏承担”，但底层 `ArtTable` 的 `constants.ts` 将 `"total"` 列入 `STRIP_LAYOUT_PARTS` 强制剔除，导致全站 20+ 张服务端分页表彻底丢失总记录数展示 | [constants.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/tables/art-table/constants.ts#L14-L25)、[art-table/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/tables/art-table/index.vue#L70-L145)、[resource-table-page-shell/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/common/resource-table-page-shell/index.vue#L1-L5) |
| **W04-2** | `G-04` | P1 | 免代码工作台 `WorkbenchTopBar` 手写 `toggleTheme()` 直接调用 `settingStore.setGlopTheme` + `applyToDom()`，绕过了 `useTheme().switchThemeStyles()` 动态重算 `--el-color-primary-light-1..9` 暗色衍生色阶的管线，导致切暗色返回主控台后表格按钮残留刺眼亮白浅蓝底 `#ecf5ff` | [WorkbenchTopBar.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/WorkbenchTopBar.vue#L376-L383)、[useTheme.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/hooks/core/useTheme.ts#L30-L85) |
| **W04-3** | `B-01` | P1 | **全局样式劫持纠偏**：`el-ui.scss` 第 763-848 行用 `!important` 强行把所有 `.el-table .el-button.is-link` 覆写为带实底与描边的彩色胶囊（且漏写 `:not(.is-disabled)` 导致禁用链接按钮仍高亮），制造满屏色块噪音与误触风险；此外租户列表 `tenant_code`、`tenant_id`、`name` 列宽分配局促 | [el-ui.scss](file:///workspace/rust-proj/mazbri/mazbri-web/src/assets/styles/core/el-ui.scss#L763-L848)、[table-action-row/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/common/table-action-row/index.vue#L21-L52)、[tenants/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/tenants/index.vue#L182-L210) |
| **W04-4** | `G-05` | P1 | `ListQueryBar` 未聚焦宽度锁死 `180px` 腰斩多模块搜索占位符，聚焦下拉面板直接遮挡首行；全站 55 个含表视图中 54 个未对金额/数量/耗时列声明右对齐与 `tabular-nums` | [list-query-bar/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/common/list-query-bar/index.vue#L1266-L1340)、[art-table/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/tables/art-table/index.vue#L20-L59) |
| **W04-5** | `B-11` | P2 | **禁用令牌根因纠偏**：`TableExportDropdown.vue` 实际已向内部 `<ElButton>` 透传 `:disabled="disabled"`，但全局默认白底按钮 `.is-disabled` 在浅色工具栏上字色/背景衰减度过弱，造成未禁用错觉；同时 `ArtTable` 默认 `#empty` 插槽仅渲染干瘪纯文本，未接入公共 `<EmptyState>` | [TableExportDropdown.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/table/TableExportDropdown.vue#L1-L25)、[el-ui.scss](file:///workspace/rust-proj/mazbri/mazbri-web/src/assets/styles/core/el-ui.scss#L87-L99)、[art-table/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/tables/art-table/index.vue#L63-L66) |
| **W04-6** | `D-01`, `D-02` | P2 | `用户管理` 短枚举列（状态、MFA）未封顶宽度均摊留白，导致核心 `邮箱` 列（`minWidth: 132`）过早截断为 `.co...`；`证书管理` 指纹列手写 `28×28` 方框复制按钮，未复用全站标准 `CopyableText` | [users/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/users/index.vue#L258-L300)、[certificates/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/certificates/index.vue#L74-L95) |

---

## 2. 第一性原理本质归因

1. **主题状态单源派生不变量（Single Pipeline Derivation，公理 U3）**：Element Plus 的视觉色彩系统由「根类 `html.dark`」与「内联动态混色变量 `--el-color-primary-light-1..9`」共同构成。只要允许代码库中存在第二个不重算衍生变量就能改 `html.dark` 的入口，两套状态投影就会立即分裂。唯一根治方式是在 `settingStore.applyToDom()` / `themeStore.applyThemeToDom()` 底层直接内聚衍生色阶同步，并令 `WorkbenchTopBar` 统一调用 `useTheme()`。
2. **契约在底座层闭合而非业务层重复劳动**：分页总数展示、空状态组件、数值列右对齐 + 等宽数字、表格操作按钮降噪与默认按钮禁用对比度，均属于全站 50+ 张表的共性契约。将它们收敛进 `ArtTable` 与 `el-ui.scss` 底座，可用 `< 150` 行底座代码一次性消除数十个业务视图的重复散装修补。

---

## 3. 详尽可落地技术方案

### 3.1 打通唯一主题切换管线并加固按钮/表格全局样式令牌（`mazbri-web`）

1. **收口工作台主题切换与初始化自愈（`W04-2`）**
   - 修改 [WorkbenchTopBar.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/WorkbenchTopBar.vue#L379-L383)：移除手写的 `settingStore.setGlopTheme` + `applyToDom()`，统一改为 `useTheme().switchThemeStyles(next)`。
   - 同时在 `themeStore.applyThemeToDom()`（或应用挂载入口）确保根据当前 `isDark` 幂等同步 `--el-color-primary-light-1..9`，即使从本地存储恢复暗色态也绝不残留亮色混色值。

2. **重构表格行内链接按钮与全局默认禁用按钮令牌（`W04-3`, `W04-5`）**
   - 修改 [el-ui.scss](file:///workspace/rust-proj/mazbri/mazbri-web/src/assets/styles/core/el-ui.scss#L763-L848) 中的 `.el-table .el-button.is-link`：
     - 移除常态下的高饱和实底背景与实线边框（`background-color: transparent !important; border-color: transparent !important;`），恢复纯净紧凑的语义色文字链接态；仅在 `:hover:not(.is-disabled)` 时浮现柔和微底纹（`var(--el-fill-color-light)` 或 `color-mix(in srgb, currentColor 10%, transparent)`）；
     - 显式增加 `&.is-disabled` 守卫：`color: var(--el-text-color-disabled) !important; background-color: transparent !important; cursor: not-allowed !important;`，修复禁用链接按钮仍呈亮色的缺陷；
     - 对高危 `删除` 操作，在 `TableActionRow` 中当同行按钮数 `>= 3` 时与前序常规操作拉开分隔线间距（或提供轻量竖线分隔），降低误触概率。
   - 在 [el-ui.scss](file:///workspace/rust-proj/mazbri/mazbri-web/src/assets/styles/core/el-ui.scss) 为非彩色默认按钮补充清晰的禁用态视觉衰减：
     ```scss
     .el-button--default.is-disabled:not(.is-link):not(.is-text) {
       background-color: var(--el-fill-color-light) !important;
       border-color: var(--el-border-color-lighter) !important;
       color: var(--el-text-color-disabled) !important;
       opacity: 0.68;
     }
     ```
     使 `TableExportDropdown` 等所有默认白底按钮在无数据禁用时一眼可辨。

### 3.2 恢复 `ArtTable` 底部分页 `total`、统一 `<EmptyState>` 与声明式数值列（`mazbri-web`）

1. **底部分页栏恢复总记录数展示（`W04-1`）**
   - 在 [art-table/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/tables/art-table/index.vue#L75-L87) 的 `.pagination-bar` 最左侧（`ElPagination` 尺寸选择器旁）显式渲染受控总数单元（受 `showTotal !== false` 控制）：
     ```vue
     <span
       v-if="showPaginationTotal"
       class="pagination-total tabular-nums"
     >
       {{ formattedTotalText }}
     </span>
     ```
   - 支持可选 `pagination.totalCapped?: boolean`（当海量日志表返回封顶计数时渲染 `共 10,000+ 条`，常规实体表渲染 `共 24 条` / `Total 24`），在 `zh.json` / `en.json` 补齐 `common.pagination.total` 与 `common.pagination.totalCapped`。

2. **`ArtTable` 默认空状态统一桥接 `<EmptyState compact>`（`W04-5`）**
   - 将 [art-table/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/tables/art-table/index.vue#L63-L66) 的 `#empty` 插槽升级为：优先渲染外部传入的 `$slots.empty`，未传时统一渲染 `<EmptyState compact :description="emptyText" />`，一次性统一回调订阅、投递任务等全站空表视觉体验。

3. **声明式数值列对齐与全站核心表批量校准（`W04-4`, `W04-6`）**
   - 在 `ColumnOption` 类型（[types/component](file:///workspace/rust-proj/mazbri/mazbri-web/src/types/component)）新增可选语义标记 `numeric?: boolean`：在 `ArtTable` 的 `cleanColumnProps(col)` 中，凡 `col.numeric === true` 自动默认注入 `align: col.align ?? "right"`、`headerAlign: col.headerAlign ?? "right"`，并合并 `className: "tabular-nums"`。
   - 批量为全站金额、单价、余额、调用次数、耗时、QPS、错误数、端口/实例数列补齐 `numeric: true`（覆盖全局总览 `ProductTopN`、租户账户流水、产品授权单价、通道成本价、账单管理、用量对账、调用监控耗时列等）。
   - 优化 `ListQueryBar` 与列宽细节：
     - 将 [list-query-bar/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/common/list-query-bar/index.vue) 默认宽度由死值 `180px` 调整为响应式 `clamp(210px, 18vw, 250px)`（聚焦展开至 `300px`），精简各模块超长占位符为字段名词组（如 `编码 / 账号 / 名称`），语法说明收入聚焦浮层头部并控制浮层最大高度不遮蔽多行数据；同步更新 [AGENTS.md](file:///workspace/rust-proj/mazbri/AGENTS.md#L49) 对应宽度描述；
     - [tenants/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/tenants/index.vue)：将 `tenant_code` 放宽至 `minWidth: 130`、`tenant_id` 放宽至 `minWidth: 190`（移除 18 字符业务账号上的过早 `compact` 截断）、`name` 放宽至 `minWidth: 180`；
     - [users/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/users/index.vue)：为 `status`、`mfa_enabled`、`mfa_required` 锁定精确 `width: 96`～`108`，将 `email` 提升至 `minWidth: 220`；
     - [certificates/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/certificates/index.vue#L74-L95)：删除 `.cert-fingerprint-cell` 手写复制按钮，统一替换为 `<CopyableText :value="row.fingerprint" :display="formatShortFingerprint(row.fingerprint)" />`。

---

## 4. 二阶、三阶影响与红队边界守卫

| 风险场景与反例 | 连锁影响分析 | 防护与消解措施 |
| :--- | :--- | :--- |
| **反例 1：卡片内嵌无分页迷你表（如 Dashboard Top 5）误渲染底部分页或大尺寸空插画** | `ArtTable` 同时用于整页资源表与监控小卡片，若空插画过高会撑破小卡片。 | `showPagination` 仅在传入 `pagination` 对象时激活；默认 `#empty` 强制采用 `EmptyState` 的 `compact` 紧凑模式（高度可控在 `120px` 内），不破坏小卡片高度。 |
| **反例 2：表格操作列去除实底背景后紧凑屏列宽留白偏大** | 原操作列宽度按带框胶囊计算，纯链接态视觉体积更轻盈。 | 保持 `28px` 触控热区与 `8px~12px` 间距，巡检租户、产品、供应商、通道、用户、证书 6 张主表操作列宽，确保中英双语均单行不折行且右侧无空旷脱节。 |
| **反例 3：验证全站分页接口 `pagination.total` 真值性** | 若个别列表前端传了假 `total: data.length`，显示 `共 X 条` 会在翻页前误导管理员。 | 静态核查全部挂载 `ResourceTablePageShell` / `ResourceTableShell` 的视图，确认 `pagination.total` 100% 绑定后端响应的 `res.total` 字段。 |

---

## 5. 验收门禁与实测清单

1. **自动化单测必测项**
   - 新增 `ArtTable` 组件/辅助函数单测：① 传入 `pagination = { current: 1, size: 10, total: 245 }` 时渲染 `共 245 条` / `Total 245`；② 列定义含 `numeric: true` 时自动派生 `align === "right"` 与 `"tabular-nums"` 类名。
   - 新增 `useTheme` 回归单测：断言调用 `switchThemeStyles(DARK)` 后 `document.documentElement` 同时具备 `dark` 类与重算后的 `--el-color-primary-light-9` 暗色值。
2. **构建、热替换与实机核验**
   - 执行 `mazbri-web` `pnpm build && pnpm test` 与 `mazbri-bin -p mazbri-mgmt` 门禁，热替换重启 `mazbri-mazbri-mgmt-1`。
   - 浏览器实测：① 进入免代码工作台切换亮/暗主题后返回 `/#/operations/upstream-supply?tab=channels`，确认暗黑模式下行内按钮无亮白眩光色斑；② 核验租户、审计日志、产品、账单列表底部分页栏均清晰展示总条数，数值列严格右对齐等宽；③ 在 `/#/operations/callbacks` 空表态核验 `<EmptyState compact>` 与置灰明显的「导出」禁用态；④ 核验用户管理邮箱完整展示、证书管理指纹复制交互与全站一致。
