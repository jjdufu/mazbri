# Gateway ↔ Bypass 通信机制与热路径延迟优化方案

## 1. 通信架构与单跳耗时构成

### 1.1 进程职责与边界划分

Mazbri 数据面将入口网关治理与外部上游出站执行拆分为两个独立容器进程：

| 进程 | 核心职责 | 独占资源与边界 |
| :--- | :--- | :--- |
| `mazbri-gateway` | 租户鉴权、传输信封加解密、限流、余额预检、计费入队、路由匹配（`RouteEngine`）、熔断切换、响应信封组装 | 直连平台 PostgreSQL（加载路由规则、写入交互事件）与 Redis；内存持有密文态 `supplier_config`（`enc:v2:...`），不执行凭据解密 |
| `mazbri-bypass` | 封装共享引擎 `mazbri-upstream-runtime`，执行出站协议适配（HTTP/JSON、Form、SOAP、GraphQL、JSON-RPC、gRPC、WebSocket、Job Poll）、动态换票、上游报文加解密与重试 | 持有 `MAZBRI_SUPPLIER_SECRETS_KEY` 解密供应商凭据与客户端私钥；执行 SSRF 地址拦截与公网出站收敛；提供 `:8890` 管理探针接口供 `mazbri-mgmt` 免代码工作台在线试运行 |

### 1.2 传输协议栈与配置增量下发

1. **连接池与传输层**：Gateway 通过 `UpstreamHttp2ClientPool`（`mazbri-bin/mazbri-gateway/src/http2_upstream_client/mod.rs`）对每个上游端点默认维护 **16 条持久 HTTP/2 + 双向 mTLS 连接**，按原子游标轮询（round-robin）分发 gRPC Unary 请求（`BypassHttpProxyService/Execute`，Prost 编解码）。Bypass 通过 `BypassGrpcServer`（`mazbri-bin/mazbri-bypass/src/server.rs`）强制校验客户端证书并处理请求。
2. **配置哈希增量协议**：为避免每请求传输完整 JSON 配置，Gateway 在 `build_invoke_request`（`mazbri-bin/mazbri-gateway/src/gateway/execution/http_upstream.rs`）中计算 `egress_spec` 与 `supplier_config` 的 SHA-256 哈希。`SupplierConfigSendCache` 判定已下发过同哈希时，仅发送哈希与空配置串；Bypass 在本地 Moka 缓存（空闲过期 600 秒）中按哈希查找已解密配置。若 Bypass 重启或缓存淘汰，返回 `CONFIG_CACHE_MISS`，Gateway 捕获后作废发送标记并携带全量 JSON 自动重发一次。

### 1.3 双容器单跳耗时拆解（持续负载 `1.5–2.1 ms`）

```text
gateway worker ──► gateway h2 driver (mTLS 加密) ──► Docker veth / bridge / conntrack ──► bypass h2 driver (mTLS 解密) ──► bypass worker
gateway worker ◄── gateway h2 driver (mTLS 解密) ◄── Docker veth / bridge / conntrack ◄── bypass h2 driver (mTLS 加密) ◄── bypass worker
```

| 耗时分项 | 典型耗时（IP 上游 + 小报文） | 典型耗时（域名上游 + 中大报文） | 构成说明 |
| :--- | :--- | :--- | :--- |
| 跨容器网络与协议栈底噪 | `0.9–1.3 ms` | `0.9–1.3 ms` | 双端 Protobuf 编解码、HTTP/2 帧封装、4 次 TLS 加解密、Linux TCP + Docker 虚拟网桥转发、4 次跨任务/跨进程调度唤醒 |
| 双端热路径冗余计算与阻塞 | `0.45–0.85 ms` | `1.1–5.0+ ms` | 重复反序列化、整棵 JSON 树深比较、主路径大对象克隆、二次 JSON 编解码、同步阻塞 DNS 解析 |

---

## 2. 热路径代码缺陷与修复清单（保留双容器架构）

以下 7 项问题均属于纯代码级冗余或逻辑缺陷，无需改变 `gateway → bypass` 双容器部署架构即可直接修复。

