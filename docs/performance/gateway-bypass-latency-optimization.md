# Gateway ↔ Bypass 通信机制与热路径性能优化规范

## 1. 通信架构与单跳耗时构成

### 1.1 进程职责与边界划分

Mazbri 数据面将入口治理与外部上游出站执行拆分为两个独立容器进程：

| 进程 | 核心职责 | 独占资源与边界 |
| :--- | :--- | :--- |
| `mazbri-gateway` | 租户鉴权、传输信封加解密、限流、余额预检、计费入队、路由匹配（`GatewayRouteEngine`）、熔断切换、响应信封组装 | 直连平台 PostgreSQL（加载路由规则、异步批量写入交互事件）与 Redis；内存持有预解析路由快照与密文态 `supplier_config`（`enc:v2:...`），不执行凭据解密 |
| `mazbri-bypass` | 封装共享运行时 `mazbri-upstream-runtime`，执行出站协议适配（HTTP/JSON、Form、SOAP、GraphQL、JSON-RPC、gRPC、WebSocket、Job Poll）、动态换票、上游报文加解密与重试 | 持有 `MAZBRI_SUPPLIER_SECRETS_KEY` 解密供应商凭据与客户端私钥；执行全协议 SSRF 地址拦截与公网出站收敛；提供 `:8890` 管理探针接口供 `mazbri-mgmt` 工作台试运行 |

### 1.2 传输协议栈与按连接配置同步

1. **连接池与传输层**：Gateway 通过 `UpstreamHttp2ClientPool`（`mazbri-bin/mazbri-gateway/src/http2_upstream_client/mod.rs`）对每个上游端点默认维护 **16 条持久 HTTP/2 + 双向 mTLS 连接槽位（`PooledChannel`）**，按原子游标轮询分发 gRPC Unary 请求（`BypassHttpProxyService/Execute`，Prost 编解码）。Bypass 通过 `BypassGrpcServer`（`mazbri-bin/mazbri-bypass/src/server.rs`）强制校验客户端证书并处理请求。
2. **按连接配置哈希增量协议**：Gateway 在路由加载期预序列化 `egress_spec` 与 `supplier_config` 并预计算 SHA-256 哈希。每条 `PooledChannel` 独立维护对端已确认哈希表（`synced_hashes`）：槽位未同步时下发全量 JSON，仅在收到对端成功响应后登记同步状态；后续命中同一槽位且哈希一致时仅下发哈希与空串。Bypass 通过本地 Moka 单飞缓存（`try_get_with`，空闲过期 600 秒）复用已解密配置；若对端返回 `CONFIG_CACHE_MISS`，Gateway 锁定当前连接槽位作废对应键并携带全量 JSON 重试一次；若发生传输层错误或连接重建迹象，槽位自动清空已确认哈希。

### 1.3 双容器单跳物理耗时构成

```text
gateway worker ──► gateway h2 driver (mTLS 加密) ──► Docker veth / bridge / conntrack ──► bypass h2 driver (mTLS 解密) ──► bypass worker
gateway worker ◄── gateway h2 driver (mTLS 解密) ◄── Docker veth / bridge / conntrack ◄── bypass h2 driver (mTLS 加密) ◄── bypass worker
```

| 耗时分项 | 典型耗时（IP 上游 + 小报文） | 典型耗时（域名上游 + 中大报文） | 构成说明 |
| :--- | :--- | :--- | :--- |
| 跨容器网络与协议栈底噪 | `0.9–1.3 ms` | `0.9–1.3 ms` | 双端 Protobuf 编解码、HTTP/2 帧封装、4 次 TLS 加解密、Linux TCP + Docker 虚拟网桥转发、4 次跨任务调度唤醒 |
| 双端热路径本地开销 | `< 0.15 ms` | `0.15–0.35 ms` | 路由快照零拷贝匹配、原始 JSON 切片直通、按连接哈希查找、异步批写队列入队 |

---

## 2. 双容器热路径关键机制与代码边界

### 2.1 全协议连接期 SSRF 拦截与零阻塞预检

- **实现位置**：
  - `mazbri-libs/mazbri-upstream-runtime/src/ssrf.rs`（`validate_url_syntax_and_literal`、`resolve_checked_socket_addrs`、`SsrfSafeResolver`、`extract_ssrf_error`）
  - `mazbri-libs/mazbri-upstream-runtime/src/engine.rs`（`RuntimeEngine::preflight`、`h2c_client`）
  - `mazbri-libs/mazbri-upstream-runtime/src/http_call.rs`、`src/auth.rs`、`src/adapters/grpc.rs`、`src/ws_pool.rs`
