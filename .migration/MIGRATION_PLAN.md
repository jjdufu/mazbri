# Mazbri WebUI React 19 分阶段迁移与 Subagent 并行协作蓝图

> 本蓝图**不设时间排期**，严格按依赖拓扑划分为 **7 个独立阶段（Stage 1 ~ Stage 7）**。每个阶段由用户开启一个新的 Agent 会话作为本阶段 **Coordinator** 执行；Stage 2 ~ Stage 6 内部按互斥目录边界拉起 Subagent 团队并行施工。

---

## 一、目标目录结构（纯净 React 19 架构）

```text
mazbri/mazbri-web/
├── index.html
├── package.json
├── tsconfig.json
├── vite.config.ts
├── scripts/
└── src/
    ├── main.tsx
    ├── App.tsx
    ├── router.tsx                        # 仅 Coordinator 维护，聚合各域 routes.tsx
    ├── styles/
    │   └── globals.css                   # 参照 .migration/reference/globals.css + Mazbri 品牌蓝/深海蓝
    ├── api/                              # Stage 1 原样平移自 legacy（27 文件，100% 纯 TS）
    ├── types/                            # Stage 1 原样平移自 legacy（含 api/generated/ 供 ts-rs 校验）
    ├── utils/                            # Stage 1 原样平移（120 文件纯 TS + 改写 http/index.ts & mfa-dialog-service.ts）
    ├── constants/                        # Stage 1 原样平移
    ├── enums/                            # Stage 1 原样平移
    ├── list-catalog/                     # Stage 1 原样平移
    ├── locales/                          # Stage 1 原样平移 zh.json / en.json + 域增量合并
    ├── stores/                           # Zustand: auth.ts / theme.ts / locale.ts / mfa-step-up.ts / lockscreen.ts
    ├── components/
    │   ├── ui/                           # Stage 2 封板：primitives.tsx / ResizableWorkspace.tsx / DataTablePage.tsx /
    │   │                                 #   EntityDetailLayout.tsx / SettingsSectionLayout.tsx / FormModal.tsx /
    │   │                                 #   FormDrawer.tsx / DateRangePicker.tsx / CodeEditor.tsx / DiffViewer.tsx
    │   ├── layout/                       # Stage 2 封板：AppShell.tsx / Sidebar.tsx / Topbar.tsx / MfaStepUpHost.tsx /
    │   │                                 #   LockScreen.tsx / WatermarkOverlay.tsx / PersonalSecurityDrawer.tsx
    │   └── charts/                       # Stage 3：EChartBox.tsx / ChartCard.tsx（统一 UTC+8 / 空态 / Resize / 亮暗联动）
    └── domains/                          # 各业务 Subagent 100% 独占目录，各导出 routes.tsx
        ├── auth/                         # 登录 / 首次改密 / MFA 绑定与校验
        ├── control-panel/                # users / certificates / audit-logs(含 labels.ts) / system / config-center
        ├── tenants/                      # 租户列表 / 5 Tab 租户详情重构
        ├── billing/                      # 经营概览 / 账单列表 / 用量对账
        ├── products/                     # 产品目录 / 授权订阅 / 产品详情
        ├── upstream-supply/              # 供应商列表与详情 / 通道列表 / 草稿箱 / 微服务接入抽屉
        ├── callbacks/                    # 订阅配置 / 投递任务
        ├── ops/                          # 调用链路与详情 / 告警中心 / 运行指标与服务详情
        └── workbench/                    # 免代码接入工作台（复用 4,144 行纯 TS 引擎 + ResizableWorkspace UI）
```

---

## 二、7 个阶段依赖拓扑与门禁矩阵