### 2.1 Bypass 每请求触发 2 次同步阻塞 DNS 解析与重复规格校验

- **代码位置**：
  - `mazbri-libs/mazbri-upstream-runtime/src/engine.rs:495-539`（`RuntimeEngine::preflight`）
  - `mazbri-libs/mazbri-upstream-runtime/src/http_call.rs:55`（`send_http`）
  - `mazbri-libs/mazbri-upstream-runtime/src/ssrf.rs:117-174`（`SsrfChecker::validate_url`）
- **现状与根因**：
  1. `preflight()` 在每次请求入口重跑 `validate_egress_spec(&input.spec)`（该规格在写入 Bypass Moka 缓存时已由 `parse_and_validate_egress` 校验通过），随后调用第 1 次 `ssrf.validate_url(&base_url).await`（含换票 `token_url` 时再调 1 次）。
  2. 进入适配器调用 `send_http()` 时，第 55 行再次调用 `ssrf.validate_url(call.url).await`。
  3. 当上游地址为域名而非字面量 IP 时，`validate_url` 每次都通过 `tokio::task::spawn_blocking(move || lookup_target.to_socket_addrs()).await` 执行系统同步 DNS 查询——即便 `reqwest::Client` 已复用 Keep-Alive 连接且已挂载连接期防重绑定解析器 `SsrfSafeResolver`（`ssrf.rs:110-112`）。
- **修复方案**：
  1. 对来自已校验缓存的 `ChannelEgressSpec` 跳过每请求 `validate_egress_spec`。
  2. 将请求预检 `validate_url` 收敛为纯内存校验（URL scheme、元数据主机名黑名单、字面量私网 IP 拦截），移除 `preflight` 与 `send_http` 的双重调用，将域名解析与重绑定拦截统一交由连接建立期的 `SsrfSafeResolver` 处理（或为预检 DNS 增加带短 TTL 的异步缓存）。
- **预期效果**：
  - 字面量 IP 上游场景每请求节省 **`20–50 µs`**；
  - 域名上游场景消除每请求 **2 次跨线程池唤醒与同步 DNS 查询**，单请求节省 **`0.3–2.0+ ms`**，消除高并发下 Tokio blocking 线程池排队引发的 P99 毛刺。

### 2.2 Gateway 单次请求 5 次反序列化 `SupplierConfig`、4 次解析 `EgressSpec` 与整树深比较

- **代码位置**：
  - `mazbri-bin/mazbri-gateway/src/gateway/execution/types_and_entry.rs:357-370`
  - `mazbri-bin/mazbri-gateway/src/gateway/execution/channel.rs:56-71, 111-207, 216-235`
  - `mazbri-bin/mazbri-gateway/src/gateway/execution/http_upstream.rs:15, 177-212`
- **现状与根因**：
  1. `upstream.attempt` span 包裹整个 `run_channel_attempt`（含熔断器查找、超时包装与 `execute_http_upstream_request`）。
  2. 在进入 `http_upstream.rs:15` 前，`execute_gateway_request_with_event` 与 `execute_channel_attempt` 连续调用 **4 次** `resolve_mode_and_contract_hint`（构造 `primary_route`、`last_attempted_route`、`mode_hint` 及 `execute_channel_attempt` 内部各 1 次），每次均无缓存裸调 `SupplierConfig::deserialize(&channel.supplier_config)` 与 `parse_channel_config_egress(&channel.channel_config)`。
  3. `execute_http_upstream_request` 第 15 行执行第 5 次 `SupplierConfig::deserialize`；随后 `cached_egress` 与 `cached_supplier` 在命中 `DashMap` 时执行 `e.source == *source`，对整棵 `serde_json::Value` 与 `SupplierConfig` 做递归深比较。
- **修复方案**：
  - 在 `mazbri-bin/mazbri-gateway/src/route_source.rs:109-136` 加载路由构建 `CandidateChannel` 时，一次性预解析并挂载不可变的 `Arc<PreparedChannelConfig>`（内含 `SupplierConfig`、`ChannelEgressSpec`、`mode`、`contract_id`、预序列化的 JSON 字符串及 SHA-256 哈希），热路径直接按字段引用读取，移除全局 `EGRESS_CACHE` / `SUPPLIER_CACHE` 的运行期深比较。
