# Step 07：免代码工作台紧凑屏自适应、暗色对比度提亮与平台设置单按钮脏状态闭环

## 1. 覆盖问题清单与源码精确定位

| 编号 | 关联审计项 | 级别 | 问题描述与核验纠偏 | 精确源码位置 |
| :--- | :---: | :---: | :--- | :--- |
| **W07-1** | `B-09` | P1 | **超管强制发布逃生门禁守卫**：免代码工作台在 `1440×900` / `1366×768` / `1280×800` 笔记本紧凑高度下，上下 `Splitpanes` 死板按 `50%:50%` 平分，导致未试调时下方空白响应区空占 `340px`，而上方 `出站规格 -> 请求构造` 底部的核心开关「启用业务报文加解密与签名」被分割线拦腰硬裁切；同时顶栏门禁为红点 `0/1` 时右侧 `[发布]` 仍呈全高亮主按钮（注：不能简单加原生 `disabled` 封死 `force_publish` 与悬停提示） | [workbench.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/workbench.vue#L168-L210)、[workbench.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/workbench.vue#L540-L560)、[WorkbenchTopBar.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/WorkbenchTopBar.vue#L300-L325)、[useWorkbenchStore.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/useWorkbenchStore.ts#L2111-L2134)、[CryptoProfileEditor.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/CryptoProfileEditor.vue#L245-L255) |
| **W07-2** | `B-10` | P1 | 免代码工作台暗黑模式下（底色 `#131720` / `#161b26`），`workbench-tokens.scss` 定义的 `--wb-text-muted` 与 `--wb-text-faint` 亮度对比度仅约 `2.5:1 ~ 2.8:1`，远低于 WCAG AA `4.5:1` 标准，长时间联调时键值表占位符与辅助文案雾化难辨 | [workbench-tokens.scss](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/workbench-tokens.scss#L61-L85) |
| **W07-3** | `D-03` | P1 | **前端脏字段联动提交纠偏**：`平台设置 -> 安全策略` 同页前 3 张卡由右下角 `[保存安全策略]` 提交，唯独第 4 张卡 `GlobalMfaCard` 内嵌独立 `[保存]` 按钮，管理员同时修改密码长度与 `TOTP 发行者名称` 后触发底部主按钮时，`TOTP 发行者名称` 的修改被静默丢弃；且设置页底部无吸底操作栏与脏数据离开提示，`平台授权` 页签禁用态 `[导入激活]` 缺原因提示、`剩余/还剩` 天数微文案不一 | [SecurityPolicyTab.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/system/modules/SecurityPolicyTab.vue#L1-L45)、[GlobalMfaCard.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/system/modules/GlobalMfaCard.vue#L13-L30)、[usePlatformSecurityPolicy.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/system/modules/usePlatformSecurityPolicy.ts#L96-L138)、[PlatformLicenseTab.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/system/modules/PlatformLicenseTab.vue) |
| **W07-4** | `G-07` | P1 | 个人安全设置抽屉（`PersonalSecurityDrawer`）默认 `520px` 宽度偏窄，首张卡片头部标题折成 `两步验证 / (MFA)` 两行，右侧 `[重置备用码]` 与 `[停用]` 纵向错位堆叠将头部撑高至 `90px` | [personal-security-drawer/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/common/personal-security-drawer/index.vue#L1-L55) |

---

## 2. 第一性原理本质归因

1. **视口预算按信息熵动态分配（Entropy-Proportional Viewport Allocation，公理 U4）**：在 IDE 型上下分栏界面中，尚未发起任何在线试调（`lastRun === null`）时，下半栏的信息熵趋近于零，而上半栏承载着 URL、认证、请求头、Body 编码与出站报文加解密等高密度必填表单。按静态 `50%:50%` 均分视口违反了信息熵配比原则，必然在 `<= 900px` 笔记本高度下截断底部安全开关。
2. **意图原子化与最小后端扰动平衡（公理 U4 + 红队修正准则）**：用户在同一个「安全策略」页签编辑表单时，心智模型是单一表单提交。无需为了低频修改的 `totp_issuer` 破坏性重构后端 REST 接口与 `ts-rs` 绑定——在前端状态层记录初始快照 `initialSnapshot`，仅在脏字段集合非空时按需联动提交对应子接口并对失败子项精确保脏提示，即可在零后端接口破坏的前提下达成单按钮原子体验。

---

## 3. 详尽可落地技术方案

### 3.1 免代码工作台自适应分栏、滚动提示与发布门禁视觉校准（`mazbri-web`）

1. **未试调态与紧凑视口动态高度比（`W07-1`）**
   - 修改 [useWorkbenchStore.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/useWorkbenchStore.ts) 与 [workbench.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/workbench.vue#L168-L176)：
     - 将初始默认 `requestSize` 由 `50` 提升至 **`74`**（即未手动拖拽且未产生试调响应前，请求编辑区占 `74%`、底部待命响应条占 `26%`，确保 `1280×800` / `1440×900` 下完整露出底部的「启用业务报文加解密与签名」开关）；
     - 当用户首次执行「试调」拿到 `lastRun` 响应且此前未手动拖拽固定比例时，自动平滑展开至 `48 : 52` 便于检视响应报文；用户一旦手动拖拽分割条（`userResizedInner = true`），则尊重用户自定义比例。
   - 为 `.wb-request` 滚动容器底部增加 CSS 渐隐阴影提示（利用 `background-attachment: local, local, scroll, scroll` 纯 CSS 滚动阴影，无 JS 开销），明确提示下方仍有可滚配置项；同步将 [CryptoProfileEditor.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/CryptoProfileEditor.vue#L249) 局部混用的 `size="small"` 控件对齐至工作台标准尺寸。

2. **门禁徽标与「发布」按钮视觉语义联动（保留超管 `force_publish`）**
   - 修改 [WorkbenchTopBar.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/WorkbenchTopBar.vue#L311-L325) 与 [useWorkbenchStore.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/useWorkbenchStore.ts#L2111-L2117)：
     - 当 `!canPublish`（即门禁 `gate.ok < gate.total`）时，为发布按钮附加修饰类 `.wb-topbar__primary-btn--gated`（降级为克制的次级描边态而非高饱和实心蓝主色），包裹 `<ElTooltip>` 清晰提示门禁未就绪原因（纯状态陈述：`门禁校验未完成（{ok}/{total}）`，符合微文案规范）；
     - 若当前角色具备 `canForcePublish`（超级管理员），在 `openPublish()` 中允许打开 `PublishModal` 并在弹窗内显式警告门禁未过、要求勾选强制发布开关与填写强制原因；普通业务角色未过门禁触发时则自动展开门禁清单浮层定位缺失项。

3. **提亮工作台暗色令牌至 WCAG AA 基线（`W07-2`）**
   - 在 [workbench-tokens.scss](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/workbench-tokens.scss#L61-L85) 的 `html.dark .connector-workbench-page` 作用域下校准对比度：
     - `--wb-text: #e5eaf3`（对比度 `~13.2:1`）；
     - `--wb-text-muted: #94a3b8`（对比度 `~6.1:1`，用于次级标签与元信息）；
     - `--wb-text-faint: #7c8ba1`（对比度 `~4.6:1 >= 4.5:1`，用于键值表占位符与空状态说明）；
     - `--wb-divider: #283246`，并为激活编辑行增加微弱聚焦边框高亮。

### 3.2 平台设置单保存按钮收敛、吸底操作栏与安全抽屉防折行（`mazbri-web`）

1. **收敛 `SecurityPolicyTab` 唯一保存入口与脏状态守卫（`W07-3`）**
   - 移除 [GlobalMfaCard.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/system/modules/GlobalMfaCard.vue#L21-L28) 内嵌的独立 `[保存]` 按钮与 `@keyup.enter="saveIssuer"`。
   - 重构 [usePlatformSecurityPolicy.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/system/modules/usePlatformSecurityPolicy.ts)：
     - 在 `loadAll()` 成功后记录 `savedPolicyJson` 与 `savedIssuer` 快照，派生响应式计算属性 `isPolicyDirty`、`isIssuerDirty` 与总脏标志 `isDirty`；
     - 在统一的 `savePolicy()` 中：先执行前端校验（特殊字符非空、`totp_issuer.trim()` 非空）；若 `!isDirty.value` 则无需重复请求；若 `isPolicyDirty` 则调用 `updatePasswordPolicy` 并更新 `savedPolicyJson`；若 `isIssuerDirty` 则紧接调用 `updateMfaSettings` 并更新 `savedIssuer`——若第二步因故失败，保留 `isIssuerDirty = true` 并明确提示未完成子项，不吞没脏状态；全部成功后弹出单次保存成功反馈。
   - 为 `SecurityPolicyTab.vue` 与 `BasicSettings.vue` 统一挂载吸底操作栏 `.settings-sticky-footer`（半透明背景毛玻璃衬底 + 顶部分隔细线，左侧实时显示脏状态圆点提示 `有未保存更改`，右侧放置唯一主按钮 `保存`），并通过 `onBeforeRouteLeave` 拦截未保存离开。
   - 在 [PlatformLicenseTab.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/system/modules/PlatformLicenseTab.vue) 为未选文件禁用态的「导入激活」包裹 `<ElTooltip>`（提示 `未选择 .jws 授权文件`），并将全站有效期剩余天数词条统一对齐为 `剩余 {days} 天`。

2. **拓宽 `PersonalSecurityDrawer` 并锁定卡片头单行基线（`W07-4`）**
   - 修改 [personal-security-drawer/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/common/personal-security-drawer/index.vue#L2-L55) 与 [settings-card/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/common/settings-card/index.vue)：
     - 将抽屉宽度由 `520px` 提升至标准表单抽屉档位 `560px`（移动端自适应 `min(560px, 92vw)`）；
     - 为卡片头部标题区声明 `white-space: nowrap; flex-shrink: 0`，右侧 `#actions` 容器声明 `display: inline-flex; flex-wrap: nowrap; align-items: center; gap: 8px; flex-shrink: 0`，确保 `两步验证 (MFA)`、`已启用` 标签与 `[重置备用码]` `[停用]` 始终在同一水平基线利落排布。

---

## 4. 二阶、三阶影响与红队边界守卫

| 风险场景与反例 | 连锁影响分析 | 防护与消解措施 |
| :--- | :--- | :--- |
| **反例 1：平台设置内切换子 Tab（`基础设置` ↔ `安全策略`）时未触发 `onBeforeRouteLeave`** | 同路由页签切换若通过内部 `v-model` 驱动而不改路由或仅改 query，Vue Router 的 `onBeforeRouteLeave` 不会拦截子页签切换丢脏数据。 | 同时在父级 `system/index.vue` 的 `:before-leave` 钩子与 `window.beforeunload` 接入子页签 `isDirty` 确认守卫，覆盖切 Tab、切菜单与关标签页三类离开路径。 |
| **反例 2：`savePolicy` 串行调用双接口时的 Step-up MFA 弹窗次数** | `password_policy.update` 与 `mfa_settings.update` 同属管理安全策略类别。 | [mfa-dialog-service.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/utils/http/mfa-dialog-service.ts#L253-L261) 的 `ensureCategory` 按 `MfaCategory` 共享提权窗口期（提权一次后窗口期内有效），串行发起的第二个请求自动复用刚获得的会话提权票据，绝不会连续弹出两次 MFA 框。 |

---

## 5. 验收门禁与实测清单

1. **自动化单测必测项**
   - 新增 `usePlatformSecurityPolicy` 脏状态与联动保存单测：① 仅改密码策略时只发 `updatePasswordPolicy`；② 同时改密码策略与 `totp_issuer` 时依次调用双接口并清零 `isDirty`；③ 第二接口报错时保留 `isIssuerDirty === true`。
   - 新增暗色令牌 WCAG 对比度计算单测：解析 `workbench-tokens.scss` 暗色块中的 `--wb-bg` / `--wb-surface-input` 与 `--wb-text-faint` 计算相对亮度比，断言 `>= 4.5`。
2. **构建、热替换与实机核验**
   - 执行 `mazbri-web` `pnpm build && pnpm test` 与 `mazbri-bin -p mazbri-mgmt` 门禁，热替换重启 `mazbri-mazbri-mgmt-1`。
   - 浏览器实测：① 在 `1280×800` 视口打开免代码工作台，确认首次进入时「启用业务报文加解密与签名」完整露出无裁切，暗黑模式键值表占位符清晰可读，门禁 `0/1` 时发布按钮呈受限次级态；② 在 `/#/control-panel/system?tab=security` 验证全页仅底部吸底栏一个「保存」按钮，同时修改密码策略与 `TOTP 发行者名称` 保存后刷新页面双项均持久化生效；③ 打开顶栏个人安全抽屉确认卡片头单行水平对齐无折行。
