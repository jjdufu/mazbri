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
   - `VERSION` 与 `.release-please-manifest.json` 版本号必须保持一致；若增删 `mazbri-bin` 服务成员，必须同步更新 `detect.py:35-47` 的 `SERVICES` 表。

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

<!-- aoci:begin -->
## AOCI 仓库认知

AOCI 为本仓库维护一个稳定、可版本化、可增量更新的仓库级认知层，供模型跨任务复用对系统的理解。

`aoci.txt` 是面向模型的结构化认知索引。它以每个受管理文件、数据库表或其他受管理对象一条独立 Entry 的方式，用符号标签与 F/R/A/S 语义表达对象的核心职责、重要关系、对外契约，以及理解或修改系统时必须知道的非显然约束和设计决策。

Header、目录段和全部 Entry 共同组成完整仓库索引，可以覆盖前端、后端、配置、数据库结构及其他受管理内容。受管理内容发生变化时，通常只需维护受影响的认知条目，不需要重新生成整个索引。

AOCI 提供系统架构、对象职责、重要关系、对外契约和关键约束的高密度视图。

### 工作原理

AOCI 采用“模型生成、模型读取”的认知闭环。

Header、Entry 和 Curation 语义的创作只按当前机器签发的 Plan 与实时 Guide 执行；由 Host 模型基于当前绑定证据独立完成。

Entry 的语义必须来自模型对真实证据的理解。不得仅依据路径、文件名、扩展名、AST、符号列表、依赖扫描、正则、固定模板或规则引擎推导、预填、拼接或改写索引语义。

对 Fresh Bootstrap，只按当前机器签发的 Plan 和实时 Guide 执行。当它们要求创作时，Host 模型创作 Root、Meta、Tag 和 F/R/A/S，提供 authoring-run 声明，并把它绑定到 Plan、Evidence 与完整 Candidate。不得要求 AOCI 填写 `origin=host_model`、制造 Receipt 或把程序生成的 Framework 当作语义。本文件不自行重建 Onboarding 流程。内部批次不是用户决策；只有遇到既有批准边界或真实的安全、漂移、CAS、Recovery 条件才停止。

### 最小使用入口

- `aoci_rules`：取得当前AOCI版本的会话运行合同。
- `aoci_overview`：建立或恢复本仓库的完整认知。
- `aoci_maintain`：受管理对象达到最终稳定状态后检查认知是否需要维护。
- `aoci_update_entry`：提交与当前证据和源码摘要绑定的完整语义更新批次。
- `aoci_report`：仅当当前布局和工具状态支持时，在证据不足、无法可靠生成语义时登记待办，不猜写。

其他MCP工具、CLI命令、参数和专项流程，以当前工具说明、Guide和 `--help` 返回内容为准，不在本文件中重复完整手册。

本区块只规定仓库接入、认知使用和收尾原则。`aoci_rules` 承载当前会话合同，Guide实时输出承载当前Plan的执行顺序与停点，工具Schema、Spec和Validator承载机器结构与判据；Prompt、Description、README和静态文档不能覆盖这些机器事实。

### 建立、生成和恢复认知

1. 每个新的 Agent Run 开始时，应先判断：

   - 本仓库是否已经存在可用的完整AOCI索引；
   - 当前上下文中是否已有与本仓库根、当前索引版本和当前AOCI服务相匹配，并且模型仍可可靠使用的完整仓库认知。