- **运行机制**：
  1. **规格校验去重**：`InvokeInput.spec_validated` 标记来自 Bypass 缓存或工作台预检的 `Arc<ChannelEgressSpec>`，`RuntimeEngine::preflight` 与上游密码流水线（`seal_request_with_validation` / `open_response_with_validation`）直接跳过重复规格校验。
  2. **同步语法与字面量预检**：数据面热路径（`preflight`、`send_http_timed`、换票 `fetch_token`、`grpc::invoke`、`ws_unary`）仅执行纯内存 `validate_url_syntax_and_literal`（校验协议头、元数据主机黑名单及字面量私网/回环 IP），不在请求期派发 `spawn_blocking` 同步 DNS 查询。保留异步 `SsrfChecker::validate_url` 供 `mazbri-mgmt` 管理面保存与导入预检调用。
  3. **全协议连接期防 DNS 重绑定**：HTTP/HTTPS、换票客户端与明文 HTTP/2（`RuntimeEngine.h2c_client`）统一挂载 `SsrfSafeResolver`；WebSocket 连接池 `WsPool::connect` 通过 `resolve_checked_socket_addrs` 完成异步解析与全地址校验后再建立 TCP/TLS 连接。任一解析结果命中封禁网段即整批拒绝，并通过 `extract_ssrf_error` 精确归类为 `ExecuteError::SsrfBlocked`。

### 2.2 路由加载期通道配置预解析与单通道故障隔离

- **实现位置**：
  - `mazbri-bin/mazbri-gateway/src/route_source.rs`（`PreparedChannel`、`prepare_channel`、`GatewayRouteEngine`）
  - `mazbri-bin/mazbri-gateway/src/gateway/execution/http_upstream.rs`
- **运行机制**：
  1. `load_route_engine` 构建候选通道时一次性生成 `PreparedChannel`：
     - `Transparent`：持有已校验 `adapter`、`timeout_ms`、预序列化 `supplier_json` / `egress_json`、SHA-256 哈希与同步键（`supplier:<code>` / `egress:<id>`）；
     - `Microservice`：持有已校验 `contract_id` 与 `timeout_secs`；
     - `Invalid`：隔离配置损坏或契约未知的单条通道，保留模式与契约提示，不中断整表路由热加载。
  2. 请求期路由匹配、日志元数据提取、重试提示与 `InvokeRequest` 组装均直接借用 `PreparedChannel` 字段；单条 `Invalid` 通道仅在实际尝试时记录契约错误指标并返回 `BadGateway`，按 `switch_on_bad_gateway` 切换至后续健康候选通道。

### 2.3 请求与响应原始 JSON 直通及零冗余克隆

- **实现位置**：
  - `mazbri-bin/mazbri-gateway/src/auth/middleware.rs`（`parse_gateway_request_payload`）
  - `mazbri-bin/mazbri-gateway/src/gateway/validation.rs`（`GatewayRequestBody`）
  - `mazbri-libs/mazbri-upstream-runtime/src/adapters/{mod.rs, http_json.rs, http_envelope.rs}`
  - `mazbri-bin/mazbri-bypass/src/service.rs`（`BypassLogSnapshot`、`invoke_success`）
  - `mazbri-bin/mazbri-gateway/src/gateway/execution/http_upstream.rs`、`src/gateway/serializer.rs`、`src/gateway/response_cache.rs`
- **运行机制**：
  1. **网关入口单遍解析与原始载荷捕获**：`parse_gateway_request_payload` 对常规明文请求单遍解析 `GatewayRequest`，仅在快速前缀或 `"encrypted"` 特征命中时解析 `TransportEnvelope`。`GatewayRequestBody` 基于 `serde_json::value::RawValue` 校验并捕获 `Arc<str>` 原始 JSON 文本，不分配 `serde_json::Value` DOM 树。
  2. **去程单次请求零克隆**：`invoke_bypass` 首轮直接移动 `InvokeRequest` 进入 gRPC 调用，仅在 `CONFIG_CACHE_MISS` 或传输层重试慢路径按需重建请求。
  3. **Bypass 零拷贝日志快照与直通返回**：生产模式下无响应改写、无解密及无正文令牌作废检查的 `http_json` / `http_envelope` 通道直接通过 `&RawValue` 校验上游字节流并填入 `InvokeResult.raw_payload_json`；`run_traced_invoke` 在派发异步日志前仅提取标量快照 `BypassLogSnapshot`，不克隆请求与响应正文。
  4. **网关回程 `RawValue` 直嵌序列化**：`invoke_response_to_upstream` 将合法 JSON 响应封装为 `ProxyBody::JsonRawStr(Arc<str>)`，泛型 `SuccessEnvelope<'a, D>` 通过 `&RawValue` 将响应数据直接拼接至输出字节缓冲，后置任务仅在实际命中活跃回调订阅时才解析 `Value`。

