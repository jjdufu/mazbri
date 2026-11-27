# Step 07：免代码工作台紧凑屏自适应、双主题 WCAG AA 校准与安全策略单事务保存

## 1. 覆盖问题清单与源码精确定位

| 编号 | 关联审计项 | 级别 | 问题描述与全栈契约定位 | 精确源码位置 |
| :--- | :---: | :---: | :--- | :--- |
| **W07-1** | `B-09` | P1 | 免代码工作台（路由 `/#/operations/upstream-supply/workbench`）上下 `Splitpanes` 默认 `requestSize = 52`（且持久化在 `localStorage["mazbri:wb:ide-layout"]`），在 `1280×800` 下分为 `454px : 420px`，未试调时下方空闲区占半屏而上方 `.wb-req-body`（`h: 332px`）裁切底部「启用业务报文加解密与签名」开关；同时门禁为 `校验 0/1` 时顶栏 `[发布]` 仍呈全高亮主色，`canPublish` 仅判断单次试跑成功而未对齐 `gate.ok === gate.total` | [workbench.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/workbench.vue#L168-L210)、[workbench.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/workbench.vue#L1298-L1470)、[WorkbenchTopBar.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/WorkbenchTopBar.vue#L300-L325)、[WorkbenchPublishDrawer.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/WorkbenchPublishDrawer.vue#L1-L210)、[useWorkbenchStore.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/useWorkbenchStore.ts#L170-L254)、[useWorkbenchStore.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/useWorkbenchStore.ts#L613-L617)、[useWorkbenchStore.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/useWorkbenchStore.ts#L2111-L2134) |
| **W07-2** | `B-10` | P1 | **亮/暗双主题占位与辅助文案对比度不足**：`workbench-tokens.scss` 中暗色 `--wb-text-faint: #68758d` 在 `#161b26`/`#212838` 上对比度仅 `3.10:1 ~ 3.63:1`，而**亮色模式** `--wb-text-faint: #8b95a7` 在 `#ffffff`/`#eaeff8` 上对比度仅 **`2.62:1 ~ 3.02:1`**，双双低于 WCAG 2.1 AA `4.5:1` 标准 | [workbench-tokens.scss](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/workbench-tokens.scss#L12-L86) |
| **W07-3** | `D-03` | P1 | **跨安全域分裂导致双保存按钮与 TOTP 重放死锁隐患**：`平台设置 -> 安全策略` 中密码策略（`PUT /settings/password-policy`，要求 `Category::Platform` MFA）与第 4 张卡 `GlobalMfaCard` 的 `totp_issuer`（`PUT /settings/mfa`，要求 `Category::Mfa` MFA）分属两个后端提权域，造成同页上下两个保存按钮（`top: 448` 与 `top: 663`）且点主按钮静默漏存 `totp_issuer`；设置页缺吸底栏与未保存拦截，`平台授权` 禁用态按钮缺提示且剩余天数微文案不一 | [password_policy_handlers.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/handlers/password_policy_handlers.rs#L13-L55)、[mfa_settings_handlers.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/handlers/mfa_settings_handlers.rs#L21-L65)、[SecurityPolicyTab.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/system/modules/SecurityPolicyTab.vue#L1-L45)、[GlobalMfaCard.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/system/modules/GlobalMfaCard.vue#L13-L30)、[usePlatformSecurityPolicy.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/system/modules/usePlatformSecurityPolicy.ts#L96-L138)、[PlatformLicenseTab.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/system/modules/PlatformLicenseTab.vue) |
| **W07-4** | `G-07` | P1 | `PersonalSecurityDrawer` 默认 `520px` 偏窄，首张 `<SettingsCard>` 内部委托渲染的子组件 `<SurfaceCardHeader>`（`.surface-card-actions`）中 `[重置备用码]`（`top: 133`）与 `[停用]`（`top: 175`）纵向错位折行，将 `.el-card__header` 撑高至 `103px` | [personal-security-drawer/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/common/personal-security-drawer/index.vue#L1-L62)、[surface-card-header/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/common/surface-card-header/index.vue#L25-L30) |

---

## 2. 第一性原理本质归因

1. **按字段安全敏感度正交归域，保障单事务单次 MFA 原子保存（公理 U4）**：若仅在前端删除 `GlobalMfaCard` 按钮并串行调用 `PUT /settings/password-policy`（`Category::Platform`）与 `PUT /settings/mfa`（`Category::Mfa`），不仅会连弹两次 MFA 弹窗，且管理员在第二个弹窗输入当前 30 秒窗口内的同一 6 位 TOTP 码时，会直接触发后端 `advance_totp_step` 防重放拦截报错 **`400 MFA_CODE_REUSED`** 锁死最多 30 秒！从领域本质看，`totp_issuer` 仅是验证器 App 展示的品牌发行方名称（非 `totp_digits` / `totp_period` 等密码学核心参数）。在后端将可选 `totp_issuer` 纳入 `Category::Platform` 守卫的安全策略单事务更新（同时保持 `PUT /settings/mfa` 继续由 `Category::Mfa` 保护底层算法参数），即可在不削弱安全边界的前提下达成真正的单请求、单事务、单次 MFA 原子保存。
2. **视口预算按信息熵分配与持久化版本演进**：未执行在线试调前下半栏信息熵趋近于零，初始分配 `74 : 26` 可让 `1280×800` 下半栏保持紧凑待命而上半栏 `.wb-req-body`（可用高度 `~388px`）完整露出底部的业务报文加解密开关；由于旧默认值 `52` 已写入客户端 `localStorage`，必须通过提升存储键版本使新布局对存量浏览器立即生效。

---

## 3. 详尽可落地技术方案

### 3.1 免代码工作台分栏演进、门禁对齐与亮/暗双主题 WCAG AA 校准（`mazbri-web`）

1. **紧凑视口动态分栏与 `.wb-req-body` 滚动提示（`W07-1`）**
   - 修改 [useWorkbenchStore.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/useWorkbenchStore.ts#L170-L254)：
     - 将持久化键升级为 `mazbri:wb:ide-layout:v2`（清理旧键 `mazbri:wb:ide-layout`），将未手动拖拽时的初始 `requestSize` 设为 **`74`**（请求区 `74%`、响应待命区 `26%`）；当首次试调返回 `lastRun` 且用户未手动拖拽分割线时，自动展开为 `48 : 52`；
     - 在 [WorkbenchResponsePane.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/WorkbenchResponsePane.vue#L49-L73) 适度收敛 `.wb-res__idle` 空态上下内边距与图标尺寸，确保非全屏浏览器窗口（`700px` 净高）下 `26%` 底栏无溢出裁切。
   - 在真实纵向滚动容器 **`.wb-req-body`**（[workbench.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/workbench.vue#L1460)，注意外层 `.wb-request` 为 `overflow: hidden` 且内部 `.wb-sheet` 不透明）底部挂载轻量 `position: sticky; bottom: 0` 渐隐阴影指示条（滚到底部时自动隐藏）；将 [CryptoProfileEditor.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/CryptoProfileEditor.vue#L249) 局部 `size="small"` 对齐至标准尺寸。
2. **门禁就绪度与发布按钮视觉/交互联动**
   - 修改 [WorkbenchTopBar.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/WorkbenchTopBar.vue#L311-L325) 与 [useWorkbenchStore.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/useWorkbenchStore.ts#L613-L617)：
     - 定义 `isGateReady = computed(() => gate.value.total > 0 && gate.value.ok >= gate.value.total)`；当 `!isGateReady` 时，为顶栏发布按钮施加次级描边受限样式 `.wb-topbar__primary-btn--gated` 并包裹 `<ElTooltip>` 陈述状态 `门禁未就绪（{ok}/{total}）`；
     - 在 `openPublish()`（第 2111 行）中：若 `!isGateReady` 且当前用户具备 `canForcePublish`（`R_ADMIN` / `R_FINANCE`），允许打开 [WorkbenchPublishDrawer.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/WorkbenchPublishDrawer.vue) 并在抽屉顶部显式展示门禁未满警告、引导开启强制发布（触发后端 `Category::Platform` MFA）或补齐试调门禁。
3. **亮/暗双主题 WCAG 2.1 AA 精确校准（`W07-2`）**
   - 修改 [workbench-tokens.scss](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/workbench-tokens.scss#L27-L80)：
     - **亮色模式**：保持 `--wb-text: #111827`、`--wb-text-muted: #4b5565`，将 `--wb-text-faint` 由 `#8b95a7`（`2.62:1`）加深为 **`#5e6878`**（在 `--wb-surface-hover: #eaeff8` 上对比度 **`4.68:1 >= 4.5:1`**，在 `#ffffff` 上 `5.65:1`）；
     - **暗色模式**：保持已达标的 `--wb-text: #e6edf7`（`14.65:1`）与 `--wb-text-muted: #9aa6bc`（`7.02:1`），将 `--wb-text-faint` 由 `#68758d` 提亮为 **`#8493a8`**（在最浅暗色表面 `--wb-surface-hover: #212838` 上对比度 **`4.70:1 >= 4.5:1`**，在 `--wb-surface: #161b26` 上 `5.55:1`）。

### 3.2 平台安全策略后端单事务保存与个人安全抽屉排版收敛（`mazbri-mgmt` & `mazbri-web`）

1. **后端单请求单事务收敛 `totp_issuer`（`W07-3`）**
   - 在 [password_policy_handlers.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/handlers/password_policy_handlers.rs#L13-L55) 的 `UpdatePasswordPolicyRequest` 新增可选字段 `pub totp_issuer: Option<String>`：
     - 保持 `Category::Platform` 单次 MFA 校验与 `R_ADMIN` 角色守卫；
     - 当 `totp_issuer` 非空时复用 `mfa_settings_service::validate_totp_issuer` 校验格式，在同一 PostgreSQL 事务内原子更新 `password_policies` 与 `mfa_settings.totp_issuer`，失效双缓存并记录单条完整审计日志（保持 `PUT /settings/mfa` 原有 `Category::Mfa` 守卫不变以保护 `totp_digits` / `totp_period`）。
   - 前端重构 [GlobalMfaCard.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/system/modules/GlobalMfaCard.vue) 与 [usePlatformSecurityPolicy.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/system/modules/usePlatformSecurityPolicy.ts)：
     - 移除 `GlobalMfaCard` 内嵌的独立 `[保存]` 按钮；记录初始快照派生 `isDirty`，由底部唯一 `[保存]` 按钮单次调用 `updatePasswordPolicy` 一并提交密码策略与 `totp_issuer`；
     - 为 `SecurityPolicyTab.vue` 与 `BasicSettings.vue` 挂载统一吸底栏 `.settings-sticky-footer`（左侧显示未保存状态点 `有未保存更改`，右侧放置 `保存` 按钮），在父级 `system/index.vue` 的 `<ElTabs :before-leave>`、路由 `onBeforeRouteLeave` 与 `window.beforeunload` 三处接入脏状态离开拦截；
     - 在 [PlatformLicenseTab.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/system/modules/PlatformLicenseTab.vue) 为未选文件时的禁用按钮包裹原因提示，统一全站剩余天数词条为 `剩余 {days} 天`。
2. **拓宽 `PersonalSecurityDrawer` 并校准 `<SurfaceCardHeader>` 动作区（`W07-4`）**
   - 将 [personal-security-drawer/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/common/personal-security-drawer/index.vue#L2-L50) 宽度调整为 `560px`（窄屏 `min(560px, 92vw)`），精简英文按钮词条为简洁动词（`"Backup Codes"` / `"Disable"`）。
   - 在 [surface-card-header/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/common/surface-card-header/index.vue#L25-L30)（及配套全局表面样式）为 `.surface-card-actions` 设置 `display: inline-flex; flex-wrap: nowrap; align-items: center; gap: 8px; flex-shrink: 0;`，确保标题、状态标签与操作按钮在同一水平基线利落排布。

---

## 4. 二阶、三阶影响与边界守卫

| 边界场景 | 连锁风险分析 | 消解与防护设计 |
| :--- | :--- | :--- |
| **同路由切换设置子页签（`基础设置` ↔ `安全策略`）丢失未存改动** | 仅挂 `onBeforeRouteLeave` 无法拦截组件内 `ElTabs` 切换。 | 在父容器 `system/index.vue` 的 `<ElTabs :before-leave>` 同步检查当前激活子页签的 `isDirty` 引用并弹窗确认。 |
| **存量浏览器 `localStorage` 残留 `requestSize: 52` 覆盖新默认值** | 旧键不升级会导致已访问过工作台的用户依然看到 `52 : 48` 裁切态。 | 布局存储键升版为 `mazbri:wb:ide-layout:v2`，首次加载自动应用 `74 : 26` 默认比例。 |

---

## 5. 验收门禁与实测清单

1. **自动化单测必测项**
   - 后端 `mazbri-mgmt` 单测：验证 `PUT /settings/password-policy` 携带 `totp_issuer` 时单事务同步更新密码策略与 `totp_issuer`，非法空串返回 `400` 整体回滚。
   - 前端 WCAG 对比度单测：解析 `workbench-tokens.scss` 亮/暗双色块中 `--wb-text-faint` 对 `--wb-bg`、`--wb-surface`、`--wb-surface-input`、`--wb-surface-hover` 的全部组合相对亮度比，断言 100% `>= 4.5`。
2. **构建、热替换与 `9224` 实机核验**
   - 执行 `mazbri-web` `pnpm build && pnpm test` 与 `mazbri-bin -p mazbri-mgmt` 门禁，原子热替换重启 `mazbri-mazbri-mgmt-1`。
   - 连接 `127.0.0.1:9224` 验证：① `1280×800` 访问 `/#/operations/upstream-supply/workbench`，确认「启用业务报文加解密与签名」完整可见无裁切、亮暗双模式占位符清晰可读、门禁 `0/1` 时发布按钮呈次级受限态；② `/#/control-panel/system?tab=security` 全页仅吸底栏一个「保存」按钮，同时修改密码长度与 `TOTP 发行者名称` 仅触发最多 1 次 MFA 即双项持久化成功；③ 打开个人安全抽屉确认卡片头单行水平对齐（高度收敛至 `~56px`）。