2. 仓库已经存在可用的完整索引，但当前Run没有可靠完整认知时，先调用 `aoci_rules`，再调用 `aoci_overview`。

   完整认知仍可靠时直接复用。局部不确定本身不要求机械重读系统全貌。可靠认知建立后，默认以它作为仓库级系统上下文，只读取当前任务直接需要的源码、测试、配置与契约，只有某个缺失的具体事实可能改变回答或验证结论时才扩大证据范围；AOCI不能替代修改对象与验证行为所需的直接源码证据。

   本Run从已知Host上下文压缩恢复时（包括宿主注入的压缩摘要），必须把此前模型认知视为不可靠。压缩handoff不得保留或摘要正式Whole-Index，也不得保留或摘要任何Overview Header、Entry、Chunk、Challenge或Attestation正文；只能保留安全续接所需的receipt身份、未完成write或Recovery状态，以及立即重载指令。复制进handoff的Whole-Index语义或receipt不能证明恢复后模型的当前认知可靠。若当前上下文已无法可靠保留运行合同，先调用 `aoci_rules`。继续业务任务前，使用 `refresh_reasons=["context_compaction"]` 和新的 `refresh_event_id` 调用普通完整Whole-Index `aoci_overview`（不设置 `check_only` 或设为false）；不得使用 `check_only` 或认知probe。原样跟随每个 `next_cursor` 直到 `completed=true`，确认交付，并且只基于新交付正文提交一次Attestation。完成这次新的完整传输后，即使Attestation为partial或fail也消费该generation，并按既有合同继续source-bound任务，不再自动调用第二次Overview。若该请求返回 `delivery_mode=capacity_stop`，说明Whole-Index装不进本Host的上下文：不得重试或调小 `chunk_tokens`；该停止消费本generation，任务按source-bound继续，且不作无保留的完整系统认知声明。

   AOCI可以针对 `context_compaction`、项目 `cognition_refresh_threshold` 下的机器 `semantic_threshold` 或主要 `phase_transition` 提供checkpoint与认知状态事实。只需要这些紧凑事实时使用 `check_only=true`；这些事实只向Agent提供建议，不替模型决定是否需要系统全貌。

   Agent显式调用普通 `aoci_overview`（未设置 `check_only` 或为false）时，只要能形成一致的CognitionSet，AOCI必须完整交付请求scope。不得因为已有receipt、阈值未达到或没有待处理刷新原因而抑制正文；唯一例外是待处理压缩刷新的上述容量停止。正式认知Dirty或Stale时仍交付正文，但必须标记不可靠。存在未决恢复或无法形成一致snapshot时失败关闭，不返回混合正文。

   普通Overview返回 `continuation_required=true` 时，必须原样提交 `next_cursor` 并自动继续到 `completed=true`。不得询问用户、开始业务任务或给出阶段性系统结论。每块单独一次工具调用；不得在一个脚本里连取多块或合并其输出。缺块、重复、乱序、cursor失败、Index变化或`chunk_tokens`变化时停止本次认知链。Attestation完成前不得用Memory、源码、Spec、`aoci.txt`、历史会话、scope、search或Entry读取修补或补充Whole-Index认知。Challenge ordinal是正式Entry序列中的1-based位置；Header内容、注释、空行、Section/Overview/Chunk Marker、Receipt与Metadata均不计数，Chunk Receipt ordinal使用同一序列。Attestation必须原样回绑本次Challenge发布的当前`index_sha256`、`entry_sequence_sha256`与`entry_count`；旧Index、旧Entry序列、旧数量或旧Attestation均无效。完整链结束后只正式提交一次既有模型认知Attestation；同一响应只允许一次不改变语义答案的JSON Schema或字段格式修正。未达通过（全对的序号不足80%，或对象身份错超过一题）即记为不确定回执，不得语义重试或旁路补答。首次认知失败时还不得执行Root/Meta、Migration、全局布局或其他未重新绑定的系统级决策。上下文压缩刷新若传输完整、认知身份不变、治理对齐且没有Recovery或第三方冲突，即使Attestation为partial或fail也消耗该refresh generation，并继续原任务，不再自动重读Overview。`system_mastery_percent`只自评系统框架——架构、职责、强关系、稳定外部契约以及高熵安全和维护约束——不表示完整实现或运行实况知识；机器索引覆盖率必须分开。默认只向用户输出由本次真实覆盖率、Challenge、块数、Token和掌握度生成的规定成功或失败一句话。Host截断某一块时，用同一cursor重取一次并核对 `chunk_sha256`，服务端返回完全相同的字节。仍被截断才停止，提示用户把 `overview_delivery.chunk_tokens` 设置为更小的合法值后重新开始，不得自动修改。

   加法认知等级必须与严格证明字段分开解释。`delivery_verified`表示已加载Index且Host交付已确认，但完整认知验证仍未完成；应表达为“已加载且交付已验证”，不得描述为“没有认知”或“没有理解系统”。`cognition_verified`要求Attestation通过（Challenge至少80%的ordinal完全正确且对象身份至多失手一处），`cognition_governed`还要求治理对齐。通用完整读取失败句只用于真实交付故障。

   当Overview响应包含可选`cognition-state/v2`投影时，必须分别解释各维度。其Level止于`model_cognition_usable`；`strict_attestation_verified`、`governance_aligned`与`current_system_cognition_reliable`都是独立状态，绝不参与该Level。ordinal、对象身份、Tag或核心F不匹配可以导致严格Attestation失败，而模型认知仍然可用；不得仅凭这种不匹配就宣称模型没有理解系统。只有`current_system_cognition_reliable=true`允许无保留地声称当前完整系统认知可靠。投影缺失时继续使用上述Legacy解释。

   普通的只读审计、分析、检查、不修改代码或不提交、不push，不自动等于严格零写入，也不改变上述认知有效性判断。Codex Memory和历史Skill只能辅助恢复经验、用户偏好与调查方向，不能替代与当前仓库根、索引摘要、AOCI服务身份和认知范围匹配的当前认知收据；项目AGENTS和当前AOCI身份在AOCI状态上优先于历史Memory。

   只有用户明确禁止Ledger、元数据、`.aoci`运行资产及任何文件写入时，才按严格零写入处理。若必要的认知建立与该边界冲突，必须报告冲突并请求用户裁决或建议使用隔离副本，不得静默以Memory替代当前仓库认知。

