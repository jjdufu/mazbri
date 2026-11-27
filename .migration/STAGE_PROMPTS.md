# Mazbri WebUI React 19 架构迁移 — 分阶段新会话执行提示词（Stage 1 ~ Stage 7）

> **使用说明**：每个阶段开启一个**全新的 Agent 会话**（工作目录设为 `mazbri` 根目录，例如 `/workspace/rust-proj/mazbri`），直接复制对应阶段的提示词全文发给 Agent 即可。
> **前置一次性准备（开发机执行 Stage 2 前完成）**：将 `.migration/.env.local.example` 复制为 `.migration/.env.local`（已被 `.gitignore` 忽略，永不入库），填入测试账号密码与 Base32 TOTP Secret，并确认本机 Chrome 调试端口 `127.0.0.1:9224` 与目标服务 `http://192.168.50.50:8085` 可达。

---

## Stage 1 提示词｜旧代码隔离 + React 19 工程初始化 + 3.3 万行纯 TS 资产平移 + Rust 路径对齐

```markdown
你现在负责执行 Mazbri WebUI React 19 破坏性架构迁移的 **Stage 1**。

### 一、必读上下文（动手前必须完整读取）
1. `AGENTS.md`（注意顶部迁移哨兵块）
2. `.migration/MIGRATION_IRON_LAWS.md`
3. `.migration/MIGRATION_PLAN.md`
4. `.migration/PROGRESS.md`
5. `.migration/FEATURE_PARITY_LEDGER.md`
6. `.migration/reference/package.reference.json` 与 `.migration/reference/vite.config.reference.ts`

### 二、本阶段精确目标（单 Agent 串行，严禁越界提前写业务页面）
1. **隔离旧前端**：将现有 `mazbri-web/` 整体重命名为 `mazbri-web-legacy/`（作为后续阶段只读对照源，严禁在 legacy 中做功能开发），在原位新建纯净的 React 19 + TypeScript + Vite + Tailwind CSS v4 工程 `mazbri-web/`（完全脱离外部 xBlueNode 目录，仅参照 `.migration/reference/`）。
2. **安装精简依赖**：仅安装 React 19、React DOM、React Router、Zustand、@tanstack/react-query、@tanstack/react-table、Tailwind CSS v4 (@tailwindcss/vite)、lucide-react、echarts、@codemirror/*（复用 legacy 已有版本）、diff、diff2html、dompurify、file-saver、qrcode.react（或轻量 QR 方案）、axios、clsx、tailwind-merge、vite-plugin-compression 等必要依赖；**严禁出现任何 Vue / Pinia / Element Plus / Art Design Pro / Sass 依赖**。
3. **零重写平移 3.3 万行纯 TS/JSON 业务资产**：从 `mazbri-web-legacy/src/` 原样平移并整理以下纯资产到新 `mazbri-web/src/`：
   - `api/`（27 个 API 模块）
   - `types/`（保留 `types/api/generated/` 路径不变，满足 `mazbri-bin/mazbri-mgmt/build.rs` 的 `ts-rs` 契约）
   - `utils/`（120+ 纯 TS 工具与 `utils/http/` 内所有纯 TS 文件及单元测试）
   - `locales/`（`zh.json` / `en.json`，并立即补齐审计发现的 4 个缺失 Key：`common.actions.edit` 与 `ops.components.statusName.{healthy,degraded,down}`，并编写轻量无刷新响应式 `useT()` i18n 模块）
   - `constants/`、`enums/`、`list-catalog/`
   - `domains/control-panel/audit-logs/labels.ts`（从旧 `views/mazbri/control-panel/audit-logs/labels.ts` 迁入，并**同步修改 Rust 侧** `mazbri-bin/mazbri-mgmt` 与 `mazbri-bin/mazbri-cert-service` 中引用该文件的 `include_str!` 与单测路径，彻底消除旧 Vue 路径残留）
   - `domains/workbench/engine/`（原样平移 `upstream-connectors` 下 10 个纯 TS 引擎文件与 4 个引擎单测）
4. **仅重写 2 个薄桥接层**：
   - `src/utils/http/index.ts`：剥离 Pinia 依赖，对接新的 Zustand `useAuthStore`；
   - `src/utils/http/mfa-dialog-service.ts`：剥离 Element Plus 依赖，改为框架无关的 Promise 事件/Store 桥（严格守住 U1 公理：`MfaStepUpCancelledError` 与网络取消正交）；
   - 建立最小可构建的 React 占位入口 `src/main.tsx` + `src/App.tsx`（使 `pnpm build` 输出有效 `dist/`）。

### 三、本阶段必过门禁与验收
1. 运行 `node .migration/gates/gate-0-env-hygiene.mjs`（确认无敏感文件入暂存区）；
2. 运行 `cd mazbri-web && pnpm tsc --noEmit && pnpm build && pnpm test`（确保平移过来的全部纯 TS 单元测试 100% 通过）；
3. 运行 `cd mazbri-bin && cargo test --release --locked -p mazbri-mgmt -p mazbri-cert-service`（验证 `build.rs`、`ts-rs`、`labels.ts` 新路径、`mfa_guard` 全部绿灯，且 `git diff --exit-code -- mazbri-web/src/types/api/generated` 无脏漂移）；
4. 清理本步多余编译垃圾（删除 `mazbri-bin/target/debug` 若存在），记录磁盘快照。

### 四、交付与退出
- 更新 `.migration/PROGRESS.md` 的 Stage 1 状态与 Commit/磁盘记录；
- 按 `.migration/MIGRATION_IRON_LAWS.md` 铁律 10 的 5 段式详尽中文模板执行本地 `git commit`（**严禁 `git push`**，严禁修改 `VERSION`）。
```

