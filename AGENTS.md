# Mazbri 项目开发铁律

## 一、架构与重构原则

1. **可破坏性重构，不留无用兼容层**
   - 废弃的旧页面、旧路由辅助函数、无用组件、冗余字段或状态一律直接删除，严禁保留任何过渡性兼容层、废弃别名或死代码。
2. **前后端彻底一致（禁止假性修改前端）**
   - 凡涉及分页、搜索过滤、状态枚举、数据结构或业务功能裁剪，必须前后端同步修改（前端视图/API/TS 类型 + 后端 Handler/Service/SQL/权限矩阵/审计映射），绝不能只隐藏前端元素而留着后端冗余接口或假分页。
3. **真服务端分页**
   - 大数据量列表必须前后端联动实现真正的服务端分页（控制单页查库量与页面高度），禁止一次性查库过多或前端内存假分页。

---

## 二、编译、静态检查与测试要求（对齐 GitHub Actions CI 门禁）

1. **严格构建顺序：先前端 `pnpm build`，后后端 `cargo`**
   - `build.rs:31-36` 会将 `mazbri-web/dist` 真实打包进二进制，禁止使用 `MAZBRI_SKIP_FRONTEND_EMBED` 绕过；任何涉及前端或 `mazbri-mgmt` 的构建/检查前，必须先在 `mazbri-web` 执行 `pnpm build`（内含 `vite build && vue-tsc --noEmit`）。
2. **双 Workspace 严格 Clippy 校验（`pedantic` + `unsafe_code` + `-D warnings`）**
   - `mazbri-libs` 与 `mazbri-bin` 均开启了 `clippy::all = "warn"`、`clippy::pedantic = "warn"` 与 `rust::unsafe_code = "warn"`。
   - 改动 `mazbri-libs` 或 `mazbri-bin` 时，必须按 CI 标准通过带 `--release --locked` 的严格校验（确保锁文件无漂移、零警告）：
     ```bash
     # 若修改了 mazbri-libs
     cd mazbri-libs && cargo clippy --release --locked --workspace --all-targets -- -D warnings
     # mazbri-bin 全量（或仅改前端时至少 -p mazbri-mgmt）
     cd mazbri-bin && cargo clippy --release --locked --workspace --all-targets -- -D warnings
     ```
3. **严禁压制或跳过警告/报错**
   - 绝对禁止使用 `#[allow(...)]` 属性或任何其他方式（如 `@ts-ignore`、移除 lint 规则等）忽略、跳过编译器、Clippy 或 TypeScript 的警告和报错。
4. **前后端全量单元测试门禁**
   - 后端测试：
     ```bash
     cd mazbri-libs && cargo test --release --locked --workspace --no-fail-fast
     cd mazbri-bin && cargo test --release --locked --workspace --no-fail-fast
     ```
   - 前端测试（全仓扫描，不止于 `pnpm test`）：除了 `pnpm test`，还必须保证 CI 扫描的全仓所有 `*.test.ts` 全部通过。
5. **`ts-rs` 生成绑定与发版元数据零漂移**
   - 编译 `mazbri-mgmt` 后，`mazbri-web/src/types/api/generated` 不得出现未提交的 diff（`git diff --exit-code -- mazbri-web/src/types/api/generated`）；
   - `VERSION` 与 `.release-please-manifest.json` 版本号必须保持一致；若增删 `mazbri-bin` 服务成员，必须同步更新 `detect.py:22-34` 的 `SERVICES` 表。

---

## 三、UI 规范与公共组件铁律（`[不可更改]`）

1. **严禁页面内重复大标题与重复计数标签**
   - 页面级主标题统一由顶部导航栏的 `ArtBreadcrumb` 承载；页面内部及卡片头部禁止再渲染与菜单/面包屑同名的 `<h1>`/`<h2>` 大标题或重复的记录总数 `<ElTag>`，确保高信息密度。
2. **强制复用公共外壳与组件，禁止“各写各的”**
   - **页面外壳**：运维页统一使用 `OpsPage` + `OpsCard`（严禁在 `OpsPage` 上附加 `.art-full-height` 锁死高度）；控制面与业务表统一使用 `ControlPanelPageShell`、`ResourceTableShell`、`ResourceTablePageShell`。
   - **弹窗与抽屉**：统一使用 `EditDialog`、`CenteredDialog`、`AdaptiveDrawer`，禁止直接手写裸 `<ElDialog>` / `<ElDrawer>`。
   - **搜索框与工具栏**：搜索框统一遵循 `ExpandOnFocusInput` 规范（默认 `180px`，聚焦展开至 `260px`），工具栏表单控件统一 `32px` 默认高度（禁止混用 `24px` `size="small"` 导致高低不齐）。
   - **边距、嵌套与主题色**：详情/编辑页必须具备标准外边距（`padding: var(--ops-page-padding, 16px)`）；严禁页面壳嵌套页面壳导致双倍缩进错位；严禁硬编码亮色 Hex 背景色破坏暗黑模式，必须使用 Element Plus CSS 变量。
3. **`[不可更改]` 源码注释守卫**
   - 在核心公共组件源码中保留标记为 `[不可更改]` 的规则注释，约束后续新页面开发。

---

## 四、交付、验证与 Git 提交要求

1. **Release 编译与容器热替换**
   - 前端 `pnpm build` 后，后端执行 `cargo build --release` 编译受影响的服务（如 `mazbri-mgmt`、`mazbri-analytics`），并将二进制热替换进对应的 Docker 容器（`docker cp` ➔ `docker exec -u 0 ... mv /usr/local/bin/...` ➔ `docker restart`）。
2. **MCP 浏览器实测验证**
   - 使用 `chrome-devtools` MCP（或其他内置工具）实际访问后台页面、点击 Tab/按钮/弹窗，核验 DOM 排版、样式对齐、接口响应与控制台无报错。
3. **本地详尽中文 Commit**
   - 每次阶段性重构或修复完成后，使用结构清晰、内容详尽的中文 Commit Message 在本地提交代码。