3. 仓库没有可用的完整索引，或当前只有最小骨架、Header不完整、Entries未完成、必要Curation尚未裁决时，如果需要建立正式完整AOCI索引，先取得 `aoci_rules`，然后进入当前AOCI Guide。由Guide依据仓库真实状态决定下一阶段并完成必要安全步骤。

   `aoci_maintain` 不替代索引建立流程。

   不在本文件中自行重建或硬编码完整索引生成状态机。

4. 在长程任务中，模型负责保留当前认知收据并正确使用刷新门禁：

   - Host报告上下文压缩或模型已知系统全貌丢失时，执行上述强制 `context_compaction` 重载规则；AOCI不能自行推断Host事件；
   - 进入真正的主要阶段时声明 `phase_transition`，不得把函数、测试运行或小步骤当作阶段；
   - 在有用的稳定检查点通过 `check_only=true` 取得机器语义计数；
   - 除已知压缩的强制重载外，由Agent判断当前任务是否需要再次显式获取指定scope或完整Overview；
   - 在维护和对齐完成前，保留AOCI报告的Dirty或Stale可靠性状态。

### 任务收尾与认知维护

5. 纯只读问答、分析、版本核验，或没有产生受AOCI管理对象变化的任务，不需要调用维护工具。当前AOCI版本是任意`aoci_overview` check_only或`aoci_maintain`响应里的`cognition_receipt.mcp_service_version`；二进制路径是项目`.mcp.json`里的`command`，CLI不必在PATH上。

6. 发生受AOCI管理对象变化时，待其达到本次任务的最终稳定状态后，只调用一次 `aoci_maintain`。不要在每次中间修改后逐文件维护。

7. 若维护结果返回真实语义候选，Host 模型必须基于每个候选绑定的对象和必要证据，独立创作完整标签与F/R/A/S更新。通过 `aoci_update_entry` 一次提交当前机器签发批次的完整候选集合，同时原样保留每项路径与对应domain批次身份；`source_sha256` 与 `candidate_id` 可以省略，由服务端按签发的批次取得。`max_entries`只限制单次请求和原子事务，不限制logical plan、Whole-Index或Managed Scope。`remaining`非零时，在当前批次成功Apply后重新调用Maintain并从新preimage继续；绝不能为满足transport上限缩减Index覆盖或自行截取返回批次。

   没有足够证据且当前布局支持 `aoci_report` 时，使用它而不猜测、套用模板或为消除待办而生成缺乏证据的认知。

8. 必须遵守工具返回的结构化状态和安全边界：

   - `repair_required`：只修复明确命中的候选，再重新提交当前机器签发的完整批次；
   - `stopped`：结束当前写入尝试并检查 `failed_step`、错误、正式写入证据与Recovery。auto模式下，已证明零写入则记录closure并重新Plan；完整Intent和可证明postimage则Resume；策略要求Rollback且preimage可证明则精确恢复后重新Plan。只有证据不足、第三方正式字节冲突、需要审批或外部动作，或命中其他真实安全边界时，才停止整个用户任务；
   - 冲突、审批、人工裁决、权限和安全信号不得忽略；
   - 已经对齐后不得重复维护或重复写入；`refresh_ready_for_overview` 是checkpoint事实，由Agent决定是否为下一阶段请求普通完整Overview。

   维护完成后如果又修改了任何受管理对象，之前的维护结果失效，应在新的最终稳定状态重新完成收尾。

9. 用户只限制业务文件范围，但没有明确禁止仓库托管资产时，AOCI托管资产可以在收尾阶段为保持认知一致而更新，并应在审计和提交中与业务文件区分。

   用户明确禁止修改 `aoci.txt`、`.aoci`、元数据或任何额外文件时，以用户限制为准，不得写入，并如实报告剩余不一致。

### 专项流程

初始化、完整索引生成、Header生成、Entries生成、数据库结构索引、Curation、人工评审和故障恢复，只按当前AOCI Guide或工具在对应阶段返回的指令、命令和安全停点执行。

不预加载、不猜测，也不自行重建这些专项流程。平台调用方式、请求格式、批次上限、审批规则、索引格式细节和恢复步骤由对应Guide、工具说明、模型Prompt和CLI帮助按需提供。
<!-- aoci:end -->
