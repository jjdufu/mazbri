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

1. **Release 编译与容器原子热替换（防 `Text file busy` 与层回退）**
   - 前端 `pnpm build` 后，后端执行 `cargo build --release` 编译受影响的服务，并通过容器 `/tmp` 中转原子热替换进运行中的 `mazbri-max` Docker 容器：
     ```bash
     docker cp target/release/<pkg_bin> <container>:/tmp/<target_bin>
     docker exec -u 0 <container> sh -c 'chmod 755 /tmp/<target_bin> && mv -f /tmp/<target_bin> /usr/local/bin/<target_bin>'
     docker restart <container>
     ```
   - **严禁执行 `docker compose up -d` 重建容器**（会丢弃可写层并将已热替换二进制回退至镜像原版）；注意 `cert`、`auth`、`billing`、`cache`、`ratelimit`、`cbpro`、`cbpro-ext` 容器内 `/usr/local/bin/` 二进制均带 **`-service` 后缀**（如 `mazbri-cert-service`）。
2. **MCP `9223` 浏览器实测验证与“有问题必先修好再复测”红线**
   - 使用 `chrome-devtools` MCP（`127.0.0.1:9223`，访问 `http://192.168.50.50:8085`）实际访问后台页面、点击 Tab/按钮/弹窗，在双分辨率（`1440×900` / `1280×800`）、亮/暗双主题与三角色（`R_ADMIN` / `R_FINANCE` / `R_VIEWER`）下核验 DOM 排版、样式对齐、接口响应与控制台无报错。
   - **闭环红线**：一旦实测或静态门禁发现任何缺陷（切边、错位、多余并发请求、未翻译词条、控制台报错或权限异常），**必须当场修复源码 ➔ 重新编译与热替换 ➔ 重新连 MCP `9223` 复测**，循环往复直至 100% 确认零问题，严禁带病提交。
3. **本地详尽中文 Commit（仅本地提交）**
   - 每次阶段性重构或修复经实测确认零问题后，使用结构清晰、内容详尽的中文 Commit Message 在 `/workspace/rust-proj/mazbri` 本地执行 `git commit`（未获用户明确指令前禁止 `git push`，严禁手动修改 `VERSION`、`.release-please-manifest.json`、`CHANGELOG.md`）。

---

## 五、管理后台 16 步重构专项执行铁律（跨会话断点续跑协议，`[不可更改]`）

当执行管理后台 UI/UX 重构与架构收敛方案（真源目录：`/workspace/rust-proj/docs/mazbri-admin-refactor-steps/`，总览索引 `00-overview-and-index.md` + `01`～`16` 单步详单）时，无论中途暂停多少次或重开多少个新会话，均必须无条件遵守以下协议：

1. **跨会话零失忆断点自动定位（新会话启动必做第 1 步）**
   - 接手任务时第一时间执行：
     ```bash
     git -C /workspace/rust-proj/mazbri log --oneline origin/main..HEAD
     git -C /workspace/rust-proj/mazbri status -s
     ```
   - 根据已提交 Commit 标题中的 `[Step XX/16]` 锚点识别最新已完成闭环的步骤；若工作树存在未提交改动，先对照该半途步骤文档补完“编译门禁 ➔ 容器热替换 ➔ MCP `9223` 实测清零 ➔ 本地中文 Commit”；若工作树干净，紧接着读取并执行下一步 `Step (XX+1)/16`。
2. **严格按 `01 ➔ 16` 单步串行闭环，严禁跳步或混步提交**
   - 严格遵循 `01` 至 `16` 编号顺序一步一闭环（每步完整走完：`读取当步文档` ➔ `前后端实现与中英双语对齐` ➔ `pnpm build + 全仓 *.test.ts + 双仓 release clippy + 全量 cargo test + ts-rs 零漂移` ➔ `对应容器热替换` ➔ `MCP 9223 实测与缺陷清零` ➔ `本地中文 Commit`），绝不跳步、乱序抽取或攒多步才做验证提交。
3. **单步特殊跨容器热替换与运行库免清库必守项**
   - 默认改 `web`/`mgmt` 的步骤（`01/02/03/06/07/08/11/12`）热替换 `mazbri-mazbri-mgmt-1`（`mazbri-mgmt`）；涉及多服务的步骤必须按 `00-overview-and-index.md` 第四节矩阵完整替换（其中 **Step 04** 换 `analytics + mgmt` 并先组 2-Tab 过渡壳；**Step 05** 换 `mgmt + auth + gateway` 且递增 `'auth_api_key_version'` 须用 `INSERT ... ON CONFLICT (key) DO UPDATE` 幂等自举；**Step 09** 换 `analytics + mgmt + cert + gateway`；**Step 10** 完整替换全部 **10 个**直连 PG 业务容器以根除 `50.0%` 假负载；**Step 11** 删 `/account/security` 时同步把 `mfa-required-hint` 跳转改为唤起 `PersonalSecurityDrawer`；**Step 13/14/15** 换 `cert + mgmt`，先重启 `cert-1` 健康后再重启 `mgmt-1`；**Step 16** 必须同步替换 `mgmt + bypass + gateway` 三容器以支持 `AuthProfile::Skip`）。
4. **单步 Commit 统一命名与详尽正文格式**
   - 标题格式：`fix(<scope>): [Step XX/16] <精准中文摘要>` 或 `feat(<scope>): [Step XX/16] <精准中文摘要>`（兼容 `mz-rt-pr` 发版流与 Release Please）；
   - 正文必须分点详述：① 本步解决的根因与前后端具体改动；② 静态检查与全量测试通过结果；③ 热替换重启的容器列表与 MCP `9223` 实测核验结论。
5. **标准 7 批次（“做第 N 批”）口令映射与边界守卫**
   - **第 1 批**：`Step 01 ➔ 02 ➔ 03`（全局基座止血）｜**第 2 批**：`Step 04 ➔ 05`（总览财务与租户 8-Tab）｜**第 3 批**：`Step 06 ➔ 07 ➔ 08`（产品、上游供给与账单对账收官）｜**第 4 批**：`Step 09 ➔ 10`（调用链路与运行指标监控收官）｜**第 5 批**：`Step 11 ➔ 12 ➔ 13`（个人安全/平台设置、用户、证书按需 MFA 与删 `fetchAllPages.ts`）｜**第 6 批**：`Step 14 ➔ 15`（审计降噪与配置中心加固）｜**第 7 批**：`Step 16`（免代码工作台与反开专场）。
   - 当用户下达 **“做第 N 批”**（或 **“继续做下一批”**）时：先查 `git log` 确认前置批次已全部提交（严禁跨批跳做；若本批已完成部分步骤则从剩余步骤续跑），在本批内严格**一步一热替换验证、一步一 `[Step XX/16]` 本地 Commit** 串行执行，**完成本批最后一步 Commit 后立即停下汇报**，不自动越界进入下一批。
