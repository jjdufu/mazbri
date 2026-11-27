# Mazbri WebUI React 19 架构迁移进度账本

> **跨会话交接铁律**：每个阶段的新 Agent 会话在开始前必须读取本文件，并在本阶段门禁全绿、本地 `git commit` 完成后更新状态、Commit SHA、门禁结果与磁盘快照。

| 阶段 | 阶段名称 | 状态 | 负责方式 | Commit SHA | 门禁结果 | 磁盘快照 | 备注 |
| :--- | :--- | :---: | :--- | :---: | :--- | :--- | :--- |
| **Stage 0** | 迁移治理包、参照资产与门禁落库 | ✅ DONE | 规划阶段 | *(落库提交)* | 治理包自检通过 | — | 仅落盘 `.migration/` 与 `AGENTS.md` 哨兵块，未动业务代码 |
| **Stage 1** | 旧代码隔离 + React 19 工程初始化 + 3.3 万行纯 TS/JSON 资产平移 + Rust 路径对齐 | ⬜ PENDING | 单 Agent | — | 待执行 | — | 目标：纯 TS 单测 100% 通过 + `cargo test -p mazbri-mgmt` 绿灯 |
| **Stage 2** | Design System + xBlueNode 原语移植 + 4 大 B 端骨架壳 + AppShell + 认证/MFA/锁屏闭环 | ⬜ PENDING | 2 并行 Subagent | — | 待执行 | — | 目标：底座封板 + 首次容器热替换 + `9224` 登录/MFA/暗色初验 |
| **Stage 3** | 控制面板域（5 页）+ 运维监控域（3 页 + 2 详情页 + 统一 ECharts 包装器） | ⬜ PENDING | 2 并行 Subagent | — | 待执行 | — | 目标：`gate-1/2/3` 全绿 + MCP 交互抽检通过 |
| **Stage 4** | 运营中心核心域（租户 5 Tab 重构/账单结算/产品治理/上游供给/回调订阅） | ⬜ PENDING | 2 并行 Subagent | — | 待执行 | — | 目标：旧版 20 项审计缺陷主体清零 + 充值/MFA/加密链路通过 |
| **Stage 5** | 免代码接入工作台（`upstream-connectors`）独立重构 | ⬜ PENDING | 专项 Subagent 组 | — | 待执行 | — | 目标：复用 4,144 行纯 TS 引擎 + 融入主站视觉与亮暗模式 |
| **Stage 6** | 全站 QA 深度回归（三角色 × 双分辨率 × 亮暗 × 全浮层 × 20 项旧图核销） | ⬜ PENDING | Coordinator + 按域回派 | — | 待执行 | — | 目标：`FEATURE_PARITY_LEDGER` 与 20 项旧图 100% 核销 |
| **Stage 7** | 破坏性清场 + 深度清盘 + 永久 `AGENTS.md` 切换 + 销毁 `.migration/` | ⬜ PENDING | 单 Agent | — | 待执行 | — | 完成后本文件随 `.migration/` 一并删除，迁移铁律正式失效 |

## 遗留问题 / 跨会话交接备注

- 当前无遗留阻塞项。准备执行 Stage 1 前，请在目标开发机将 `.migration/.env.local.example` 复制为 `.migration/.env.local` 并填入测试账号与 TOTP Secret（不入 Git）。
