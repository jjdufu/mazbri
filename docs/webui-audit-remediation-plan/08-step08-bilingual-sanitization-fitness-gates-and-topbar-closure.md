# Step 08：全量双语与微文案清零、异常页自愈闭环、Vitest 架构门禁与顶栏身份/语言开关收官

## 1. 覆盖问题清单与源码精确定位

| 编号 | 关联审计项 | 级别 | 问题描述与核验纠偏 | 精确源码位置 |
| :--- | :---: | :---: | :--- | :--- |
| **W08-1** | `G-06` | P1 | 多组件残留 30+ 处未国际化硬编码中文（`JsonPathHelper`、`JsonTemplateEditor`、`SilencesPanel`、`OpsCard`、`settings-card`）；`en.json` 将动作词直接机械拼接后缀 `"Successfully {action}ed {count} items"` 拼出错误语法 **`Deleteed` / `Enableed`**；`en.json` 将解释塞入字段标签（`routes.fields.routePurpose` 达 57 字符、`monitoring.mode.ranged` 达 49 字符）撑爆固定 `label-width="110px"` 表单 | [JsonPathHelper.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/suppliers/components/JsonPathHelper.vue#L20-L84)、[JsonTemplateEditor.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/suppliers/components/JsonTemplateEditor.vue#L6-L25)、[SilencesPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/alerts/SilencesPanel.vue#L251)、[en.json](file:///workspace/rust-proj/mazbri/mazbri-web/src/locales/langs/en.json)、[user-routes/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/operations/user-routes/index.vue#L221) |
| **W08-2** | `E-01` | P1 | 登录流首次强制改密表单校验硬编码中文 `"密码必须包含大小写字母和数字"`；`LoginForm` 残留硬编码 `#ff4e4f` / `#26272F` 色值；`MfaSetupForm` 与 `PersonalSecurityDrawer` 二维码在暗色卡片上白块直角边缘突兀，且旁侧密钥复制按钮混用 `size="small"` 与输入框高度不一 | [ChangePasswordForm.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/auth/login/modules/ChangePasswordForm.vue#L146)、[LoginForm.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/auth/login/modules/LoginForm.vue#L47)、[MfaSetupForm.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/auth/login/modules/MfaSetupForm.vue#L15-L85) |
| **W08-3** | `E-02` | P2 | `403` / `404` / `500` 异常兜底页共用极简 `ArtException.vue`，仅提供单一「返回首页」按钮；当 `R_FINANCE` / `R_VIEWER` 通过深链触发 `403` 时不展示当前账号与角色，也缺少「返回上一页」或「切换账号」自愈出口 | [ArtException.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/views/exception/ArtException.vue#L1-L43)、[403/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/exception/403/index.vue) |
| **W08-4** | `G-02` | P1 | **时序倒置守卫**：已登录全局顶栏 `headerBar.ts` 静态关闭了语言切换（`language.enabled = false`），`ArtUserMenu` 不显示当前登录账号名与生效 RBAC 角色胶囊，且锁屏、主题切换、退出三个纯图标按钮缺失 `<ElTooltip>` 与 `aria-label` | [headerBar.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/config/modules/headerBar.ts#L24-L26)、[art-header-bar/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/layouts/art-header-bar/index.vue#L58-L103)、[ArtUserMenu.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/layouts/art-header-bar/widget/ArtUserMenu.vue#L1-L38) |

---

## 2. 第一性原理本质归因

1. **先门禁清零、后开放入口（Gate-Before-Expose，红队修正准则）**：在尚未清零深水区硬编码中文、语法拼接缺陷与超长英文标签爆框前开放全局顶栏语言切换，会将隐蔽瑕疵放大为全链路生产故障。唯有先把全站双语词条与静态契约门禁做实，再在收官步打开顶栏语言开关，才能交付真正自洽的双语控制面。
2. **用自动化适应度函数替代人工规范记忆（Architecture Fitness Functions，公理 U4）**：后端之所以能长期维持零 `unsafe`、零路由漏鉴权、零动作标签漂移，是因为将这些架构约束编码成了每次 `cargo test` 必跑的静态断言。前端同样必须建立对等的 Vitest 静态源码扫描守卫，让任何新引入的硬编码中文、本地时区手写拼接、绕过 `useTheme` 切主题或中英词条键不对称直接在 CI 阶段红灯阻断。

---

## 3. 详尽可落地技术方案

### 3.1 全量双语词条清零、英文语法修正与弹性标签排版（`W08-1`, `W08-2`）

1. **清零全部 `.vue` / `.ts` 硬编码界面中文**
   - 将 [JsonPathHelper.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/suppliers/components/JsonPathHelper.vue)、[JsonTemplateEditor.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/suppliers/components/JsonTemplateEditor.vue)、[ChangePasswordForm.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/auth/login/modules/ChangePasswordForm.vue#L146)、[SilencesPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/alerts/SilencesPanel.vue#L251)、[OpsCard.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/ops/OpsCard.vue#L24) 与 [settings-card/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/common/settings-card/index.vue#L22) 中所有中文文案、占位示例与无障碍 `aria-label` 完整抽取至 `zh.json` 与 `en.json`。
2. **修复自然语言机械词缀拼接与超长表单标签**
   - 废弃 `en.json` 中 `"Successfully {action}ed {count} items"` 的词根硬拼写法，按离散动作枚举拆分为独立精准词条（`batchDeleteSuccess` / `batchEnableSuccess` / `batchDisableSuccess`）或统一使用零词形变化结构 `"Batch {action}: {count} succeeded"`，根除 `Deleteed` / `Enableed`。
   - 遵循微文案规范（字段标签仅保留简洁名词，解释说明迁入字段旁提示图标或下方辅助说明）：
     - `routes.fields.routePurpose`：英文精简为 `"Route purpose"`，括号说明迁入字段 `info` 提示；并将 [user-routes/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/operations/user-routes/index.vue#L221) 等弹窗表单升级为 `label-position="top"` 或响应式标签宽度，防止英文标签多行挤压；
     - `operations.monitoring.mode.ranged`：英文精简为 `"Time Range"` / `"Custom Range"`。
3. **登录流与二维码暗色视觉融合**
   - 将 [LoginForm.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/auth/login/modules/LoginForm.vue) 中的 `#ff4e4f` / `#26272F` 及 [theme.scss](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/layouts/art-menus/art-sidebar-menu/theme.scss#L264) 中的硬编码色值替换为 `--el-color-danger` 与 `--el-bg-color-overlay` 等语义令牌。
   - 为 [MfaSetupForm.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/auth/login/modules/MfaSetupForm.vue) 与 `PersonalSecurityDrawer` 的二维码画布统一包裹圆角柔和白底内衬容器（`padding: 10px; border-radius: 10px; background: #ffffff; box-shadow: var(--el-box-shadow-light)`，兼顾高对比扫码识别率与暗色卡片过渡），并将密钥复制按钮尺寸统一为 `32px`。

### 3.2 异常兜底页角色感知与多路径自愈（`W08-3`）

- 扩展 [ArtException.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/views/exception/ArtException.vue) 支持可选插槽 `#meta` 与 `#extra-actions`：
  - **`403` 页**：在说明下方展示当前身份胶囊（如 `当前账号：finance_01 · 财务运营`），操作区并排提供次按钮 `返回上一页`、主按钮 `返回首页` 与轻量按钮 `切换账号`（触发登出跳登录页）；
  - **`500` 页**：提供 `重试`（刷新当前目标路由）与 `返回首页`；
  - **`404` 页**：提供 `返回上一页` 与 `返回首页`。

### 3.3 落地 Vitest 前端架构适应度门禁（`mazbri-web` CI 防复发底座）

新建 [src/__tests__/architecture-fitness.test.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/__tests__/architecture-fitness.test.ts)，随 `pnpm test` 与 GitHub Actions CI 强制执行以下四项静态守卫断言：

1. **双语词条树对称与反模式守卫**：递归遍历 `zh.json` 与 `en.json` 所有叶子键路径，断言两侧键集合差集严格为空（`missingInEn.length === 0 && missingInZh.length === 0`），且 `en.json` 全文不匹配 `/\}\s*ed\b/` 机械词缀拼接及任何残留 CJK 字符。
2. **源码未国际化硬编码中文扫描守卫**：扫描 `mazbri-web/src/**/*.{vue,ts}`（排除测试文件、`locales/`、注释块与显式受控常量如时区/正则测试数据），断言 `<template>` 属性/文本与脚本字符串字面量零未国际化中文残留。
3. **单一时区源守卫**：扫描 `mazbri-web/src/**/*.{vue,ts}`（排除测试文件与允许列表），断言零处直接调用 `.getHours()`、`.getMinutes()`、`.toLocaleTimeString()` 绕过 `utils/format.ts` 的 `DISPLAY_TIME_ZONE`。
4. **唯一主题管线守卫**：扫描 `mazbri-web/src/**/*.{vue,ts}`，断言仅 `hooks/core/useTheme.ts` 与 `store/modules/setting.ts` 允许引用 `setGlopTheme`，杜绝任何业务视图私自切换底层主题状态。

### 3.4 开启全局顶栏与工作台双语入口、补齐身份角色胶囊与按钮提示（`W08-4`）

在上述三节与架构门禁 100% 通过后，执行收官开启：
1. 将 [headerBar.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/config/modules/headerBar.ts#L24-L26) 中的 `language.enabled` 设为 `true`，并在全屏免代码工作台 [WorkbenchTopBar.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/WorkbenchTopBar.vue) 右侧工具区同步挂载同源语言切换下拉菜单，实现登录页 ↔ 主控台 ↔ 免代码工作台全链路语言即时同步。
2. 升级 [ArtUserMenu.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/layouts/art-header-bar/widget/ArtUserMenu.vue) 与 [art-header-bar/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/layouts/art-header-bar/index.vue#L58-L103)：
   - 为顶栏所有纯图标按钮（锁屏、语言切换、亮暗主题）补齐 `<ElTooltip placement="bottom">` 与国际化 `aria-label`；
   - 在 `ArtUserMenu.vue` 渲染紧凑的「当前用户名 + 角色语义徽标（`管理员` / `财务` / `观察员`）」触发胶囊，展开下拉菜单聚合 `安全设置`、`锁屏`、`退出登录`，使多账号值班员随时清晰感知当前权限上下文。

---

## 4. 二阶、三阶影响与红队边界守卫

| 风险场景与反例 | 连锁影响分析 | 防护与消解措施 |
| :--- | :--- | :--- |
| **反例 1：已登录会话中动态切换 `zh` ↔ `en` 时，已挂载 ECharts 图表与计算列标题未重绘** | 部分组件若在初始化时用非响应式快照 `const label = $t(...)` 缓存标题，切语言后会出现半页英文半页中文。 | 检查使用静态数组定义表头的组件确保包裹在 `computed(() => [...])` 内，并确认 `useOpsChartHost` / 各图表组件已 `watch(locale, () => renderChart())`（如 `TrafficCallsPanel` 第 198 行模式全覆盖）。 |
| **反例 2：Vitest 中文扫描误伤源码注释或合法日志常量** | 仓库代码广泛使用详尽的中文 JSDoc 与行内架构注释（如 `[不可更改]`）。 | 扫描器先剥离 HTML 注释 `<!-- ... -->`、块注释 `/* ... */` 与单行注释 `// ...` 后再检测非注释代码区 CJK 字符，精准拦截用户可见硬编码中文而完全不影响中文源码注释。 |
| **反例 3：`403` 页点击「返回上一页」时，若浏览器历史栈首跳即为外部粘贴的无权限深链** | `window.history.length <= 1` 或上一跳仍为受限路由时，`router.back()` 无效或循环回跳 `403`。 | 判断 `window.history.state?.back` 存在且非 `/403` 时执行 `router.back()`，否则安全回退至当前角色有权限的默认工作台首页 `homePath.value`。 |

---

## 5. 验收门禁与全量收官实测清单

1. **全栈静态检查与单测总门禁**
   - 运行 `cd mazbri-web && pnpm build && pnpm test`，确认新增的 `architecture-fitness.test.ts` 四大静态架构守卫及全仓所有 `*.test.ts` 100% 绿灯。
   - 运行 `mazbri-libs` 与 `mazbri-bin` 全工作区 `cargo clippy --release --locked --workspace --all-targets -- -D warnings` 及全量 `cargo test --release --locked --workspace --no-fail-fast`，确认 `git diff --exit-code -- mazbri-web/src/types/api/generated` 零漂移。
2. **全量终验实机矩阵（`9223`/`9224` 浏览器）**
   - 热替换重启收官容器后，按以下三维正交矩阵完成全路由闭环巡检：
     - **视口矩阵**：`1920×1080` 标准桌面、`1440×900` 笔记本视口、`1280×800` 紧凑视口（含 `<1100px` 工作台侧栏折叠态）；
     - **主题与语言矩阵**：亮色 + 中文、暗色 + 中文、亮色 + 英文、暗色 + 英文（在顶栏与免代码工作台实时热切验证零色阶失步、零硬编码中文残留、零标签爆框）；
     - **RBAC 角色矩阵**：`R_ADMIN`、`R_FINANCE`、`R_VIEWER` 逐一核验顶栏角色胶囊显示准确、受限深链访问 `/#/403` 自愈动作可用、全流程浏览器控制台零 Error / Warning。