---

## Stage 2 提示词｜Design System + xBlueNode 原语移植 + 4 大 B 端骨架壳 + AppShell + 认证/MFA/锁屏闭环（底座封板）

```markdown
你现在负责执行 Mazbri WebUI React 19 架构迁移的 **Stage 2（底座封板阶段）**。

### 一、必读上下文
1. `AGENTS.md`、`.migration/MIGRATION_IRON_LAWS.md`、`.migration/MIGRATION_PLAN.md`、`.migration/PROGRESS.md`、`.migration/FEATURE_PARITY_LEDGER.md`
2. 完整阅读 `.migration/reference/DESIGN_CONTRACT.md`、`.migration/reference/globals.css`、`.migration/reference/primitives.tsx`、`.migration/reference/ResizableWorkspace.tsx`
3. 对照读取 `mazbri-web-legacy/src/views/auth/login/` 与 `mazbri-web-legacy/src/components/common/mfa-step-up-dialog/`、`personal-security-drawer/`、`LockScreen` 相关实现，确保零功能倒退。

### 二、Subagent 并行分工（拉起 2 个目录互斥 Subagent 并行施工，你作为 Coordinator 总装验收）
- **SA-UI（独占 `mazbri-web/src/styles/**`、`mazbri-web/src/components/ui/**`）**：
  - 参照 `.migration/reference/globals.css` 与 `DESIGN_CONTRACT.md` 实现 `src/styles/globals.css`（保留 Mazbri 深海蓝侧栏 `#0b1733` + 品牌蓝 `#5d87ff` + 浅灰画布，彻底修复暗色侧栏激活态、卡片描边、亮/暗 Hover 对比度与暗色 Tooltip）；
  - 参照 `.migration/reference/primitives.tsx` 与 `ResizableWorkspace.tsx` 完整构建 `src/components/ui/`：全部基础原语 + 6 大 B 端扩展组件（`DataTablePage`、`EntityDetailLayout`、`SettingsSectionLayout`、`FormModal`、`FormDrawer`、`DateRangePicker`、`CodeEditor`、`DiffViewer`、`CopyableText`、`AuthorizationTooltipButton`）；
  - 强约束：圆角锁死 `6px/10px/999px`、高度锁死 `32/28/24px`、弹窗/抽屉强制三段式 Header 底线 + Footer 顶线 + `480/600/760px` 三档 + `label-top` 默认排版。
- **SA-ShellAuth（独占 `mazbri-web/src/stores/**`、`mazbri-web/src/components/layout/**`、`mazbri-web/src/domains/auth/**`）**：
  - 实现 Zustand Stores：`auth`、`theme`（亮/暗切换无闪烁）、`locale`（U4 无刷新双语热切，不销毁页面状态）、`mfa-step-up`、`lockscreen`、`watermark`；
  - 实现 `AppShell`（深海蓝侧栏按 RBAC 三角色过滤菜单 + 顶栏面包屑/锁屏/双语/亮暗/个人安全抽屉/退出 + 全局 `MfaStepUpHost` + `LockScreen` + `WatermarkOverlay`）；
  - 实现完整登录域（`BrowserRouter`）：口令登录（通过构建开关 `VITE_LOGIN_SLIDER_ENABLED=false` 合法关闭纯前端装饰滑块，严禁写自动化滑块破解逻辑）、首次登录强制改密、强制 MFA 绑定（二维码 + 备用码）、TOTP 登录验证、403/404 异常页。

### 三、本阶段必过门禁与首次实机热替换闭环
1. Coordinator 总装 `src/router.tsx`（为后续域预留干净占位路由）与 `src/App.tsx`；
2. 运行 `node .migration/gates/gate-0-env-hygiene.mjs` 与 `node .migration/gates/gate-1-static-laws.mjs`；
3. 运行 `bash .migration/gates/gate-2-local-hotswap.sh`（自动执行前端构建/单测 → Rust `mazbri-mgmt` Release 编译/单测 → 容器 `/tmp` 原子热替换 → 轻量磁盘清理，**严禁 `docker compose up -d`**）；
4. **MCP 9224 实机调测**：使用 `chrome-devtools` MCP 连接 `127.0.0.1:9224` 访问 `http://192.168.50.50:8085`，通过 `node .migration/gates/totp-helper.mjs` 生成合法动态码自动完成管理员登录 + MFA 验证，实测：登录流、侧栏/顶栏、亮/暗双模式切换（核验暗色侧栏品牌蓝与 Tooltip 暗色化）、双语无刷新热切、锁屏解锁、个人安全抽屉、`1440×900` 与 `1280×800` 双分辨率零 Console 报错；发现任何问题当场修 → 再热替换 → 再复测直至 100% 绿灯。

### 四、交付与退出（底座封板声明）
- 在 `.migration/FEATURE_PARITY_LEDGER.md` 勾选 F01/F02/U4 对应项，更新 `.migration/PROGRESS.md`；
- 按 5 段式详尽中文模板执行本地 `git commit`（严禁 `git push`）。自本提交起，`src/styles/` 与 `src/components/ui/` 正式封板，后续业务阶段严禁擅改。
```

---

## Stage 3 提示词｜控制面板域（5 页）+ 运维监控域（3 页 + 2 详情页 + 统一 ECharts 包装器）并行迁移

```markdown
你现在负责执行 Mazbri WebUI React 19 架构迁移的 **Stage 3（控制面板 + 运维监控并行迁移）**。

### 一、必读上下文
1. `AGENTS.md`、`.migration/MIGRATION_IRON_LAWS.md`、`.migration/MIGRATION_PLAN.md`、`.migration/PROGRESS.md`、`.migration/FEATURE_PARITY_LEDGER.md`、`.migration/reference/DESIGN_CONTRACT.md`
2. 查看旧版标注截图基线 `.migration/reference/audit-baseline/mazbri-ui-ux-audit-report.md` 中图 03、06、07、08、09、10、11、14、15、16、17、18，确保本阶段对应问题 100% 彻底根治。

### 二、Subagent 并行分工（拉起 2 个目录完全互斥的业务 Subagent 并行开发）
- **SA-ControlPanel（独占 `mazbri-web/src/domains/control-panel/**`）**：
  - 对照 `mazbri-web-legacy/src/views/mazbri/control-panel/` 完整迁移 5 页（严禁丢失任何既有好用功能，严禁改动已封板 `src/components/ui/`，如有原语缺口向 Coordinator 提单）：
    1. **用户管理**：`DataTablePage` + 新增用户 `FormModal` + 用户详情三段式 `Drawer`（启停/编辑资料/重置密码/重置 MFA/删除，统一按钮语义权重，核销 A-17）；
    2. **证书管理**：`DataTablePage` + 签发证书弹窗 + 证书详情三段式 `Drawer`（自动续期策略/续期/下载/吊销/PEM 三标签查看复制，理顺头部按钮，核销 A-16）；
    3. **审计日志**：`DataTablePage` +「显示审计查阅记录」+ 详情三段式 `Drawer`（原始 JSON 折叠复制），使用深琥珀色修复「拒绝」状态低对比度（核销 A-11），严格复用 `labels.ts`；
    4. **平台设置**：`SettingsSectionLayout` 等高双栏卡片 + 统一标题图标，**彻底砍掉底部 1200px 空白保存条**，保持密码策略与 MFA 独立安全提权域（U4），完整迁移平台许可 `.jws` 拖拽激活与申请导出及 `useLicenseGate`（核销 A-06、A-07）；
    5. **配置中心**：`DataTablePage` + 新建/编辑配置迁移为右侧宽 `FormDrawer`（CodeMirror 编辑器，核销 A-15）+ 版本历史与 Diff 回滚抽屉 + U1 级 MFA 提权护航（取消 MFA 严禁弹出可保存空白框）。
  - 导出 `src/domains/control-panel/routes.tsx`。

- **SA-Ops（独占 `mazbri-web/src/domains/ops/**`、`mazbri-web/src/components/charts/**`）**：
  - 先封装统一图表组件 `src/components/charts/EChartBox.tsx` 与 `ChartCard.tsx`（内置 `ResizeObserver`、亮/暗主题同步、强制 `UTC+8` 时区格式化（U2）、同排图表无流量空态一致表现）；
  - 对照 `mazbri-web-legacy/src/views/mazbri/ops/` 完整迁移：
    1. **调用链路 & 链路详情**：客户调用/上游调用/分布式链路三视图，快捷筛选芯片与搜索栏收进主白卡 Header（核销 A-08），统一时间窗与自动刷新，完整迁移 Trace 瀑布图详情页；
    2. **告警中心**：告警事件（含排障跳转）、告警规则（补齐筛选搜索、**新建/编辑规则从 800px 弹窗迁入右侧 `FormDrawer`**，核销 A-14）、告警静默与抑制（统一按钮位置，核销 A-09；静默时间窗严格 `UTC+8`）；
    3. **运行指标 & 服务详情**：微服务健康拓扑（彻底消灭 3 处 `ops.components.statusName.*` 裸露 Key，核销 A-03）、4 维 Tab、实例健康概览修复数值列对齐断层、服务详情补齐左上服务页头并统一 RED/USE 18 图表空态（核销 A-10）；
    4. **清理 `/dashboard` 孤岛**：按破坏性重构铁律，将原隐藏全局总览正式评估并入运维监控首项或彻底下线（若下线则前后端同步清理不留死路由）。
  - 导出 `src/domains/ops/routes.tsx`。

### 三、Coordinator 总装、门禁与 MCP 9224 实测闭环
1. Coordinator 将两个域的 `routes.tsx` 挂载进 `src/router.tsx`，合并增量词条（确保中英 100% 对齐）；
2. 运行 `node .migration/gates/gate-0-env-hygiene.mjs` 与 `node .migration/gates/gate-1-static-laws.mjs`；
3. 运行 `bash .migration/gates/gate-2-local-hotswap.sh`（本机编译 + 容器原子热替换 + 轻量磁盘清理）；
4. 运行 `node .migration/gates/gate-3-mcp-cdp-verify.mjs --stage 3`；
5. 使用 `chrome-devtools` MCP 连接 `127.0.0.1:9224` 实测 `http://192.168.50.50:8085` 上本阶段全部 8 页 + 详情页 + 抽屉，**特别实测配置中心编辑触发 MFA 提权弹窗（取消不留空白框 + 通过 `totp-helper.mjs` 填码正常打开）**，以及双分辨率 + 亮暗切换；有缺陷立刻回派对应 Subagent 修复 → 重热替换 → 复测直至 100% 绿灯。

### 四、交付与退出
- 更新 `.migration/FEATURE_PARITY_LEDGER.md` 与 `.migration/PROGRESS.md`；
- 按 5 段式详尽中文模板执行本地 `git commit`（严禁 `git push`）。
```

---

## Stage 4 提示词｜运营中心核心域全量迁移（租户 5 Tab 重构 / 账单结算 / 产品治理 / 上游供给 / 回调订阅）

```markdown
你现在负责执行 Mazbri WebUI React 19 架构迁移的 **Stage 4（运营中心核心域并行迁移）**。

### 一、必读上下文
1. `AGENTS.md`、`.migration/MIGRATION_IRON_LAWS.md`、`.migration/MIGRATION_PLAN.md`、`.migration/PROGRESS.md`、`.migration/FEATURE_PARITY_LEDGER.md`、`.migration/reference/DESIGN_CONTRACT.md`
2. 查看旧版标注截图基线 `.migration/reference/audit-baseline/mazbri-ui-ux-audit-report.md` 中图 01、02、04、05、13、19、20，确保所有运营中心历史缺陷在本阶段彻底清零。

### 二、Subagent 并行分工（拉起 2 个目录互斥业务 Subagent 并行开发）
- **SA-TenantsBilling（独占 `mazbri-web/src/domains/tenants/**`、`mazbri-web/src/domains/billing/**`）**：
  - **租户列表 & 租户详情（破坏性重构为 5 个高内聚 Tab，核销 A-01、A-19、A-20）**：
    - 列表页使用 `DataTablePage`（卡内工具栏、CSV 批量导入导出、0 选中隐藏/禁用批量删除）；
    - 详情页使用 `EntityDetailLayout`，将旧 8 Tab 重组为 5 组，**100% 保留全部业务能力**：
      1. `概览 360`（KPI + 合理比例错误分布，修复单条数据拉满突兀问题）；
      2. `基础与凭据`（基本信息修改 + API 密钥轮换/续期/吊销）；
      3. `接入与安全策略`（IP 白名单导入导出增删 + 代理缓存 TTL/上限/预热/接口差异 + 限流套餐模板单排不折行/突发/接口覆盖/应急运维 + 传输加密关闭/请求/双向/推荐与国密自定义/Legacy 弱算法警告/密钥版本新建与轮换/生效接入契约摘要）；
      4. `已开通产品`（开通/修改单价与熔断/删除/跳转全局授权）；
      5. `账户与流水`（充值/退款 MFA 提权，**严格守住 U2 元↔分换算**，用标准次按钮/深琥珀替代低对比度浅黄「退款」按钮，保留交易类型筛选与余额变化流水）。
  - **账单结算（核销 A-02、A-13）**：
    - `经营概览`：补全 4 KPI 图标（消灭空白浅蓝块）、统一趋势图 Y 轴格式化器（消灭 9,000.00 突切 12k）、优化单日柱图、保留三排行与余额可用天数预警；
    - `账单列表`：生成账单、状态流转、调账、导出；
    - `用量对账`：对账日历、立即对账、差异处置；**`对账策略设置` 弹窗用 `FormModal` + `label-top` + `whitespace-nowrap` 彻底根治文字折行压盖（P0 A-13）**。
  - 导出 `src/domains/tenants/routes.tsx` 与 `src/domains/billing/routes.tsx`。

- **SA-ProductsSupply（独占 `mazbri-web/src/domains/products/**`、`mazbri-web/src/domains/upstream-supply/**`、`mazbri-web/src/domains/callbacks/**`）**：
  - **产品治理 & 产品详情（核销 A-05）**：产品目录（保留产品编码自动预留与关闭释放机制）、授权/订阅管理、产品详情使用 `EntityDetailLayout` 统一页头与带标题卡片（基本信息/路由规则/已授权租户）；
  - **上游供给（核销 A-04）**：供应商列表/详情（消灭 `common.actions.edit`、与产品/租户详情页头完全统一）、通道列表（新建/详情/编辑/删除）、草稿箱、**4 步微服务接入向导迁移为右侧三段式 `FormDrawer`**（保留全部 4 步校验）；「免代码接入」按钮先指向 `/operations/upstream-supply/workbench` 占位（Stage 5 填充）；
  - **回调订阅**：订阅配置（含密钥随机生成、导入导出、新建编辑删除）与投递任务列表。
  - 导出三个域各自的 `routes.tsx`。

### 三、Coordinator 总装、门禁与 MCP 9224 实测闭环
1. Coordinator 将 5 个域路由挂载进 `src/router.tsx`，合并增量词条；
2. 运行 `node .migration/gates/gate-0-env-hygiene.mjs` 与 `node .migration/gates/gate-1-static-laws.mjs`；
3. 运行 `bash .migration/gates/gate-2-local-hotswap.sh`（本机编译 + 容器原子热替换 + 轻量磁盘清理）；
4. 运行 `node .migration/gates/gate-3-mcp-cdp-verify.mjs --stage 4`；
5. 使用 `chrome-devtools` MCP 连接 `127.0.0.1:9224` 实测运营中心全页面 + 弹窗/抽屉，**重点核验**：租户 5 Tab 全览、账户充值弹窗（打开检查元/分换算展示与 MFA 流程，测试完取消不污染账本）、`对账策略设置` 弹窗零折行重叠、`经营概览` 4 图标与 Y 轴、`供应商详情` 无裸露 Key、`微服务接入` 右侧抽屉、亮暗双模式与双分辨率；发现任何缺陷立刻回派修复闭环。

### 四、交付与退出
- 更新 `.migration/FEATURE_PARITY_LEDGER.md` 与 `.migration/PROGRESS.md`；
- 按 5 段式详尽中文模板执行本地 `git commit`（严禁 `git push`）。
```

---

## Stage 5 提示词｜免代码接入工作台（`upstream-connectors`）独立重构

```markdown
你现在负责执行 Mazbri WebUI React 19 架构迁移的 **Stage 5（免代码接入工作台重构）**。

### 一、必读上下文
1. `AGENTS.md`、`.migration/MIGRATION_IRON_LAWS.md`、`.migration/MIGRATION_PLAN.md`、`.migration/PROGRESS.md`、`.migration/FEATURE_PARITY_LEDGER.md`、`.migration/reference/DESIGN_CONTRACT.md`
2. 检查 Stage 1 已平移至 `mazbri-web/src/domains/workbench/engine/` 的 10 个纯 TS 核心引擎文件（4,144 LOC）与 4 个引擎单测；对照 `mazbri-web-legacy/src/views/mazbri/upstream-connectors/` 的 33 个 `.vue` 面板确保功能 100% 保真；查看 `.migration/reference/audit-baseline/images/12-upstream-workbench.png`（核销 A-12）。

### 二、Subagent 并行分工（独占 `mazbri-web/src/domains/workbench/**`，严禁改动主站已封板原语）
- **SA-WB-Core（`workbench/store/**`、`workbench/layout/**`、`workbench/tree/**`）**：
  - 基于 Stage 1 纯 TS 引擎搭建工作台 Zustand Store（严禁重造校验/序列化引擎）；
  - 使用 `src/components/ui/ResizableWorkspace.tsx` 构建全屏三段可拖拽工作区，**100% 绑定 `globals.css` 语义 Token、圆角（`6px`/`10px`）与按钮高度（`28px`/`32px`）**，彻底融入主站亮/暗双模式；
  - 实现顶栏与左侧分组接口树（搜索、分组折叠、新增/复制/删除接口、方法着色徽标、未保存圆点），**消除旧版右上角与左下角重复出现的「全局环境 (0)」入口**（核销 A-12）。
- **SA-WB-Editor（`workbench/editors/**`）**：
  - 实现请求地址栏（HTTP 方法切换、URL 输入、准入状态徽标、协议类型、发送按钮）与全部 8 个编辑 Tab：参数、请求体（CodeMirror）、请求头、鉴权、出站规格、断言、提取变量、连接与 TLS。
- **SA-WB-Runtime（`workbench/runtime/**`、`workbench/dialogs/**`）**：
  - 实现底部/右侧试调响应面板（状态码、耗时、响应体高亮、断言结果、变量提取预览）、全局环境变量抽屉、准入校验面板、导入/导出弹窗、草稿保存与正式发布流、快捷键面板与快捷键绑定（`Ctrl/Cmd+Enter` 发送、`Ctrl/Cmd+S` 保存草稿）。

### 三、Coordinator 总装、门禁与 MCP 9224 实测闭环
1. Coordinator 组装 `src/domains/workbench/routes.tsx` 并挂载进 `src/router.tsx`；
2. 运行 `node .migration/gates/gate-0-env-hygiene.mjs`、`node .migration/gates/gate-1-static-laws.mjs` 及工作台全部纯 TS 引擎单测；
3. 运行 `bash .migration/gates/gate-2-local-hotswap.sh`（本机编译 + 容器原子热替换 + 轻量磁盘清理）；
4. 运行 `node .migration/gates/gate-3-mcp-cdp-verify.mjs --stage 5`；
5. 使用 `chrome-devtools` MCP 连接 `127.0.0.1:9224` 从「上游供给 → 免代码接入」及「供应商详情 → 工作台」两个入口实测工作台全链路：接口树增删、8 个编辑 Tab 切换、环境变量编辑、准入校验、亮/暗模式视觉一致性与 `1440×900` / `1280×800` 拖拽无溢出错位；有缺陷立刻闭环修复。

### 四、交付与退出
- 勾选 `.migration/FEATURE_PARITY_LEDGER.md` 中 F12 与 A-12，更新 `.migration/PROGRESS.md`；
- 按 5 段式详尽中文模板执行本地 `git commit`（严禁 `git push`）。
```

---

## Stage 6 提示词｜全站 QA 深度回归与 20 项审计缺陷 100% 核销闭环

```markdown
你现在负责执行 Mazbri WebUI React 19 架构迁移的 **Stage 6（全站深度 QA 回归与缺陷清零）**。

### 一、必读上下文
1. `AGENTS.md`、`.migration/MIGRATION_IRON_LAWS.md`、`.migration/PROGRESS.md`、`.migration/FEATURE_PARITY_LEDGER.md`
2. 完整对照 `.migration/reference/audit-baseline/mazbri-ui-ux-audit-report.md`（20 张旧版标注图）。

### 二、全维度核验与回派修复（SA-QA 只读巡检 + 按域回派 Subagent 定点修复）
1. **自动化全量硬门禁**：
   - 运行 `node .migration/gates/gate-0-env-hygiene.mjs`
   - 运行 `node .migration/gates/gate-1-static-laws.mjs`
   - 运行 `node .migration/gates/gate-3-mcp-cdp-verify.mjs --full`（全站全部路由 × 全部子 Tab × 全部非破坏性弹窗/抽屉 × 双分辨率 `1440×900` & `1280×800` × 亮/暗双模式 × 真实鼠标 Hover 对比度），必须达到 **0 FAIL**；
2. **MCP 9224 三角色与核心写链路实机核验**：
   - 使用 `chrome-devtools` MCP 连 `127.0.0.1:9224` 访问 `http://192.168.50.50:8085`（结合 `totp-helper.mjs` 自动填写 MFA），逐一实测：
     - 三角色（`R_ADMIN` / `R_FINANCE` / `R_VIEWER`）侧栏菜单隔离、直达越权路由 403 拦截、无权限按钮提示；
     - 关键交互闭环：MFA 提权取消（U1 不留空白框）与通过、中英双语无刷新热切不丢未保存表单（U4）、东八区时间展示一致（U2）、真服务端分页切换（U3）、锁屏解锁、水印开关、证书 PEM 复制、配置 Diff、租户 5 Tab 与工作台进入退出；
3. **逐条核销账本**：对照 `.migration/FEATURE_PARITY_LEDGER.md` 全部 4 条公理 + 12 大功能特性 + 20 项旧图缺陷（A-01 ~ A-20），任何未达标项立刻按目录所有权派发对应域 Subagent 修复 → `gate-2-local-hotswap.sh` 热替换 → 重新执行 `gate-3` 与 MCP 复测，直到 **100% 全部打勾 ✅**。

### 三、交付与退出
- 确认 `.migration/FEATURE_PARITY_LEDGER.md` 无任何未勾选项，更新 `.migration/PROGRESS.md`；
- 按 5 段式详尽中文模板执行本地 `git commit`（严禁 `git push`）。
```

---

## Stage 7 提示词｜破坏性清场 + 深度清盘 + 永久 `AGENTS.md` 切换 + 销毁 `.migration/`（迁移铁律正式失效）

```markdown
你现在负责执行 Mazbri WebUI React 19 架构迁移的 **Stage 7（终结退场阶段）**。本阶段结束后，全部临时迁移铁律将物理失效，仓库进入干净的长期运行态。

### 一、必读上下文
1. `AGENTS.md`、`.migration/MIGRATION_IRON_LAWS.md`、`.migration/PROGRESS.md`、`.migration/FEATURE_PARITY_LEDGER.md`、`.migration/POST_MIGRATION_AGENTS_SECTION3.md`

### 二、严格按序执行终结退场（单 Agent）
1. **核对前置完成度**：确认 `PROGRESS.md` 中 Stage 1~6 全部 `✅ DONE`，且 `FEATURE_PARITY_LEDGER.md` 100% 勾选；
2. **清理旧版遗留目录**：彻底删除 `mazbri-web-legacy/`（连同其旧 `node_modules` 一并回收磁盘空间）；
3. **处理登录滑块开关**：核对 `VITE_LOGIN_SLIDER_ENABLED`（按项目最终决策移除该临时开关或保留清晰配置注释，严禁遗留死代码）；
4. **切换 `AGENTS.md` 为永久形态**：
   - 删除 `AGENTS.md` 顶部的 `<!-- migration-iron-laws:begin --> ... <!-- migration-iron-laws:end -->` 整个哨兵块；
   - 用 `.migration/POST_MIGRATION_AGENTS_SECTION3.md` 完整替换 `AGENTS.md` 原第三节（旧 Vue/Element Plus 规范）；
   - 将 `AGENTS.md` 第二节中前端构建命令更新为 React 19 版（`pnpm build` 内含 `tsc -b && vite build`），确保全文零“迁移期 / legacy / .migration / xblue-ui-reference / Element Plus / ArtBreadcrumb”残留；
5. **运行终结退场门禁**：
   - 运行 `node .migration/gates/gate-4-sunset-check.mjs`（校验零 legacy、AGENTS.md 切换合规、前端 `pnpm build && pnpm test`、后端 `mazbri-libs` 与 `mazbri-bin` 双 Workspace `cargo clippy --release --locked --workspace --all-targets -- -D warnings` 及 `cargo test --release --locked --workspace`、`ts-rs` 零漂移）；
6. **物理销毁 `.migration/`（迁移铁律正式失效）**：
   - 上述全绿后，执行 `rm -rf .migration/`，并清理 `.gitignore` 中不再需要的 `.migration/` 临时条目；
7. **最终热替换 + 深度磁盘清理 + 终态复验**：
   - 执行最终一次前端 `pnpm build` + 后端 `cargo build --release -p mazbri-mgmt` + 容器 `/tmp` 原子热替换（严禁 `docker compose up -d`）；
   - 执行深度磁盘清理：删 `mazbri-bin/target/debug`、清容器 `/tmp/mazbri-*`、`docker image prune -f`、`docker builder prune -f`、`pnpm store prune`，记录清理前后 `df -h` 磁盘回收数据；
   - 通过 `chrome-devtools` MCP 连接 `127.0.0.1:9224` 对 `http://192.168.50.50:8085` 做最终抽检确认一切正常；
8. **最终本地 Commit**：按详尽中文 Commit 模板提交本地（严禁 `git push`，严禁修改 `VERSION`/`.release-please-manifest.json`/`CHANGELOG.md`）。
```