- **预期效果**：
  - 单次请求 5 次 `SupplierConfig` 反序列化、4 次无缓存 `EgressSpec` 解析、2 次整树深比较全部降为 **0 次**，Gateway 去程本地 CPU 耗时稳定减少 **`0.25–0.40 ms`**。

### 2.3 双端主路径正文二次编解码与 4 处大对象克隆

- **代码位置**：
  - `mazbri-bin/mazbri-gateway/src/gateway/execution/http_upstream.rs:337, 428-447`
  - `mazbri-bin/mazbri-bypass/src/service.rs:334-337, 822-823`
  - `mazbri-bin/mazbri-gateway/src/gateway/handler.rs:750`
  - `mazbri-bin/mazbri-gateway/src/gateway/serializer.rs:18-27, 110-145`
- **现状与根因**：
  1. **Gateway 去程无条件克隆**：`invoke_bypass` 循环首轮即执行 `request.clone()`，在无需重试的常规成功路径上深拷贝整个 `InvokeRequest`（含完整 `payload_json`）。
  2. **Bypass 异步日志前主线程全量克隆**：`run_traced_invoke` 在 `tokio::spawn` 前执行 `let req_owned = req.clone(); let response_owned = outcome.response.clone();`，深拷贝完整的请求与响应 `payload_json`，而日志函数 `emit_bypass_access` 仅读取 `.payload_json.len()` 字节长度。
  3. **Gateway 回程二次 JSON 编解码**：`invoke_response_to_upstream` 将 Bypass 返回的合法 JSON 字符串 `response.payload_json` 反序列化为 `serde_json::Value`，`finish_request` 克隆整棵 `Value` 树传给后置任务，随后 `serializer.rs` 构造 `SuccessEnvelope { data: serde_json::Value, ... }` 时再次将其 `to_vec` 序列化为字节流。注意：现有 `serializer.rs:110-119` 对 `ProxyBody::JsonRaw` 仍会调用 `serde_json::from_slice::<Value>`，仅改枚举分支无法省去解析。
- **修复方案**：
  1. `invoke_bypass` 首轮直接消费请求（或仅在 `CONFIG_CACHE_MISS` / 传输重试慢路径按需克隆）。
  2. Bypass 在 `tokio::spawn` 前仅提取标量日志上下文（`request_bytes`、`response_bytes`、状态码与路由标识），将 `outcome.response` 直接移交 gRPC 返回。
  3. 将 `ProxyBody` 回程载荷存为共享原始 JSON 切片（`Arc<str>` / `Arc<[u8]>`），并将 `SuccessEnvelope` 的 `data` 序列化类型改为 `&serde_json::value::RawValue`，直接内嵌已校验 JSON 文本；仅在后置任务实际命中活跃回调订阅时才按需解析 `Value`。
- **预期效果**：
  - 小报文（`< 1 KB`）每请求节省 **`0.10–0.20 ms`**；
  - 中大报文（`10–100 KB`）消除 **2 次全树 JSON 解析/序列化与 4 次大块内存拷贝**，单请求节省 **`0.5–3.0+ ms`**，显著降低内存带宽占用。

### 2.4 路由匹配与通道限流每请求深拷贝候选通道配置树

- **代码位置**：
  - `mazbri-libs/mazbri-routing/src/engine.rs:30-53, 130-145`
  - `mazbri-bin/mazbri-gateway/src/gateway/execution/types_and_entry.rs:268-289`
  - `mazbri-bin/mazbri-gateway/src/ratelimit/channel_limit.rs:216-233`
- **现状与根因**：
  1. `CandidateChannel` 直接内联持有 `supplier_config: Value`、`channel_config: Value` 与十余个 `String`；每次 `RouteEngine::match_route`（开启响应缓存命中时还会额外调用 `match_primary_route_metadata`）均通过 `.cloned()` 深拷贝该路由下全部候选通道。
  2. `check_channel_rate_limit` 在判断 `channel.qps_limit <= 0`（默认不限流）之前，先克隆整棵 `channel.channel_config` JSON 树并创建 `check_channel_rate_limit` span，进入 `check()` 后立即短路返回。
