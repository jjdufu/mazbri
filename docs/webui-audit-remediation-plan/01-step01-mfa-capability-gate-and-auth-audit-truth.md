# Step 01：Step-up MFA 前置能力门卫、取消信号正交解耦、多阶段登录审计闭环与配置中心元数据补齐

## 1. 覆盖问题清单与源码精确定位

| 编号 | 关联审计项 | 级别 | 问题描述与全栈契约定位 | 精确源码位置 |
| :--- | :---: | :---: | :--- | :--- |
| **W01-1** | `G-03` | P0 | `MfaStepUpCancelledError` 定义了 `readonly cancelled = true`（用于抑制全局 HTTP 错误 Toast），被 `useLatestRequest` 的 `isRequestAbortedError` 误判为网络请求作废并静默返回 `undefined`，导致配置中心先挂载 `ConfigEditDialog` / `HistoryDrawer` 再叠加弹出 `MfaStepUpDialog`、取消 MFA 后滞留空白可保存编辑器（`saveBtnDisabled: false`）；同时 `CertificateDetailDrawer` 私钥解锁死分支及全站另外 14 个受 MFA 保护视图存在取消异常未捕获或误弹红框报错问题 | [useLatestRequest.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/composables/useLatestRequest.ts#L21-L34)、[error.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/utils/http/error.ts#L131-L137)、[mfa-dialog-service.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/utils/http/mfa-dialog-service.ts#L16-L27)、[ConfigEditDialog.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/config-center/modules/ConfigEditDialog.vue#L276-L331)、[HistoryDrawer.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/config-center/modules/HistoryDrawer.vue#L243-L294)、[CertificateDetailDrawer.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/certificates/modules/CertificateDetailDrawer.vue#L545-L615) |
| **W01-2** | `D-04` | P0 | **多阶段认证状态机与审计账本失配**：`/auth/login` 与 `/auth/mfa/verify` 在路由表均映射为 `AuditAction::UserLogin`（`"user.login"`）；`admin_login` 在签发 `mfa_pending` 或 `force_password_reset` 临时票据返回 HTTP 200 时未提交阶段事件覆盖默认动作，导致单次 MFA 登录同秒落库两条完全相同的 `登录 · 成功` 记录，而 `/auth/mfa/required-enable` 与 `/auth/first-login-change-password` 最终签发正式会话时却缺失 `user.login` 记录 | [route_map.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/audit/route_map.rs#L130-L171)、[auth_flow.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/admin/auth_flow.rs#L559-L605)、[required_flow.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/mfa/required_flow.rs#L322-L348)、[passwords.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/admin/passwords.rs#L316-L378)、[types.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/audit/types.rs#L34)、[service.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/audit/service.rs#L91)、[labels.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/audit-logs/labels.ts#L26) |
| **W01-3** | `D-04` | P1 | 审计日志工具栏中表头专用的 `36×36px` 无边框日历图标及裸露在灰底上的复选框与同行 `32px` 控件高度不一 | [audit-logs/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/audit-logs/index.vue#L18-L46)、[table-header-date-picker-popover/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/common/table-header-date-picker-popover/index.vue#L156-L175) |
| **W01-4** | `D-05`, `G-01` | P1 | `mazbri-cert-service` 的 `GET /configs/search` 未返回真实记录总数与当前版本号，`mazbri-mgmt` 以 `(offset + count) + 1` 硬编码伪造分页 `total`（如 8 条记录在 `page_size=5` 时返回 `total: 6`），配置中心列表缺少生效版本列；窄屏下 `HistoryDrawer` 内嵌 `DiffViewer` 固定双栏并排模式引发内外双横向滚动条 | [models.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/config_center/models.rs#L6-L29)、[metadata.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/config_center/repository/metadata.rs#L288-L320)、[query.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/config_center/query.rs#L103-L119)、[config-center/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/config-center/index.vue#L40-L85)、[DiffViewer.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/config-center/modules/DiffViewer.vue) |

---

## 2. 第一性原理本质归因

1. **取消控制流代数正交性（公理 U1）**：[error.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/utils/http/error.ts#L131-L137) 的 `handleError` 将所有 Axios `ERR_CANCELED` 转换为 `HttpError { code: 500, cancelled: true }`，而 `MfaStepUpCancelledError` 为复用全局响应拦截器的免弹窗判断也携带 `cancelled = true`。前者代表「并发旧请求已被取代，应静默丢弃」，后者代表「操作人主动中止授权，必须上抛回滚业务动作」。在 `isRequestAbortedError` 入口显式分离两类异常类型，才能同时保住全站 17 个模块的并发取消静默性与 MFA 取消回滚可靠性。
2. **轻量提权能力门卫（Capability-Before-Mount）**：对读取本身受 `RouteKind::SensitiveRead` 与 Step-up MFA 保护的资源，若在父列表组件直接预拉取完整业务载荷再挂载弹窗，会导致弹窗打开后内部 `watch` 二次请求并写入双倍高危审计日志，且在已提权慢网环境下造成开窗迟滞。仅将「会话提权能力校验（`ensureMfaBeforeOpen`）」前置到打开容器之前：未提权时单弹 `MfaStepUpDialog`，通过后立即挂载弹窗并在容器内部单飞加载载荷，即可零耦合、零双遮罩、零重复审计地闭环。
3. **认证状态跃迁与审计事实严格同构**：口令核验通过进入 `TokenKind::MfaPending` 或 `TokenKind::PasswordReset` 仅属认证半态，管理会话尚未建立；唯有真正签发完整管理会话 Cookie/JWT 的终态出口才对应 `user.login | Success`。

---

## 3. 详尽可落地技术方案

### 3.1 正交化 `useLatestRequest` 取消判定并收敛全站 Step-up MFA 取消流（`mazbri-web`）

1. **精确分离 `MfaStepUpCancelledError` 与 `HttpError` 请求中止**
   - 修改 [useLatestRequest.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/composables/useLatestRequest.ts#L21-L34)：
     ```ts
     export function isRequestAbortedError(error: unknown): boolean {
       if (isMfaStepUpCancelled(error)) return false;
       if (!error || typeof error !== "object") return false;
       if (error instanceof HttpError && error.cancelled === true) return true;
       const candidate = error as { name?: string; code?: string | number };
       if (candidate.code === "ERR_CANCELED") return true;
       return candidate.name === "AbortError" || candidate.name === "CanceledError";
     }
     ```
   - 在 [useLatestRequest.test.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/composables/__tests__/useLatestRequest.test.ts)（`node:test` + `node:assert/strict`）增补断言：① 经 `handleError` 包装的 `HttpError(..., { cancelled: true })` 与 `ERR_CANCELED` 均静默返回 `undefined`；② 抛出 `new MfaStepUpCancelledError()` 时原样 reject 上抛且 `loading.value` 复位为 `false`。

2. **引入轻量 `ensureMfaBeforeOpen` 前置门卫与编辑器防空守卫**
   - 在 [mfa-dialog-service.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/utils/http/mfa-dialog-service.ts) 导出辅助函数 `ensureMfaBeforeOpen(category: MfaCategory): Promise<boolean>`：调用 `ensureCategory(category)`（若当前会话窗口有效则直接命中返回 `true`，未激活则单层弹出 `MfaStepUpDialog`，用户取消时捕获 `isMfaStepUpCancelled` 静默返回 `false`）。
   - 在 [useConfigCenterPage.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/config-center/composables/useConfigCenterPage.ts) 触发编辑配置、版本历史与选择导出前先执行 `if (!(await ensureMfaBeforeOpen("platform"))) return;`，通过后再置弹窗/抽屉 `visible = true`，内部维持单次加载（零重复 `SensitiveRead` 审计请求）。
   - 在 [ConfigEditDialog.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/config-center/modules/ConfigEditDialog.vue#L9) 为确认按钮增加硬守卫 `:confirm-disabled="authorizationDisabled || loadingDetail || (isEdit && !detailLoaded)"`，并在服务端窗口刚过期的极端路径命中 `isMfaStepUpCancelled` 时自动关闭弹窗。

3. **全站受 MFA 保护调用点的取消静默收敛**
   - 核验 [CertificateDetailDrawer.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/certificates/modules/CertificateDetailDrawer.vue#L608-L615) 私钥解锁 `unlockRunner`：在 `useLatestRequest` 透传 `MfaStepUpCancelledError` 后，确认取消 MFA 时静默保持锁定态、不打 `logger.error`。
   - 统一清理其余受 MFA 保护模块的取消异常处理：
     - 为 [RulesPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/alerts/RulesPanel.vue#L524-L556) 的 `save`、`toggle`、`remove` 补齐 `try / catch` 并对 `isMfaStepUpCancelled` 静默早退，消除 `UnhandledPromiseRejection`；
     - 在 [SilencesPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/ops/alerts/SilencesPanel.vue#L302-L326) 与 [BillListPanel.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/billing/bills/BillListPanel.vue#L1263-L1266) 的 `catch` 块首行增加 `if (isMfaStepUpCancelled(error)) return;`，消除取消 MFA 误弹红框提示；
     - 在 `ExportDialog.vue`、`api-keys.vue`、`transport-encryption.vue`、`account.vue`、`usage-reconciliation/index.vue`、`tenants/index.vue`、`users` 相关弹窗的 `catch` 分支过滤 `isMfaStepUpCancelled`，杜绝向控制台输出 `logger.error`。

### 3.2 多阶段登录审计状态机闭环与跨包标签契约同步（`mazbri-mgmt` & `mazbri-web`）

1. **细分前置认证阶段与最终会话签发审计动作**
   - 在 [types.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/audit/types.rs) 的 `audit_actions!` 宏新增枚举变体：
     - `UserLoginMfaChallenge => "user.login_mfa_challenge"`（口令核验通过，进入二次认证或首次 MFA 绑定挑战态）
     - `UserLoginPasswordResetRequired => "user.login_password_reset_required"`（口令核验通过，要求首次改密半态）
   - 在 [auth_flow.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/admin/auth_flow.rs#L559-L600)（第 559 行已执行 `audit::set_actor`）：
     - `user_row.force_password_reset` 分支（第 562–578 行）通过 `params.audit.log(...)` 记录 `AuditAction::UserLoginPasswordResetRequired`，覆盖路由默认 `A::UserLogin`；
     - `determine_mfa(...)` 返回 `Some(mfa_response)` 分支（第 598–600 行）通过 `params.audit.log(...)` 记录 `AuditAction::UserLoginMfaChallenge`（附带 `next_step` 明细），覆盖路由默认 `A::UserLogin`；
   - 在首次强制绑定 MFA 完成出口 [required_flow.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/mfa/required_flow.rs#L322-L348) 与首次改密无需 MFA 直接签发正式会话出口 [passwords.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/admin/passwords.rs#L350-L378)，在签发完整会话令牌时额外落库/并入终态登录事件，确保所有建立管理会话的完整链路均拥有唯一终态登录记录。

2. **前后端动作标签表严格同步**
   - 后端 [service.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/audit/service.rs) `ACTION_LABELS` 新增 `("user.login_mfa_challenge", "登录口令核验")` 与 `("user.login_password_reset_required", "登录口令核验（待改密）")`。
   - 前端 [labels.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/audit-logs/labels.ts) 与 `zh.json` / `en.json` 同步补齐双语词条，确保 `mazbri-mgmt` 与 `mazbri-cert-service` 的动作标签契约测试 100% 通过。

### 3.3 配置中心真实计数与版本号补齐、审计工具栏等高与 Diff 自适应（`mazbri-cert-service`、`mazbri-mgmt`、`mazbri-web`）

1. **补齐配置中心真实分页计数与当前生效版本号**
   - 修改 `mazbri-cert-service`：
     - 在 [models.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/config_center/models.rs#L131-L138) 的 `ConfigEntryWithMeta` 增加 `pub version: i32`，并定义分页搜索响应结构 `ConfigSearchPage { pub items: Vec<ConfigEntryWithMeta>, pub total: i64 }`；
     - 在 [repository/metadata.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/config_center/repository/metadata.rs#L288-L355) 的 `search_configs` 中，通过同源过滤条件查询精确 `COUNT(*)`，并在条目投影中关联子查询 `COALESCE((SELECT MAX(ch.version_number) FROM config_history ch WHERE ch.service_name = ce.service_name), 1) AS version`；
     - 同步更新 [handlers/config/metadata.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/handlers/config/metadata.rs) 搜索处理器与 [query.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/config_center/query.rs#L103-L119)，**彻底删除 `(offset + count) + 1` 假总数计算**，透传真实 `total` 与 `version`。
   - 在 [config-center/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/config-center/index.vue) 与 [useConfigCenterPage.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/config-center/composables/useConfigCenterPage.ts#L22-L42) 保留现有 `服务名`、`更新人`、`更新时间`、`操作` 列，在 `服务名` 后新增等宽 `版本` 列（渲染 `v{row.version}`）。

2. **审计工具栏 `32px` 容器化对齐与 `DiffViewer` 窄屏降级**
   - 为 [table-header-date-picker-popover/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/common/table-header-date-picker-popover/index.vue) 新增 `variant?: "header-icon" | "toolbar-button"`（默认 `"header-icon"` 兼容表头内嵌）：`variant="toolbar-button"` 渲染 `height: 32px; padding: 0 12px; border: 1px solid var(--el-border-color); background: var(--el-fill-color-blank)` 的描边按钮（文案 `时间范围` / 已选区间摘要）。
   - 在 [audit-logs/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/audit-logs/index.vue) 工具栏接入 `variant="toolbar-button"`，并将「显示审计查阅记录」封装进 `height: 32px` 描边胶囊容器（保持左侧 `ListQueryBar` 既有 `240px` 标准宽度不变）。
   - 在 [DiffViewer.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/control-panel/config-center/modules/DiffViewer.vue) 绑定 `ResizeObserver`：容器宽度 `< 820px` 时自动将 `outputFormat` 切换为 `'line-by-line'`，并在右上角提供 `并排 / 统一` 手动分段器，消除窄视口双层横向滚动条。

---

## 4. 二阶、三阶影响与边界守卫

| 边界场景 | 连锁风险分析 | 消解与防护设计 |
| :--- | :--- | :--- |
| **Axios 取消请求经 `handleError` 包装后属性丢失** | `utils/http/error.ts` 将 `ERR_CANCELED` 转为 `HttpError { code: 500, cancelled: true }`，若仅按 `ERR_CANCELED` 判断会导致并发切页取消报错外溢。 | `isRequestAbortedError` 显式保留 `error instanceof HttpError && error.cancelled === true`，仅优先排除 `isMfaStepUpCancelled(error)`。 |
| **`SensitiveRead` 重复触发审计落库** | `GET /config-center/configs/{service}` 属于必记审计的敏感读取，父子组件重复拉取会产生双倍查阅日志。 | `ensureMfaBeforeOpen("platform")` 仅校验/获取会话提权窗口，详情拉取仍由弹窗内部唯一执行一次。 |
| **跨服务升级顺序依赖** | `mazbri-mgmt` 依赖 `mazbri-cert-service` 搜索接口的新响应结构。 | 热替换时严格先覆盖重启 `mazbri-mazbri-cert-1`（`/usr/local/bin/mazbri-cert-service`）待 `healthy` 后，再覆盖重启 `mazbri-mazbri-mgmt-1`。 |

---

## 5. 验收门禁与实测清单

1. **自动化单测必测项**
   - 前端 `useLatestRequest.test.ts`：验证 `HttpError(cancelled: true)` 静默返回 `undefined`、`MfaStepUpCancelledError` 原样上抛。
   - 后端 `mazbri-cert-service` 与 `mazbri-mgmt` 测试：验证配置分页返回精确 `total` 与 `version`；验证两阶段 MFA 登录首请求记录 `user.login_mfa_challenge`、次请求记录唯一 `user.login`；`every_action_has_frontend_label` 全绿。
2. **构建、热替换与 `9224` 实机核验**
   - 依次执行 `mazbri-web` `pnpm build && pnpm test`，`mazbri-bin` 全工作区 `--release --locked` Clippy 与 `cargo test`。
   - 原子热替换 `mazbri-mazbri-cert-1` ➔ `mazbri-mazbri-mgmt-1`。
   - 连接 `127.0.0.1:9224` 验证：① 未提权态在配置中心点编辑/历史并取消 MFA，仅单层弹窗且关闭后零空白残留；② 配置中心列表准确显示 `版本` 列且 `page_size=5` 下返回真实总数 `8`；③ 审计日志工具栏全部控件 `32px` 水平对齐，新执行一次 MFA 登录不再出现同秒重复 `登录` 记录；④ `1280×800` 下历史对比自动呈现单栏 Diff 无双滚动条。