| 阶段 | 核心目标 | Subagent 并行拓扑 | 必过门禁 | 本地 Commit |
| :--- | :--- | :--- | :--- | :---: |
| **Stage 1** | 隔离旧 `mazbri-web` → `mazbri-web-legacy/`；初始化 React 19 工程；平移 3.3 万行纯 TS/JSON 资产；重写 `http/index.ts` 与 `mfa-dialog-service.ts`；对齐 Rust `labels.ts` 路径 | 单 Agent（串行防路径错乱） | `gate-0` + 纯 TS 全量单测 + `cargo test -p mazbri-mgmt` + `cargo test -p mazbri-cert-service` | 必需 |
| **Stage 2** | 移植 xBlueNode `globals.css` + `primitives.tsx` + 4 大 B 端骨架壳 + `AppShell` + 认证/MFA/锁屏/个人安全/RBAC 完整闭环 + 底座封板 | **2 并行 Subagent**：<br>• `SA-UI`（`src/styles/**`, `src/components/ui/**`）<br>• `SA-ShellAuth`（`src/stores/**`, `src/components/layout/**`, `src/domains/auth/**`） | `gate-0` + `gate-1` + `gate-2`（首次热替换）+ `chrome-devtools` MCP 9224 登录/MFA/锁屏/暗色初验 | 必需 |
| **Stage 3** | 控制面板 5 页 + 运维监控 3 页 & 2 详情页 + 统一 ECharts 包装器（同步决定 `/dashboard` 去留） | **2 并行 Subagent**：<br>• `SA-ControlPanel`（`src/domains/control-panel/**`）<br>• `SA-Ops`（`src/domains/ops/**`, `src/components/charts/**`） | `gate-0` + `gate-1` + `gate-2` 热替换 + `gate-3` 实机自动巡检 + MCP 9224 交互抽检（含配置中心 MFA 提权） | 必需 |
| **Stage 4** | 运营中心核心域全量迁移：租户列表 & 5 Tab 重构详情、账单结算、产品治理 & 详情、上游供给 & 详情/通道/微服务抽屉、回调订阅 | **2 并行 Subagent**：<br>• `SA-TenantsBilling`（`src/domains/tenants/**`, `src/domains/billing/**`）<br>• `SA-ProductsSupply`（`src/domains/products/**`, `src/domains/upstream-supply/**`, `src/domains/callbacks/**`） | `gate-0` + `gate-1` + `gate-2` 热替换 + `gate-3` 实机自动巡检 + MCP 9224 充值(MFA)/加密/对账策略/产品授权抽检 | 必需 |
| **Stage 5** | 免代码接入工作台（`upstream-connectors`）独立重构，复用 4,144 行纯 TS 引擎，基于 `ResizableWorkspace` 重写全屏 IDE | **2~3 并行 Subagent**：<br>• `SA-WB-Core`（工作台壳、状态 Store、左侧接口树与顶部栏）<br>• `SA-WB-Editor`（参数/请求体/请求头/鉴权/出站规格/断言/变量提取/TLS 面板）<br>• `SA-WB-Runtime`（响应试调、环境管理、准入校验、导入导出、草稿发布、快捷键） | `gate-0` + `gate-1` + 工作台纯 TS 引擎单测 + `gate-2` 热替换 + `gate-3` + MCP 9224 试调与暗色模式验证 | 必需 |
| **Stage 6** | 全站深度回归 QA：三角色（`R_ADMIN`/`R_FINANCE`/`R_VIEWER`）× 双分辨率（`1440×900`/`1280×800`）× 亮暗双模式 × 全部浮层 × 旧审计 20 图逐条核销 + 缺陷修复闭环 | Coordinator + `SA-QA`（只读巡检出缺陷单）+ 按域回派对应业务 Subagent 定点修复 | `gate-0` + `gate-1` + `gate-2` 热替换 + `gate-3` 100% PASS + `FEATURE_PARITY_LEDGER` 100% 勾选 | 必需 |
| **Stage 7** | 破坏性清场：删 `mazbri-web-legacy/`、替换 `AGENTS.md` 第三节为永久版、删 `AGENTS.md` 哨兵块、深度清理磁盘、执行 `gate-4-sunset-check.mjs`、销毁 `.migration/` | 单 Agent | `gate-4-sunset-check.mjs` + `pnpm build && pnpm test` + 全仓 `cargo clippy --release --locked --workspace --all-targets -- -D warnings` + `cargo test --release --locked --workspace` + 最终热替换复验 | 必需 |

---

## 三、Subagent 协作防冲突硬协议

1. **目录独占，严禁越界**：每个 Subagent 仅允许写入分配给自己的独占目录；每次交付前必须跑 `node .migration/gates/gate-1-static-laws.mjs --scope <dir>` 校验未越权修改共享文件。
2. **路由与词条去中心化**：
   - 每个域在自己目录下导出 `routes.tsx`，仅由当前阶段 Coordinator 在 `src/router.tsx` 中统一注册；
   - 各域补充的新词条放在域内或统一由 Coordinator 合并进 `src/locales/`，避免并行修改同一个 `zh.json` 发生冲突。
3. **公共原语缺口走提单**：Stage 3~5 业务 Subagent 如发现缺少公共组件或 Token，禁止在业务域手写一次性样式替代，必须回报 Coordinator 评估后统一沉淀进 `src/components/ui/`。
4. **单点编译与单点热替换**：同一时刻只能由 Coordinator 跑 `gate-2-local-hotswap.sh` 与 `gate-3-mcp-cdp-verify.mjs`，杜绝并发 `cargo` 抢锁或容器重启踩踏。