- **修复方案**：
  1. 将 `RouteEngine` 内部存储及 `match_route` 返回值改为 `Vec<Arc<CandidateChannel>>`（或 `SmallVec<[Arc<CandidateChannel>; 4]>`），匹配过程仅递增引用计数。
  2. 在调用 `check_channel_rate_limit` 前对 `!enabled || channel.qps_limit <= 0` 直接内联短路；将排队限流参数（`mode`、`max_wait_ms`、`max_queue_depth`）预解析为轻量 `Copy` 结构体存入 `CandidateChannel`，移除运行期 `Value` 传递。
- **预期效果**：
  - 每请求减少数十至上百次小块堆内存分配与 1 个无效 OTel span，节省 **`30–100 µs`**，降低多核高并发下的全局分配器锁争用。

### 2.5 Bypass 缓存命中克隆与密码流水线每请求重建

- **代码位置**：
  - `mazbri-bin/mazbri-bypass/src/service.rs:41-44, 177, 465-478`
  - `mazbri-libs/mazbri-upstream-runtime/src/engine.rs:628-631`
  - `mazbri-libs/mazbri-crypto/src/registry.rs:80-86`
  - `mazbri-libs/mazbri-upstream-runtime/src/adapters/http_json.rs:35`、`src/crypto_pipeline.rs:172`
- **现状与根因**：
  1. `CachedEgressSpec` 中的 `spec: ChannelEgressSpec` 未包裹 `Arc`，每次 Moka 缓存命中深拷贝 1 次，`build_invoke_input` 再次深拷贝 1 次。
  2. `build_invoke_input` 每请求克隆整张 `auth.credentials` 哈希表，并重新执行 `pipeline_keys_from_credentials` 做 hex / base64 密钥解码与 `Vec<u8>` 分配。
  3. `RuntimeEngine::attempt` 每请求调用 `AlgorithmRegistry::default_registry()` 新建 `HashMap` 并遍历注册全部 20+ 种算法元数据；启用上游加密时每请求重复执行 `profile.clone()` 与 `profile.validate()`。
- **修复方案**：
  1. `CachedEgressSpec` 与 `InvokeInput.spec` 统一改为 `Arc<ChannelEgressSpec>`。
  2. 在 `CachedSupplierConfig` 解密入库时一次性预生成 `Arc<PipelineKeys>`，请求期零分配复用。
  3. 为 `AlgorithmRegistry` 提供 `LazyLock<AlgorithmRegistry>` 进程级静态单例引用。
- **预期效果**：
  - 消除 Bypass 固定分配与密钥解码开销，每请求节省 **`30–80 µs`**（启用上游国密/AEAD 报文加解密的渠道收益更高）。

### 2.6 `SupplierConfigSendCache` 非原子竞争与多副本 Cache-Miss 往返

- **代码位置**：
  - `mazbri-bin/mazbri-gateway/src/supplier_config_cache.rs:28-43`
  - `mazbri-bin/mazbri-gateway/src/gateway/execution/http_upstream.rs:140-150, 349-354`
- **现状与根因**：
  1. `should_send_full` 与 `mark_full_sent` 拆分为两次独立的 `DashMap` 访问：冷启动或配置变更瞬间，同一渠道的并发请求会同时读到 `true`，携带全量 JSON 并发冲击 Bypass，触发重复 JSON 解析与 `MAZBRI_SUPPLIER_SECRETS_KEY` AEAD 解密。
  2. 发送状态仅按 Gateway 进程维度记录：当集群部署多个 `mazbri-bypass` 副本且 16 条 HTTP/2 连接跨副本分布时，Gateway 向副本 A 发送全量后即只发哈希，后续轮询至副本 B 的请求必触发 `CONFIG_CACHE_MISS` 重试。
