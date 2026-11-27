# Mazbri WebUI React 19 架构迁移期专属铁律（阶段性生效）

> **生效条件与退场自毁条款（Sunset Clause）**：
> - 本铁律**仅在 `mazbri/.migration/ACTIVE` 文件存在时生效**。
> - 当 Stage 7 全部终结门禁（`gate-4-sunset-check.mjs`）100% 通过后，按指令执行：删除 `mazbri/mazbri-web-legacy/`、删除 `mazbri/.migration/`、删除 `mazbri/AGENTS.md` 中的 `<!-- migration-iron-laws:begin/end -->` 哨兵块，并将 `mazbri/AGENTS.md` 第三节替换为 `.migration/POST_MIGRATION_AGENTS_SECTION3.md` 成稿。
> - 上述动作完成后，**本迁移铁律在物理与语义上同时永久失效**，仓库回归常规长期铁律治理。

---

## 十条不可逾越的迁移铁律

### 铁律 1｜完全脱离外部项目，只读仓库内参照包
- 所有架构、样式与原语参照**只允许读取**仓库内的 `.migration/reference/`（包含 `globals.css`、`primitives.tsx`、`ResizableWorkspace.tsx`、`DESIGN_CONTRACT.md`、`audit-baseline/`）。
- 严禁读取、修改或引用 `mazbri` 仓库之外的任何路径（尤其是外部 `xBlueNode` 目录）。

### 铁律 2｜技术栈洁净，零重型旧框架残留
- 新版 `mazbri-web` 统一采用：`React 19` + `TypeScript` + `Vite` + `Tailwind CSS v4 (@tailwindcss/vite)` + `React Router` + `Zustand` + `TanStack Query` + `TanStack Table` + `Lucide React` + `ECharts 6` + `CodeMirror 6` + `Diff2Html`。
- **严禁**引入 `vue`、`pinia`、`element-plus`、`@element-plus/*`、`art-*`、`ant-design`、`sass` 或任何 `.vue` / `.scss` 文件（由 `gate-1-static-laws.mjs` 自动阻断）。

### 铁律 3｜允许破坏性重构，不留无用历史兼容层
- 当前系统处于开发期且无外部用户，**禁止为旧版糟糕设计保留过渡兼容壳**：
  - 废除 Hash 路由（`#/...`），切换为标准 `BrowserRouter`（`mazbri-mgmt/src/main.rs` 的 `spa_fallback` 原生支持）；
  - 租户详情旧 8 Tab 碎片化结构直接重构为 **5 个高内聚分组**（见 `DESIGN_CONTRACT.md`）；
  - 平台设置彻底砍掉底部 1200px 空白保存条；告警新建规则、微服务接入、新建/编辑配置全部迁入右侧 `Drawer`；
  - 隐藏孤岛路由 `/dashboard` 必须在 Stage 3 明确归属（正式并入侧栏首项，或前后端彻底下线，禁止留暗门）；
  - Rust 侧对旧前端路径的硬引用（`audit-logs/labels.ts` 在 `mazbri-mgmt` 与 `mazbri-cert-service` 的 `include_str!` 及单测路径）在 Stage 1 直接对齐到新域目录（`mazbri-web/src/domains/control-panel/audit-logs/labels.ts`），彻底消除旧 `views/mazbri/` 目录残留。

### 铁律 4｜纯 TS / JSON 业务资产零重写复用
- 以下约 **3.3 万行零 Vue 依赖纯资产**必须从 `mazbri-web-legacy` 直接平移复用，**严禁手工重抄**（防接口字段、校验规则、加密逻辑与词条回归）：
  - `src/api/`（27 文件，100% 纯 TS）
  - `src/types/`（含 Rust `ts-rs` 自动生成目录 `src/types/api/generated/`）
  - `src/utils/`（120 个纯 TS 工具 + `http/` 内错误、MFA 状态机与单测；仅重写 `http/index.ts` 与 `http/mfa-dialog-service.ts` 两个适配桥）
  - `src/locales/`（`zh.json` / `en.json`，并立即补全审计抓获的 4 个缺失 Key）
  - `src/constants/`、`src/enums/`、`src/list-catalog/`、`labels.ts`
  - `upstream-connectors` 的 10 个纯 TS 核心引擎文件（4,144 LOC）及全部原有纯 TS 单元测试。

### 铁律 5｜不改变当前好用功能特性，死守 U1~U4 四大底层公理
- 严禁以“简化”为名砍掉或弱化现有好用的业务功能。每阶段交付必须对照 `.migration/FEATURE_PARITY_LEDGER.md` 逐项核销：
  - **U1**：`MfaStepUpCancelledError` 与网络取消严格正交，受 MFA 保护端点取消提权后严禁残留可保存空白框；
  - **U2**：金额「元 ↔ 分 ↔ 微元」严格换算，全站表格/图表/对账/静默时间窗严格绑定东八区（`UTC+8`）；
  - **U3**：真服务端分页 + 准确 `total`，高饱和告警色仅用于真实异常；
  - **U4**：保持权限提权域不降级；中/英切换无刷新热切且不销毁当前页未保存表单状态。

### 铁律 6｜视觉与组件契约强制受 `gate-1` 与 `gate-3` 约束
- 完整遵循 `.migration/reference/DESIGN_CONTRACT.md`：保留 Mazbri 品牌基因（深海蓝侧栏 `#0b1733` + 品牌蓝 `#5d87ff` + 浅灰画布），全面执行 xBlueNode 规范：
  - 圆角 3 档：`rounded-md` (6px) / `rounded-lg` (10px) / `rounded-full`；
  - 高度 3 档：`32px` / `28px` / `24px`；
  - 列表页工具栏必须收进主白卡 Header 内，0 选中时隐藏或禁用批量危险操作；
  - 弹窗/抽屉强制三段式（Header 底线 + 可滚 Body + Footer 顶线），宽度仅 `480 / 600 / 760px`，表单默认 `label-top`；
  - 暗色模式必须保留品牌蓝侧栏激活态、清晰卡片描边、高对比 Hover 与暗色 Tooltip；
  - 严禁 `@ts-ignore`、`@ts-expect-error`、`eslint-disable` 或跳过警告。

