# Mazbri 技术文档索引

> 全栈联调部署、数据库重置初始化（`init-dbs`）、控制面自动配置与 `upupt` / `x-loadgen` 验收跑批统一由工作区 `redep` 技能（`/workspace/rust-proj/.agents/skills/redep/SKILL.md`）从零自动化执行，本目录不重复维护手工部署流水账。

## 目录结构

```text
docs/
├── README.md                                        # 文档总览与导航
├── tenant-integration-protocol.md                   # 租户接入协议 v1（外部线上契约）
├── protocol-vectors/
│   └── tenant-integration-v1.json                   # 租户接入协议固定测试向量（Rust 单测与多语言示例共用）
├── protocol-examples/                               # 多语言独立校验实现（Python / Go / Java）
├── architecture/
│   ├── crypto-platform.md                           # 通用密码学平台分层、算法闭集与 Profile 契约
│   └── upstream-auth-and-reconnect.md               # 上游令牌失效判定、重试预算与 WS 断线重连机制
└── performance/
    └── gateway-bypass-latency-optimization.md       # Gateway ↔ Bypass 通信机制、热路径缺陷修复与低延迟演进方案
```

## 文档清单

| 分类 | 文档 | 说明 |
| :--- | :--- | :--- |
| 对外协议契约 | [tenant-integration-protocol.md](./tenant-integration-protocol.md) | 租户 API Key 换票、传输加密信封、请求绑定签名、回调验签与标准错误码规范 |
| 协议向量与示例 | [protocol-examples/README.md](./protocol-examples/README.md) | 基于 [tenant-integration-v1.json](./protocol-vectors/tenant-integration-v1.json) 的 Python / Go / Java 独立验签与加解密示例 |
| 内部架构 | [architecture/crypto-platform.md](./architecture/crypto-platform.md) | `mazbri-crypto` 算法注册表、`TenantTransportProfile` / `UpstreamCryptoProfile` 与 `enc:v2:` 落库加密规范 |
| 内部架构 | [architecture/upstream-auth-and-reconnect.md](./architecture/upstream-auth-and-reconnect.md) | `mazbri-upstream-runtime` 认证失效重放、按代令牌重试预算、WS 连接池探活与计费口径 |
| 性能优化 | [performance/gateway-bypass-latency-optimization.md](./performance/gateway-bypass-latency-optimization.md) | `gateway ↔ bypass` 双容器 mTLS/gRPC 单跳延迟拆解、7 项热路径代码级缺陷修复清单与进程内直调演进约束 |
| 前端组件规范 | [../mazbri-web/docs/page-templates.md](../mazbri-web/docs/page-templates.md) | 管理后台 `OpsPage`、`ResourceTablePageShell`、`TenantDetailShell` 页面外壳与三态规范 |