- **修复方案**：
  1. 使用 `DashMap::entry` 将哈希比对与更新合并为单次原子操作。
  2. 将已同步哈希表下沉至 `UpstreamHttp2ClientPool` 的单条 `Channel` 包装结构内，按底层连接粒度独立追踪对端缓存状态。
- **预期效果**：
  - 消除冷启动并发解密毛刺；在多副本 Bypass 部署下彻底消除偶发 **`+1.5–2.0 ms`（额外一整轮 mTLS/gRPC RTT）** 的尾延迟尖峰。

### 2.7 耗时度量口径失真（`upstream_wire_ms` / `bypass_hop_ms`）与热路径未采样日志

- **代码位置**：
  - `mazbri-libs/mazbri-upstream-runtime/src/http_call.rs:90-100`
  - `mazbri-libs/mazbri-upstream-runtime/src/engine.rs:891, 935`
  - `mazbri-bin/mazbri-gateway/src/gateway/handler.rs:182-186`
  - `mazbri-bin/mazbri-gateway/src/gateway/execution/channel.rs:122`
- **现状与根因**：
  1. **指标张冠李戴**：`http_call.rs:100` 计算出真实出站 HTTP 耗时 `_elapsed` 后直接丢弃；`Trace::upstream_meta` 将从 `RuntimeEngine::invoke_inner` 入口计时的 `elapsed_ms(self.start)` 写入 `upstream_wire_ms`，导致 Bypass 内部的配置校验、SSRF DNS、动态换票、报文加解密及内部重试轮耗时全部被算作“纯外部上游网络耗时”。
  2. **整毫秒截断放大误差**：各处使用 `as_millis()` 向下取整后再在 `handler.rs:184` 执行 `fetch_wall_ms - cache_lookup_ms - upstream_ms`，产生 `±1 ms` 的量化跳变（如 `2.05 ms → 2` 减 `0.95 ms → 0` 得 `2 ms`）。
  3. **热路径全量同步日志**：`channel.rs:122` 在每次通道尝试时无条件打印未采样的 `info!` 日志，违背 `stage_log` 默认 1/20 采样的热路径降噪策略。
- **修复方案**：
  1. 在 `OutboundResponse` 中携带真实出站网络 `wire_duration: Duration`（含读取响应体完成时间），以微秒精度回传（如新增 `upstream_wire_us` 或修正 `upstream_wire_ms` 数据源并在网关侧以 `Duration` 差值计算后再取整）。
  2. 将 `channel.rs:122` 的全量 `info!` 降级为 `debug!`（审计与排障信息已由采样阶段日志与访问日志完整覆盖）。
- **预期效果**：
  - 移除热路径同步日志格式化每请求节省 **`10–30 µs`**；
  - 校准 `upstream_wire_ms`、`bypass_hop_ms`、`platform_local_ms` 三项核心指标的物理真实性，消除排障误判。

### 2.8 纯代码修复收益汇总（不改双容器架构）

| 修复项 | IP 上游 + 小报文基准收益 | 域名上游 + 中大报文 + 多副本收益 | 主要改善维度 |
| :--- | :--- | :--- | :--- |
| 1. 移除每请求 2 次同步阻塞 DNS 与重复校验 | `0.02–0.05 ms` | **`0.30–2.00+ ms`** | 消除 blocking 线程池排队与同步 DNS 延迟 |
| 2. 路由加载期预解析 `SupplierConfig` / `EgressSpec` | **`0.25–0.40 ms`** | **`0.25–0.40 ms`** | 消除 Gateway 去程 5 次重复解析与整树深比较 |
| 3. `RawValue` 回程直嵌 + 移除 4 处主路径正文克隆 | `0.10–0.20 ms` | **`0.50–3.00+ ms`** | 消除二次 JSON 编解码与大块内存复制 |
| 4. `CandidateChannel` `Arc` 化 + 通道限流前置短路 | `0.03–0.10 ms` | `0.05–0.15 ms` | 降低高频小对象分配与分配器锁竞争 |
| 5. Bypass `EgressSpec` `Arc` 化 + 密码注册表单例化 | `0.03–0.08 ms` | `0.05–0.12 ms` | 消除 Bypass 每请求固定分配税 |
| 6. 按连接原子追踪配置同步哈希 | 消除冷启动解密毛刺 | **消除 `+1.5–2.0 ms` 翻倍 RTT** | 根除多副本与缓存淘汰期的尾延迟尖刺 |
| 7. 修正 `upstream_wire_ms` 口径 + 降级热路径 `info!` | `0.01–0.03 ms` | `0.01–0.03 ms` | 降低日志 I/O 并恢复毫秒级监控可信度 |
| **合计净收益** | **单请求稳降 `0.45–0.85 ms`** | **单请求稳降 `1.1–5.0+ ms`** | 双容器单跳收敛至 **`0.9–1.3 ms`** 物理底噪区间 |