### 铁律 7｜MCP 实机自测、MFA 自动填写与登录滑块合规规程
- **MCP 调测目标**：所有带页面的阶段（Stage 2 ~ Stage 6）必须使用 `chrome-devtools` MCP 连接 `127.0.0.1:9224`（支持 `CDP_PORT` 覆盖）对 `http://192.168.50.50:8085` 进行实机检查调测，并跑通 `gate-3-mcp-cdp-verify.mjs`。
- **登录滑块处理（合规红线）**：
  - 严禁使用任何自动化脚本模拟滑动、破解或绕过人机验证滑块；
  - 经核验旧版 `<ArtDragVerify>` 仅是纯前端装饰组件（后端登录接口无滑块校验）。迁移期统一在 React 登录页实现构建开关 `VITE_LOGIN_SLIDER_ENABLED`（默认 `false`），在迁移自测期间**合法关闭纯前端装饰滑块**；真实后端防爆破、强制改密与 TOTP MFA 校验全程启用。
- **MFA 自动填写规程**：
  - 严禁绕过后端 MFA 校验、严禁将任何密码或 TOTP Secret 写入代码或提交进 Git；
  - 本地测试凭据统一存放在已被 `.gitignore` 忽略的 `.migration/.env.local` 中，通过 `.migration/gates/totp-helper.mjs` 按标准 RFC 6238 算法实时计算 6 位动态码，在 MCP 遇到登录 MFA 或敏感操作 `MfaStepUpModal` 时自动填入验证。

### 铁律 8｜Subagent 并行协作互斥所有权协议
- Stage 2 ~ Stage 6 的并行 Subagent 必须严守目录独占边界（详见 `MIGRATION_PLAN.md`）：
  - 底座封板（Stage 2 结束）后，业务 Subagent **严禁擅自修改** `src/styles/`、`src/components/ui/`、`src/components/layout/`、`src/router.tsx`、`package.json`；
  - 若发现原语缺口，向当前阶段 Coordinator 提单统一扩展，严禁在业务域私造按钮或覆写全局 CSS；
  - 各业务域仅在自己目录下导出 `routes.tsx` 与域级词条增量文件，由 Coordinator 统一挂载，避免合并冲突；
  - 每轮容器热替换与 `gate-3` 巡检由 Coordinator / QA Subagent 单点执行，禁止多 Subagent 并发抢占 `cargo` 锁或重复重启容器。

### 铁律 9｜本机编译 + 容器原子热替换 + 科学磁盘清理（禁用 GitHub CI）
- 开发迁移阶段**禁止触发 GitHub Actions CI**，禁止在未获用户明确指令时 `git push`；
- 构建热替换严格走 `.migration/gates/gate-2-local-hotswap.sh`：
  1. 先 `gate-0` + `gate-1` + `pnpm tsc --noEmit && pnpm build && pnpm test`；
  2. 再 `cargo build --release -p mazbri-mgmt`（严禁 `MAZBRI_SKIP_FRONTEND_EMBED`）+ `cargo test --release -p mazbri-mgmt` + 校验 `ts-rs` 无脏漂移；
  3. 通过容器 `/tmp` 中转 `chmod 755 && mv -f /tmp/mazbri-mgmt /usr/local/bin/mazbri-mgmt` + `docker restart <container>` + `/health` 200 探活；**严禁执行 `docker compose up -d`**。
- **磁盘清理纪律**：
  - 每步热替换后强制执行轻量清理：清 `mazbri-bin/target/debug`、清容器 `/tmp/mazbri-*`、清 `mazbri-web/.vite` 旧缓存与过期截图、执行 `docker image prune -f` 与 `docker builder prune -f`（**保留 `target/release/deps` 以保持秒级增量编译，严禁 `docker image prune -a` 误删运行基线镜像**）；
  - Stage 7 终结时执行深度清场：删除 `mazbri-web-legacy/`（含其旧 `node_modules`）、删除 `.migration/`、执行 `pnpm store prune`。

### 铁律 10｜闭环零容忍 + 每步详尽本地 Commit + 进度交接
- **缺陷闭环**：任一门禁（`gate-0`~`gate-4`）或 MCP 实测发现缺陷（报错、切边、错位、重叠、未翻译词条、Hover 无感、权限泄漏），必须当场修复 → 重新热替换 → 复测直至 100% 绿灯，严禁带病进入下一步。
- **本地详尽中文 Commit**：每完成 1 个阶段必须通过 `gate-0-env-hygiene.mjs`（防 `.env.local` 误提交），按以下 5 段式中文模板本地提交，并同步更新 `.migration/PROGRESS.md`：
  ```text
  refactor(webui): [Stage X] <阶段核心成果>

  变更范围：
  - <目录/模块/组件清单与破坏性重构说明>

  功能特性保真（对照 FEATURE_PARITY_LEDGER）：
  - <本阶段核销的好用功能与 U1~U4 不变量>

  门禁与 MCP 9224 实测结果：
  - gate-0/1/2/3 执行结果、单测数量、分辨率(1440x900 & 1280x800)、亮暗主题、MFA 链路

  磁盘与环境清理：
  - <清理项与 df -h / du -sh 快照>
  ```
