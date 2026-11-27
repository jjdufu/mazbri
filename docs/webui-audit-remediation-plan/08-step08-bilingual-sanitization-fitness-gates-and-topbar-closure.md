# Step 08：模板死资产清扫、无损双语热切、`node:test` 架构门禁与顶栏收官

## 1. 覆盖问题清单与源码精确定位

| 编号 | 关联审计项 | 级别 | 问题描述与全栈契约定位 | 精确源码位置 |
| :--- | :---: | :---: | :--- | :--- |
| **W08-1** | `G-06`, `E-01` | P1 | **全仓扫描实存 13 个非注释硬编码中文文件与 40+ 处初始化期静态词条快照**：除 `JsonPathHelper`、`JsonTemplateEditor`、`ChangePasswordForm`、`SilencesPanel`、`OpsCard`、`settings-card` 外，还存在 `cryptoProfileHelpers.ts`（19 处密码学校验中文）、`workbenchHelpers.ts`（cURL 报错中文）、`alerts-format.ts`（`"全部"`）及 `useOpsChartHost.ts` / `utils/format.ts` 内联三元中文；`en.json` 存在 `"{action}ed"` 语法拼接错误（`Deleteed`）与超长字段标签（`routePurpose` 57 字符、`clockSkewSeconds` 36 字符撞上 `140px` `nowrap` 设置行）；`main.ts:15-54` 全局篡改所有弹窗内 `ElForm` 默认 `labelPosition = "left"` 加剧英文挤压；`LoginForm` 残留硬编码色值且暗色二维码缺柔和圆角衬底 | [cryptoProfileHelpers.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/cryptoProfileHelpers.ts#L385-L495)、[workbenchHelpers.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/workbenchHelpers.ts#L1190-L1196)、[alerts-format.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/utils/ops/alerts-format.ts#L91)、[JsonPathHelper.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/suppliers/components/JsonPathHelper.vue#L20-L84)、[JsonTemplateEditor.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/suppliers/components/JsonTemplateEditor.vue#L6-L25)、[ChangePasswordForm.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/auth/login/modules/ChangePasswordForm.vue#L146)、[main.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/main.ts#L15-L54)、[en.json](file:///workspace/rust-proj/mazbri/mazbri-web/src/locales/langs/en.json) |
| **W08-2** | `G-02` | P1 | **破坏性 `reload(50)` 阻碍无损语言切换**：`headerBar.ts` 静态关闭了 `language.enabled = false`，且 `art-header-bar/index.vue:187` 的 `changeLanguage` 调用 `reload(50)` 翻转 `<RouterView v-if="isRefresh">`——由于常规页与全屏免代码工作台均挂载在 `<ArtPageContent>` 内，`reload(50)` 会绕过路由守卫直接销毁未保存的设置表单与工作台多标签内存草稿！同时 `ArtUserMenu` 不显示当前账号与 RBAC 角色胶囊，顶栏锁屏、主题、退出 3 个纯图标按钮缺失 `<ElTooltip>` 与 `aria-label` | [headerBar.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/config/modules/headerBar.ts#L1-L32)、[art-header-bar/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/layouts/art-header-bar/index.vue#L58-L107)、[art-header-bar/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/layouts/art-header-bar/index.vue#L183-L188)、[art-page-content/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/layouts/art-page-content/index.vue#L19)、[ArtUserMenu.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/layouts/art-header-bar/widget/ArtUserMenu.vue#L1-L38) |
| **W08-3** | `E-02` | P2 | `403` / `404` / `500` 共用极简 `ArtException.vue`，仅提供单一出口按钮；`R_FINANCE` / `R_VIEWER` 触发 `403` 时不显示当前账号与角色，缺少返回或切换账号自愈路径 | [ArtException.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/views/exception/ArtException.vue#L1-L43)、[403/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/exception/403/index.vue) |
| **W08-4** | 历史包袱 | P2 | 前端残留上游 Art Design Pro 零消费死子树（永久空转的节日礼花/横幅、已下线外观设置的水平/混合菜单与 10 张预览 PNG、零引用的 `v-highlight` + `highlight.js` 重依赖）；`AGENTS.md` 第五节指向已删除目录，`page-templates.md` 引用不存在的 `ForbiddenBlock` | [festival.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/config/modules/festival.ts)、[images.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/config/assets/images.ts)、[highlight.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/directives/business/highlight.ts)、[AGENTS.md](file:///workspace/rust-proj/mazbri/AGENTS.md#L49-L94)、[page-templates.md](file:///workspace/rust-proj/mazbri/mazbri-web/docs/page-templates.md#L42-L63) |

---

## 2. 第一性原理本质归因

1. **纯响应式词条求值替代破坏性重挂载（Reactive I18n without Component Destruction，公理 U4）**：Vue 应用切换语言的正确模型是底层 `locale.value` 变化驱动模板与 `computed` 计算列自动重渲染，页面实例、表单脏状态与 IDE 工作台会话必须 100% 存活。清理掉初始化期一次性求值 `$t(...)` 的静态快照后，即可彻底删除 `reload(50)` 这一模板时代的粗暴刷新手段。
2. **用 `node:test` 编译/测试期适应度函数锁死不变量**：对标后端 `cargo test` 路由全覆盖断言，基于 `mazbri-web` 原生测试运行器（`node --import tsx --test 'src/**/*.test.ts'`）建立静态源码与词典结构扫描门禁，在先清扫上游模板死资产缩小扫描基线后，将双语对称、占位符一致、单一时区、唯一主题管线与禁用裸弹窗固化为每次构建必过的自动化防线。

---

## 3. 详尽可落地技术方案

### 3.1 切除上游模板零消费死子树并校准工程文档基线（`W08-4`）

1. **清理 Art Design Pro 遗留死代码与冗余依赖**（遵循 [AGENTS.md](file:///workspace/rust-proj/mazbri/AGENTS.md#L5-L6) 可破坏性重构原则；注意保留 [vite.config.ts](file:///workspace/rust-proj/mazbri/mazbri-web/vite.config.ts#L128) `additionalData` 全局注入的 `el-light.scss`）：
   - 删除永久空转的节日礼花与横幅子树：`art-fireworks-effect/`、`art-festival-text-scroll/`、`art-text-scroll/`、`hooks/core/useCeremony.ts`、`config/modules/festival.ts`、`assets/images/ceremony/*` 及 `setting.ts`/`display.ts` 对应死字段；
   - 删除零消费菜单布局与静态导入的 10 张设置预览图：`art-horizontal-menu/`、`art-mixed-menu/`、`config/assets/images.ts`、`assets/images/settings/**` 及 `config/index.ts` 中无消费者的 `settingThemeList` / `menuLayoutList`；
   - 删除零引用指令、组件、工具与 npm 包：`directives/business/highlight.ts`、`directives/core/auth.ts`、`assets/styles/custom/one-dark-pro.scss`、`assets/styles/core/md.scss`、`components/common/stack/`、`composables/ops/useRoleGatedLoader.ts`、`hooks/core/useAppMode.ts`、`utils/sys/upgrade.ts`、`utils/ui/emojo.ts`、`utils/constants/index.ts`、`types/common/index.ts`、`views/auth/login/types.ts`、`assets/images/3d/icon1.webp`、`assets/images/avatar/{avatar5,avatar10}.webp`，并从 [package.json](file:///workspace/rust-proj/mazbri/mazbri-web/package.json) 移除 `"highlight.js"`、`"@vue/reactivity"`、`"vue-demi"`；
   - 移除 [main.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/main.ts#L15-L54) 中遍历组件树强改 `ElForm.props.labelPosition.default` 为 `"left"` 的全局原型补丁，恢复弹窗表单显式或顶部对齐弹性布局能力。
2. **同步校准现态规范文档**
   - 更新 [AGENTS.md](file:///workspace/rust-proj/mazbri/AGENTS.md)：移除已完结且引用路径已不存在的第五节 16 步历史断点协议，对齐第 49 行 `ListQueryBar` `240px` 默认宽度与 `9223`/`9224` 实测端口说明。
   - 更新 [page-templates.md](file:///workspace/rust-proj/mazbri/mazbri-web/docs/page-templates.md)：以纯现态事实描述 `<EntityIdentityBar>`、`<StatusIndicator>`、详情多页签 `ElTabs` 与标准异常/空态规范，移除不存在的 `ForbiddenBlock` 引用。

### 3.2 清零全部 13 个非注释中文文件与 40+ 静态词条快照（`W08-1`）

1. **全量抽取硬编码中文并修复英文语法/长标签/微文案**
   - 将以下全部非注释中文抽取至 `zh.json` 与 `en.json`（仅保留 `utils/list-query/{parse-clause,suggest}.ts` 支持用户中文检索操作符的词法映射）：
     `cryptoProfileHelpers.ts`（19 处密码学校验文案）、`workbenchHelpers.ts`、`alerts-format.ts`、`JsonPathHelper.vue`、`JsonTemplateEditor.vue`、`ChangePasswordForm.vue`、`SilencesPanel.vue`、`OpsCard.vue`、`settings-card/index.vue`、`useOpsChartHost.ts`、`utils/format.ts`。
   - 修复 `en.json` 语法与爆框词条：
     - 将 `feedback.batch.actionSuccess` 拆分为离散动作词条或改为零词形变化结构 `"Batch {action}: {count} succeeded"`，根除 `Deleteed` / `Enableed`；
     - 精简超长字段标签（解释说明迁入 `info` 提示）：`routes.fields.routePurpose` ➔ `"Route purpose"`、`tenant.transportEncryption.fields.clockSkewSeconds` ➔ `"Clock Skew (s)"`（同时允许 `settings-card` 标签列安全换行）、`operations.monitoring.mode.ranged` ➔ `"Custom Range"`；将 `user-routes/index.vue` 等多字段弹窗表单设为 `label-position="top"`；
     - 净化高频表单中的 `请输入…` / `Please enter…` 教学式占位符为格式示例（如账号示例 `admin`、地址示例 `127.0.0.1:8080`），精简带后端副作用的冗长按钮文案（如证书包下载按钮改为简洁 `下载完整包` / `Download Bundle`）。
   - 将 [LoginForm.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/auth/login/modules/LoginForm.vue) 与 `art-sidebar-menu/theme.scss:264` 硬编码色值替换为语义变量；为 `MfaSetupForm.vue` 与 `PersonalSecurityDrawer` 的二维码统一包裹柔和圆角白底内衬托盘，复制按钮对齐 `32px`。
2. **升级全仓 40+ 处初始化期静态 `t()` 快照为响应式 `computed`**
   - 将 [headerBar.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/config/modules/headerBar.ts) 顶层立即调用的 `$t(...)` 改为存储 i18n key 并在消费侧响应式翻译；
   - 批量将 `AuthInjectFields.vue`、`drafts.vue`、`transport-encryption.vue`、`account.vue`、`tenants/[code]/tabs/products.vue`、`RulesPanel.vue`、`suppliers/[id]/index.vue` 等视图中在 `<script setup>` 顶层用 `const columns = ref([...t(...)])` 或静态常量定义的表头与下拉选项全面改为 `computed(() => [...])`。

### 3.3 异常页自愈闭环与 `node:test` 前端架构适应度门禁（`W08-3`）

1. **角色感知异常兜底页（遵循 Clean-Craft 2–4 字按钮规范）**
   - 扩展 [ArtException.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/views/exception/ArtException.vue) 支持 `#meta` 与 `#extra-actions` 插槽：
     - `403` 页：展示当前登录身份胶囊（`{username} · {roleLabel}`），提供简洁按钮 `返回`（仅当 `window.history.state?.back` 存在且非异常路由时调用 `router.back()`，否则回默认工作台首页防死循环）、`回首页`、`切换账号`；
     - `500` 页：提供 `重试` 与 `回首页`；`404` 页：提供 `返回` 与 `回首页`。
2. **新建 `mazbri-web/src/__tests__/architecture-fitness.test.ts`（基于原生 `node:test` + `node:assert/strict`）**
   随 `pnpm test` 与 GitHub Actions CI 强制执行五项静态契约断言：
   - **门禁 1（双语键对称与插值占位符守卫）**：递归校验 `zh.json` 与 `en.json` 叶子键集合严格相等、同一键两侧 `{placeholder}` 变量集合严格相等、`en.json` 零 `/\}\s*ed\b/` 词缀硬拼且零 CJK 字符残留；
   - **门禁 2（源码零未国际化中文守卫）**：扫描 `src/**/*.{vue,ts}`（剥离 HTML/JS/TS 注释，排除 `locales/`、`*.test.ts` 与显式列名的 `utils/list-query/{parse-clause,suggest}.ts`），断言零 CJK 字符串字面量残留；
   - **门禁 3（单一东八区时钟源守卫）**：扫描 `src/**/*.{vue,ts}`（排除 `*.test.ts` 与 `utils/format.ts`），断言零处直接调用 `.getHours()`、`.getMinutes()`、`.toLocaleTimeString()`；
   - **门禁 4（唯一主题状态管线守卫）**：扫描 `src/**/*.{vue,ts}`，断言仅 `store/modules/theme.ts`、`store/modules/setting.ts`、`hooks/core/useTheme.ts` 允许出现 `setGlopTheme`；
   - **门禁 5（公共弹窗外壳契约守卫）**：扫描 `src/views/**/*.vue`，断言零处绕过公共封装直接使用裸 `<ElDialog` / `<el-dialog` / `<ElDrawer` / `<el-drawer`。

### 3.4 移除 `reload(50)` 并开启全局顶栏与工作台无损双语/身份胶囊（`W08-2`）

在上述门禁 100% 绿灯后执行收官开启：
1. 修改 [art-header-bar/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/layouts/art-header-bar/index.vue#L65-L103) 与第 183–188 行：
   - **删除 `changeLanguage` 中的 `reload(50)` 调用**，仅执行 `locale.value = lang; userStore.setLanguage(lang);` 并同步更新 `document.title`；
   - 为语言 `<ElDropdown>` 显式设置 `trigger="click"`（消除与触发图标 `<ElTooltip placement="bottom">` 同时悬浮弹出双浮层的冲突），并为锁屏、语言、亮暗主题按钮补齐国际化 `<ElTooltip>` 与 `aria-label`。
2. 开启 [headerBar.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/config/modules/headerBar.ts#L21-L24) 中的 `language.enabled = true`，并在免代码工作台 [WorkbenchTopBar.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/WorkbenchTopBar.vue) 右侧工具区挂载同源无刷新语言切换菜单。
3. 升级 [ArtUserMenu.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/core/layouts/art-header-bar/widget/ArtUserMenu.vue)：渲染紧凑的「当前用户名 + 角色语义徽标（`管理员` / `财务` / `观察员`）」胶囊触发器，下拉菜单聚合 `安全设置`、`锁屏`、`退出`（简洁 2–4 字标签）。

---

## 4. 二阶、三阶影响与边界守卫

| 边界场景 | 连锁风险分析 | 消解与防护设计 |
| :--- | :--- | :--- |
| **工作台或平台设置编辑半途切换中英文** | 旧实现 `reload(50)` 会卸载 `<RouterView>` 导致未保存草稿与脏表单瞬间清空。 | 移除 `reload(50)` + 40+ 处计算列 `computed` 化 + ECharts `watch(locale)` 重绘，实现 100% 保状态的原地热切。 |
| **`ElDropdown` 默认 `hover` 与按钮 `ElTooltip` 叠压** | 同一图标既挂 `hover` 下拉菜单又挂底部文字提示会导致两个气泡重叠遮挡。 | 语言与用户菜单统一采用 `trigger="click"`，悬浮展示轻量 Tooltip、点击展开菜单。 |

---

## 5. 验收门禁与全量收官实测清单

1. **全栈静态检查与单测总门禁**
   - 运行 `cd mazbri-web && pnpm install --frozen-lockfile && pnpm build && pnpm test`（确认新增 `architecture-fitness.test.ts` 五大静态守卫及全仓所有 `*.test.ts` 100% 通过）。
   - 运行 `mazbri-libs` 与 `mazbri-bin` 全工作区 `cargo clippy --release --locked --workspace --all-targets -- -D warnings` 及全量 `cargo test --release --locked --workspace --no-fail-fast`，确认 `git diff --exit-code -- mazbri-web/src/types/api/generated` 零漂移。
2. **全量终验实机矩阵（`127.0.0.1:9224` 浏览器）**
   - 原子热替换重启收官容器后，按三维正交矩阵完成全路由闭环巡检：
     - **视口矩阵**：`1920×1080` 标准桌面、`1440×900` 笔记本视口、`1280×800` 紧凑视口；
     - **主题与无损双语矩阵**：亮色 + 中文、暗色 + 中文、亮色 + 英文、暗色 + 英文（特别验证在 `/#/operations/upstream-supply/workbench` 与 `/#/control-panel/system?tab=security` 含未保存改动时热切中英文，表单脏状态 100% 保留且表头/选项/图表即时翻译）；
     - **RBAC 三角色矩阵**：`R_ADMIN`、`R_FINANCE`、`R_VIEWER` 逐一核验顶栏身份角色胶囊准确、只读页面零写按钮泄漏、受限深链 `/#/403` 自愈返回顺畅、全流程控制台与网络面板零异常报错。