### 2.4 预排序共享路由切片与通道限流前置短路

- **实现位置**：
  - `mazbri-libs/mazbri-routing/src/engine.rs`（`RouteEngine<P>`、`CandidateChannel<P>`、`ChannelRateLimitOverrides`）
  - `mazbri-bin/mazbri-gateway/src/ratelimit/channel_limit.rs`
  - `mazbri-bin/mazbri-gateway/src/gateway/execution/types_and_entry.rs`
- **运行机制**：
  1. `RouteEngine<P>` 在构建期完成按 `order` 预排序并以 `Arc<[Arc<CandidateChannel<P>>]>` 存储；常规无权重、无版本过滤路由匹配直接返回 `Cow::Borrowed(&[Arc<CandidateChannel<P>>])`，零 `Vec` 分配、零逐通道原子计数递增。
  2. 通道限流覆盖项（`rate_limit_mode`、`rate_limit_max_wait_ms`、`rate_limit_max_queue_depth`）在加载期解析为 `Copy` 结构体 `ChannelRateLimitOverrides`；执行循环在创建限流 span 前内联判断 `state.channel_ratelimiter.is_enabled() && channel.qps_limit > 0`，未启用或不限流通道直接短路。

### 2.5 Bypass 单飞缓存、密码流水线与描述符池复用

- **实现位置**：
  - `mazbri-libs/mazbri-crypto/src/registry.rs`（`AlgorithmRegistry::global`）
  - `mazbri-libs/mazbri-upstream-runtime/src/adapters/{crypto_wire.rs, grpc.rs}`
  - `mazbri-bin/mazbri-bypass/src/service.rs`
- **运行机制**：
  1. `AlgorithmRegistry::global()` 通过 `LazyLock` 提供进程级不可变算法注册表单例，网关、管理面与出站运行时共享同一实例。
  2. `MazbriBypassServiceImpl` 对 `supplier_config_cache` 与 `egress_spec_cache` 使用 Moka `try_get_with` 单飞加载，并发冷启动请求仅执行一次 JSON 解析、`MAZBRI_SUPPLIER_SECRETS_KEY` 解密与 `Arc<PipelineKeys>` 派生。
  3. `artifact_cache` 缓存 `CachedArtifact { bytes: Arc<[u8]>, descriptor_pool: Option<Arc<DescriptorPool>> }`，gRPC 动态调用直接复用预编译 `DescriptorPool`，消除每请求二进制克隆与 `FileDescriptorSet` 重建。
  4. `profile_with_slots` 在槽位无需覆写时返回 `Cow::Borrowed(&UpstreamCryptoProfile)`，避免冗余配置克隆。

### 2.6 按连接槽位追踪配置同步状态

- **实现位置**：
  - `mazbri-bin/mazbri-gateway/src/http2_upstream_client/mod.rs`（`PooledChannel`）
  - `mazbri-bin/mazbri-gateway/src/gateway/execution/http_upstream.rs`（`invoke_bypass`）
- **运行机制**：
  1. `invoke_bypass` 先通过 `pick_slot` 选定具体 HTTP/2 连接槽位，再依据该槽位的 `is_synced` 状态决定是否附带全量配置。
  2. 仅在 gRPC 返回成功响应后调用 `slot.mark_synced`；并发冷启动请求在首个响应确认前均携带全量配置，配合 Bypass 侧 `try_get_with` 单飞解析确保零漏发与单次解密。
  3. 遇到 `CONFIG_CACHE_MISS` 时固定在同一槽位重发全量配置；遇到底层 HTTP/2 `ready` 或 `unary` 错误时立即调用 `slot.clear_synced()`，防止连接自动重连至新副本后漏发全量配置。

### 2.7 微秒级外部网络计时、异步批写与热路径日志治理

- **实现位置**：
  - `mazbri-libs/mazbri-upstream-contracts/protos/shared/v1/common.proto`（`InvokeUpstreamMeta.upstream_wire_us`）
  - `mazbri-libs/mazbri-upstream-runtime/src/types.rs`（`WireClock`）
  - `mazbri-bin/mazbri-gateway/src/interaction_store.rs`（`enqueue_interaction_event`、`drain_on_shutdown`）
  - `mazbri-bin/mazbri-gateway/src/gateway/handler.rs`、`src/gateway/execution/channel.rs`