---

## 3. 进一步压低单跳至 `< 0.2 ms` 的架构演进路线

完成第 2 节全部代码修复后，剩余的 `0.9–1.3 ms` 为跨容器 TCP + mTLS + HTTP/2 + 4 次任务唤醒的物理底噪。若业务需要进一步压缩至亚毫秒乃至 `< 0.2 ms`，按以下两个阶梯推进：

| 演进阶梯 | 传输/执行方式 | 预期平台跳延迟 | 适用前提与边界代价 |
| :--- | :--- | :--- | :--- |
| **阶梯 A：同机 UDS + h2c** | 同宿主机/同 Pod 挂载共享 Unix Domain Socket，内部链路改用明文 HTTP/2（依赖文件系统 UID/GID 权限隔离） | `0.3–0.6 ms` | 保持双进程隔离；省去 Docker `veth`/bridge/`conntrack` 与双端 4 次 mTLS 加解密，但禁止跨机部署 |
| **阶梯 B：双模执行后端（In-Process 直调 + 远程 Bypass 可选）** | 常规渠道在 `mazbri-gateway` 进程内直接调用 `mazbri-upstream-runtime::RuntimeEngine::invoke`；高隔离或固定出口 IP 渠道仍走远程 Bypass gRPC | **`< 0.1–0.2 ms`** | 彻底消除跨进程序列化、网络栈与线程切换；Gateway 需注入 `MAZBRI_SUPPLIER_SECRETS_KEY` |

### 实施阶梯 B（Gateway 进程内直调）的 4 项工程约束

1. **有状态组件按副本数放大**：`RuntimeEngine`（`mazbri-libs/mazbri-upstream-runtime/src/engine.rs:160-171`）内部维护进程级 `TokenCache`、`WsPool`（`ws_max_concurrent` 并发控制）、`RetryBudgets` 与上游 `reqwest` 连接池。内嵌至 $N$ 个 Gateway 副本后，对上游供应商的换票请求频率与最大并发连接数将变为 $N$ 倍，需按副本数折算配额或将换票缓存下沉至 Redis。
2. **管理面连接器试运行探针保留**：`mazbri-mgmt` 免代码工作台的在线试运行通过 `bypass_probe_url`（`:8890`）调用 `execute_probe_invoke` 采集脱敏阶段快照。演进时需保留轻量 Bypass 供探针与远程出口使用，或在 `mazbri-mgmt` 内直接挂载探针模式的 `RuntimeEngine`。
3. **跨机器固定出口 IP 拓扑兼容**：集群模式下部分第三方上游要求固定公网白名单 IP，仍需将 `mazbri-bypass` 独立部署于专用出口节点（通过 `WAIT_DEPS_SERVICES` 协调启动），因此应在渠道/供应商配置上支持 `in_process | remote_bypass` 双后端路由，不可直接删除 gRPC 客户端路径。
4. **链路拓扑分析 Span 兼容**：`mazbri-bin/mazbri-analytics/src/greptimedb/traces.rs:422-435` 通过查询网关 `upstream.attempt` span 及其下游出站子 span 推导「平台 → 上游供应商」拓扑边。进程内直调路径必须保留同名子 span（`upstream.http`）与阶段日志维度，确保监控大盘服务拓扑不丢边。
