# Step 01：Step-up MFA 前置能力门卫、并发取消解耦与两阶段登录审计正名

## 1. 覆盖问题清单与源码精确定位

| 编号 | 关联审计项 | 级别 | 问题描述与核验纠偏 | 精确源码位置 |
| :--- | :---: | :---: | :--- | :--- |
| **W01-1** | `G-03` | P0 | `MfaStepUpCancelledError` 定义了 `readonly cancelled = true`，被 `useLatestRequest` 的 `isRequestAbortedError` 误判为网络请求中止并静默返回 `undefined`，导致 `ConfigEditDialog` 与 `HistoryDrawer` 的异常回滚分支永不触发，取消 MFA 后滞留空白可保存 TOML 编辑器与错误初始版本提示；同时 `CertificateDetailDrawer` 的私钥解锁 `unlockRunner` 存在同源死代码分支 | [useLatestRequest.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/composables/useLatestRequest.ts#L21-L34)、[mfa-dialog-service.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/utils/http/mfa-dialog-service.ts#L16-L27)、[ConfigEditDialog.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/config-center/modules/ConfigEditDialog.vue#L276-L331)、[HistoryDrawer.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/config-center/modules/HistoryDrawer.vue#L243-L294)、[CertificateDetailDrawer.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/certificates/modules/CertificateDetailDrawer.vue#L545-L615) |
| **W01-2** | `D-04` | P0 | **后端安全审计语义失真（纠偏原审计仅改前端展示的建议）**：`/auth/login` 与 `/auth/mfa/verify` 在路由表均映射为 `AuditAction::UserLogin`（`"user.login"`）；当开启 MFA 时，`admin_login` 仅签发受限 `mfa_pending` 票据返回 HTTP 200，审计中间件直接将其落库为 `登录 · 成功`，导致单次登录产生同秒两条同名记录，且二阶段 MFA 失败时仍残留首阶段「登录成功」伪记录 | [route_map.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/audit/route_map.rs#L130-L153)、[auth_flow.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/admin/auth_flow.rs#L597-L601)、[types.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/audit/types.rs#L34)、[service.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/audit/service.rs#L91)、[labels.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/audit-logs/labels.ts#L26) |
| **W01-3** | `D-04` | P1 | 审计日志工具栏混排 `180px` 搜索框、表头专用的 `36×36px` 无边框透明日历按钮与裸露在灰底上的复选框，违背工具栏 `32px` 等高与容器边界一致规范 | [audit-logs/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/audit-logs/index.vue)、[table-header-date-picker-popover/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/common/table-header-date-picker-popover/index.vue#L156-L175) |
| **W01-4** | `D-05` | P2 | 配置中心主列表在宽屏下仅渲染 4 列，缺失配置治理核心的生效版本与变更人信息；小屏下 `HistoryDrawer` 内嵌 `DiffViewer` 固定 `side-by-side` 双栏模式引发内外双横向滚动条 | [config-center/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/config-center/index.vue#L80-L120)、[DiffViewer.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/config-center/modules/DiffViewer.vue) |

---

## 2. 第一性原理本质归因

1. **取消控制流代数类型正交性（公理 U1）**：异步调用链中存在两类截然不同的中断信号：① **并发世代取代（Supersede / Dispose Abort）**——由当前 `AbortController` 主动发出，旧请求结果已无意义，必须静默丢弃；② **主体拒绝授权（Authorization Rejected / User Cancelled）**——由用户主动关闭 Step-up MFA 弹窗产生，代表当前业务意图必须中止并回滚 UI 状态。`useLatestRequest` 通过宽泛属性名 `cancelled === true` 做鸭子类型匹配，破坏了两类控制流的边界隔离。
2. **特权资源前置门卫（Capability-Before-Mount）**：对读取本身受 Step-up MFA 保护的资源（网关 TOML 配置全文、配置历史快照），先打开编辑弹窗再在 `watch(visible)` 中拉取详情，必然造成“双层模态遮罩叠压 + 鉴权失败后空壳残留”。唯有将能力获取与数据预取前置到打开动作之前，才能在结构上令“未授权空白编辑器”成为不可达状态。
3. **认证阶段状态机与审计事实同构**：多因素认证中，口令校验通过仅代表进入 `MfaPending` 中间态，管理会话并未建立。审计账本必须忠实反映领域状态跃迁，将「口令挑战通过（待二次验证）」与「管理会话正式签发」区分为独立审计动作。

---

## 3. 详尽可落地技术方案

### 3.1 解耦 `useLatestRequest` 中止判定并重构配置中心前置预取流（`mazbri-web`）

1. **收敛 `useLatestRequest` 异常吞没边界**
   - 修改 [useLatestRequest.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/composables/useLatestRequest.ts)：
     - 从 `isRequestAbortedError(error)` 中**彻底移除 `if (candidate.cancelled === true) return true;`**，显式排除 `isMfaStepUpCancelled(error)`（返回 `false`），仅保留原生 `AbortError`、`CanceledError` 与 `ERR_CANCELED`。
     - 在 `execute` 的 `catch` 分支收敛为：`if (!ctx.isLatest() || (controller.signal.aborted && isRequestAbortedError(error))) { return undefined; }`——**仅当本轮 `controller.signal` 确实处于 `aborted` 态时才静默吞没取消异常**，外部拦截器抛出的任何异常（含 `MfaStepUpCancelledError`）原样上抛至调用方。
   - 在 [useLatestRequest.test.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/composables/__tests__/useLatestRequest.test.ts) 增补单测：断言并发新请求取代旧请求时静默返回 `undefined`，而回调抛出 `new MfaStepUpCancelledError()` 时原样 reject 上抛。

2. **配置中心「编辑配置」与「版本历史」改为前置能力门卫挂载**
   - 在 [useConfigCenterPage.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/config-center/composables/useConfigCenterPage.ts) 与 [config-center/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/config-center/index.vue) 维护行级加载态 `openingEditService: Ref<string | null>` 与 `openingHistoryService: Ref<string | null>`（对应行按钮绑定 `:loading`）。
   - 触发编辑已有配置时：先调用 `fetchConfig(row.service_name)`（若未持有有效 MFA 提权票据，HTTP 拦截器仅弹出单层 `MfaStepUpDialog`）；待拿到完整 `ConfigDetail` 后传入 `<ConfigEditDialog :initial-detail="preloadedDetail" />` 并置 `editVisible = true`；若用户取消 MFA，捕获 `isMfaStepUpCancelled` 静默复位按钮 `loading`，全程零双层弹窗闪烁、零空白编辑器挂载。
   - 同理触发版本历史时：先调用首屏历史拉取，成功后挂载打开 `HistoryDrawer`；同时在 [ConfigEditDialog.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/config-center/modules/ConfigEditDialog.vue#L9) 为保存按钮增加兜底守卫 `:confirm-disabled="authorizationDisabled || loadingDetail || (isEdit && !detailLoaded)"`，杜绝任何异常路径下空内容覆盖线上配置。

### 3.2 后端两阶段登录审计动作正名与跨包标签同步（`mazbri-mgmt` & `mazbri-web`）

1. **新增口令挑战审计动作枚举**
   - 在 [types.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/audit/types.rs) 的 `audit_actions!` 宏新增变体：
     `UserLoginMfaChallenge => "user.login_mfa_challenge"`（保留 `UserLogin => "user.login"` 专用于完整会话签发与单阶段口令直登或口令失败拒绝）。
   - 在 [auth_flow.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/admin/auth_flow.rs#L598-L600) 当 `determine_mfa(...)` 返回 `Some(mfa_response)`（即需要执行 `mfa_verify` 或 `mfa_required_setup`）时，通过 `crate::audit::record_event(...)` 显式向当前请求 `AuditScope` 提交 `AuditAction::UserLoginMfaChallenge`（附带 `details: { "next_step": mfa_stage }`）与已校验的 `AuditActor`。
   - 根据 [middleware.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/audit/middleware.rs#L311-L320) 的 `resolve_action_and_resource` 优先级规则：当业务代码显式提交 `primary` 事件时，自动覆盖路由默认的 `A::UserLogin`：
     - 口令错误 / 账号锁定：无业务事件或失败事件，仍记录 `user.login | Denied/Failure`；
     - 未启用 MFA 直登成功：记录单条 `user.login | Success`；
     - 启用 MFA 两阶段登录：首请求记录 `user.login_mfa_challenge | Success`，次请求 `/auth/mfa/verify` 通过后记录唯一一条正式 `user.login | Success`。

2. **三端动作标签契约同步**
   - 后端 [service.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/audit/service.rs) `ACTION_LABELS` 新增 `("user.login_mfa_challenge", "登录口令核验")`；若 `mazbri-cert-service` 引用审计动作映射测试，同步核验。
   - 前端 [labels.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/audit-logs/labels.ts) `ACTION_LABELS` 新增 `"user.login_mfa_challenge": "auditLogs.explicitActions.userLoginMfaChallenge"`，并在 `zh.json`（`"登录口令核验"`）与 `en.json`（`"Password Verified (MFA Pending)"`）补齐双语词条，确保后端 `every_action_has_frontend_label` 测试 100% 通过。

### 3.3 审计日志工具栏等高收敛与配置中心列表/Diff 自适应（`mazbri-web`）

1. **审计日志工具栏 `32px` 容器化对齐（`W01-3`）**
   - 为 [table-header-date-picker-popover/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/common/table-header-date-picker-popover/index.vue) 新增 `variant?: "header-icon" | "toolbar-button"` 属性（默认 `"header-icon"` 保持表头内嵌兼容）：当 `variant="toolbar-button"` 时，渲染 `height: 32px; padding: 0 12px; border: 1px solid var(--el-border-color); background: var(--el-fill-color-blank)` 的标准描边按钮（未选时显示日历图标 + `时间范围`，已选时高亮显示紧凑区间摘要与清除按钮）。
   - 在 [audit-logs/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/audit-logs/index.vue) 工具栏改用 `variant="toolbar-button"`，并将「显示审计查阅记录」包裹进 `height: 32px` 带边框胶囊容器（`.toolbar-toggle-chip`），使工具栏全部控件严格处于同一 `32px` 水平基线。

2. **配置中心列表信息补齐与 `DiffViewer` 响应式降级（`W01-4`）**
   - 检查 `ConfigSummary` DTO 已返回字段（如 `version`、`updated_by`、`updated_at`），在 [config-center/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/config-center/index.vue) 补齐 `版本`（等宽徽标 `v{version}`）与 `更新人` 列，收敛列间空白断层。
   - 在 [DiffViewer.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/config-center/modules/DiffViewer.vue) 绑定 `ResizeObserver`：当容器宽度 `< 820px` 时自动将 `diff2html` 的 `outputFormat` 切换为 `'line-by-line'`（宽屏默认 `'side-by-side'`），并在右上角提供「并排 / 统一」手动分段切换器，彻底消除小屏抽屉双层横向滚动条。

---

## 4. 二阶、三阶影响与红队边界守卫

| 风险场景与反例 | 连锁影响分析 | 防护与消解措施 |
| :--- | :--- | :--- |
| **反例 1：快速连点不同行的「编辑配置」引发竞态错位** | 前置预取模式下，若管理员先点 `mazbri-auth` 再迅速点 `mazbri-gateway`，旧请求晚返回可能打开错误的弹窗内容。 | 预取动作本身封装在页面级单例 `useLatestRequest` 实例中：第二次触发自动 `abort()` 上一次未决预取，且仅 `ctx.isLatest()` 为真时才挂载 `ConfigEditDialog`。 |
| **反例 2：`useLatestRequest` 收紧中止判定后，已有 `cancel()` 调用方是否弹错框** | 全站 17 个模块调用 `detailLoader.cancel()` 时依赖静默忽略中止错误。 | 调用 `cancel()` 时会同步执行 `currentSeq += 1` 且 `activeController.abort()`，`!ctx.isLatest()` 与 `controller.signal.aborted` 双重命中，保持 100% 静默兼容，零误报错误弹窗。 |
| **反例 3：审计日志检索「登录」过滤条件的行为延续性** | 拆分 `user.login_mfa_challenge` 后，合规员按动作筛选 `user.login` 只会看到最终完成的登录与登录失败记录，不再混入半途口令挑战噪声；按全文检索「登录」仍可同时检索出两阶段记录。 | 保持 `user.login` 的中文标签仍为 `登录`、`user.login_mfa_challenge` 为 `登录口令核验`，既净化精确动作统计，又保留完整认证链路可追溯性。 |

---

## 5. 验收门禁与实测清单

1. **自动化单测必测项**
   - 前端 `useLatestRequest.test.ts`：覆盖 `MfaStepUpCancelledError` 透传断言与 `cancel()` 静默断言。
   - 后端 `mazbri-mgmt` 审计测试：验证启用 MFA 账号执行 `/auth/login` 落库 `user.login_mfa_challenge`（`Success`），随后 `/auth/mfa/verify` 失败落库 `user.login`（`Denied`/`Failure`）、成功落库 `user.login`（`Success`）；运行 `every_action_has_frontend_label` 断言前后端标签表完全对齐。
2. **构建、热替换与实机核验**
   - 执行 `mazbri-web` `pnpm build && pnpm test`，执行 `mazbri-bin` 全工作区 `--release --locked` Clippy 与 `cargo test`。
   - 原子热替换 `mazbri-mazbri-mgmt-1` 容器内 `/usr/local/bin/mazbri-mgmt` 并重启。
   - 浏览器实测：① 未提权态在配置中心触发编辑/历史并按 `ESC` 取消 MFA，确认无空白弹窗或抽屉残留；② 执行一次完整 MFA 登录，在审计日志页确认同秒两条记录清晰区分为「登录口令核验」与「登录」，且右上角工具栏全部控件 `32px` 严丝合缝对齐；③ 切换 `1280×800` 视口打开配置历史对比，确认自动切换为单栏 Diff 无双滚动条。