- **运行机制**：
  1. **全链路微秒级纯网络计时**：`WireClock` 累加单次调用内全部外部网络 I/O 耗时（含 HTTP 请求头与响应体读取、gRPC 调用、WebSocket 握手与帧往返、动态换票实时网络请求、Digest 401 质询重放及认证拒绝重试首轮网络耗时），通过 `upstream_wire_us` 回传网关；微服务直调路径在 `execute_plain_request` 实测 RPC 耗时。
  2. **先减 `Duration` 后取整毫秒**：`handler.rs` 以 `Duration` 精度计算 `bypass_hop`（`fetch_wall_duration - cache_lookup_elapsed - wire_duration`）与 `internal_duration`（`elapsed - wire_duration`），再统一转换为毫秒并向 Prometheus 直方图写入亚毫秒浮点秒数。
  3. **交互事件异步 `UNNEST` 批写与后置快照异步化**：`interaction_store.rs` 通过有界通道聚合批次（上限 128 条或 5 ms 窗口），单事务执行两条 `UNNEST` 批量插入并在停机阶段通过 `drain_on_shutdown` 排空；CBPro 成功快照写入通过 `spawn_store_fallback_snapshot` 移出响应主路径。
  4. **热路径日志降噪与异步落盘**：`channel.rs` 通道尝试日志使用 `debug!` 级别；Bypass 访问行与四段阶段日志（`egress_build`、`upstream_auth`、`upstream_wire`、`response_decode`）统一在响应返回后的后台任务中通过无正文快照 `BypassLogSnapshot` 输出，既不在主线程格式化日志，又完整保留 ClickHouse 全链路阶段追踪记录。

---

## 3. 进一步压低单跳至 `< 0.2 ms` 的架构演进路线

双容器持久 HTTP/2 + mTLS 架构存在 `0.9–1.3 ms` 的跨容器协议栈与内核调度底噪。需要亚毫秒至 `< 0.2 ms` 平台延迟的场景可按以下阶梯演进：

| 演进阶梯 | 传输与执行方式 | 预期平台跳延迟 | 适用前提与边界约束 |
| :--- | :--- | :--- | :--- |
| **阶梯 A：同机 UDS + h2c** | 同宿主机或同 Pod 挂载共享 Unix Domain Socket，内部链路运行明文 HTTP/2（依赖文件系统 UID/GID 权限隔离） | `0.3–0.6 ms` | 保持双进程隔离；省去 Docker 虚拟网桥转发与双端 4 次 mTLS 加解密，仅支持同机部署 |
| **阶梯 B：双模执行后端（进程内直调 + 远程 Bypass 可选）** | 常规渠道在 `mazbri-gateway` 进程内直接调用 `mazbri-upstream-runtime::RuntimeEngine::invoke`；高隔离或固定出口 IP 渠道走远程 Bypass gRPC | `< 0.1–0.2 ms` | 消除跨进程序列化、网络栈与线程切换；Gateway 进程需持有 `MAZBRI_SUPPLIER_SECRETS_KEY` |

### 阶梯 B（Gateway 进程内直调）工程约束

1. **有状态组件按副本数折算**：`RuntimeEngine` 内部维护进程级 `TokenCache`、`WsPool`（`ws_max_concurrent` 并发控制）、`RetryBudgets` 与上游 `reqwest` 连接池。内嵌至 $N$ 个 Gateway 副本时，需按副本数折算并发配额或将换票缓存下沉至 Redis。
2. **管理面连接器试运行探针**：`mazbri-mgmt` 工作台在线试运行通过 `bypass_probe_url`（`:8890`）调用 `execute_probe_invoke` 采集脱敏阶段快照，演进时保留独立探针服务或在 `mazbri-mgmt` 内挂载探针模式的 `RuntimeEngine`。
3. **跨节点固定出口 IP 拓扑兼容**：要求固定公网白名单 IP 的第三方上游需继续通过独立出口节点的 `mazbri-bypass` 访问，渠道与供应商配置需支持 `in_process | remote_bypass` 双后端路由。
4. **链路拓扑 Span 兼容**：`mazbri-bin/mazbri-analytics/src/greptimedb/traces.rs` 通过网关 `upstream.attempt` span 及其下游子 span（`upstream.http`）推导平台至上游供应商拓扑边，进程内直调路径需保留同名子 span 与阶段日志维度。
