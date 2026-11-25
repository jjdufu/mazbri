# Step 05：异步运行时取消安全、连接池、响应缓存隔离与通道级熔断治理

## 1. 检测出的问题清单与源码定位

| 编号 | 级别 | 问题描述 | 精确源码位置 |
| :--- | :--- | :--- | :--- |
| **S05-1** | High | `TokenCache` Singleflight 违反 Tokio 取消安全性：Leader 在 `f().await` 期间若因调用请求超时取消被 Drop，`in_flight` 键永久残留导致后续请求持续报错 `"Singleflight sender dropped"`；且 Follower 在闭包外重复写回 `entries` 存在旧票覆盖新票竞态 | [token_cache.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-upstream-runtime/src/token_cache.rs#L26-L57)、[token_cache.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-upstream-runtime/src/token_cache.rs#L152-L173) |
| **S05-2** | High | `WsPool` / `ws_unary` 违反取消安全性：`WsConnGuard` 默认 `broken = false`，等待响应超时取消时将仍有未读帧的脏连接放回空闲池，配合 `id == request_id || id.is_empty()` 引发**跨租户 WebSocket 响应串流**；动态换票生成新 `PoolKey` 无后台回收导致连接句柄泄漏；且握手忽略自定义 `UpstreamTlsSpec` | [ws_pool.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-upstream-runtime/src/ws_pool.rs#L273-L304)、[ws_pool.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-upstream-runtime/src/ws_pool.rs#L433-L456)、[ws_unary.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-upstream-runtime/src/adapters/ws_unary.rs#L40-L45)、[ws_unary.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-upstream-runtime/src/adapters/ws_unary.rs#L280-L285) |
| **S05-3** | High | 响应缓存跨租户串漏与降级污染：缓存键不含 `tenant_id` 与路由请求头（如 `X-Api-Version`），Query 无长度前缀转义；`clear` 只删 Redis 漏清 L1 `MemoryCache`；`spawn_cache_populate` 未拦截 `fetched.from_cbpro_fallback` 将故障降级快照以完整 TTL 写入主缓存；无路径约束时非幂等 `POST` 被泛化缓存短路 | [response_cache.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-gateway/src/gateway/response_cache.rs#L130-L153)、[store.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-cache/src/store.rs#L118-L124)、[handler.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-gateway/src/gateway/handler.rs#L595-L617) |
| **S05-4** | High | 上游响应 `resp.bytes().await.unwrap_or_default()` 无上限读取可致 OOM，且将中途断流错误吞没为空响应 `200 OK` 缓存并扣费；Gateway 允许 10MiB 请求体而 Gateway↔Bypass Tonic 默认限制 4MiB，超限报错后清空槽位并对非幂等请求盲目重试；H2 流指标取消泄漏；连接池冷启动与过期重连无单飞保护 | [http_call.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-upstream-runtime/src/http_call.rs#L141)、[server.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-bypass/src/server.rs#L83)、[mod.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-gateway/src/http2_upstream_client/mod.rs#L154-L162)、[mod.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-gateway/src/http2_upstream_client/mod.rs#L321-L325) |
| **S05-5** | High | 熔断器被外层 `tokio::time::timeout` 截断导致超时从不计入失败、`HalfOpen` 无探测并发上限、上游 HTTP 5xx 被计为成功清零失败计数、产品级键 `product:{id}` 连坐封杀健康备用通道；分布式熔断器在常态 `Closed` 下每笔成功请求均写库放大负载 | [channel.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-gateway/src/gateway/execution/channel.rs#L112-L176)、[breaker.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-core/src/circuit_breaker/breaker.rs#L78-L251)、[distributed_circuit_breaker.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-core/src/distributed_circuit_breaker.rs#L265-L285) |
| **S05-6** | High | `mazbri-registry` 硬编码 `config_version = 0` 使 8 个服务每 30s 无限触发配置重载，且 `fetch_config` 失败时向管线注入空串 `""`；`config_watch` 在 `notify_rx` 关闭后陷入无休眠死循环，`PgListener` 重连后漏发唤醒信号，且 `bump_config_version_and_notify` 仅执行 `UPDATE` 未做 `UPSERT` 遇新键必报 `RowNotFound` | [heartbeat.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-registry/src/registry/heartbeat.rs#L30-L63)、[config_watch.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-core/src/config_watch.rs#L105-L120)、[config_watch.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-core/src/config_watch.rs#L213-L295) |
| **S05-7** | Medium | `bootstrap.rs` 创建两份独立脱节 `AdminState` 导致 `:9000/routes/status` 恒返 `404`；`RedisClient::run` 对 `SET NX EX` 断连自动重试误判首包重放；`cbpro-ext` 内存无上限、大 Body 超越 PG B-Tree 2704B 上限且读写主键失配；`mazbri-callback` 缺少崩溃 `'processing'` 租约回收与围栏；`soap_xml`/`graphql`/`sign_form`/`grpc` 适配器边界缺陷；Bypass 未校验全量配置哈希且探针共用生产状态 | [bootstrap.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-service/src/bootstrap.rs#L311-L329)、[client.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-core/src/redis/client.rs#L362-L372)、[manager.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cbpro-ext-service/src/manager.rs#L58-L120)、[repository.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-callback/src/repository.rs#L70-L140)、[service.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-bypass/src/service.rs#L175-L234) |

---

## 2. 第一性原理本质归因

本步对应 **公理 A5（异步取消异常安全）**、**公理 A1（空间隔离）** 与 **公理 A4（状态流同步）**：
1. **默认失效（Fail-Closed on Drop）是异步资源池的唯一正确不变量**：在协作式异步运行时中，Future 可在任意 `.await` 挂起点被同步销毁。借出长连接、抢占 Singleflight 槽位、递增在途计数或认领数据库异步任务时，初始状态必须定义为“未完结/已污染”，唯有走完完整协议闭环显式提交才允许复用或完结。
2. **故障隔离单元必须与物理故障域同构**：多通道故障转移（Failover）的目标是“主物理通道异常时秒切健康备通道”，将熔断器绑定在逻辑产品 `product_id` 上违背了物理故障域隔离原则。

---

## 3. 详尽可落地解决方案

### 3.1 异步资源池与 Singleflight 取消安全改造（`mazbri-upstream-runtime` & `mazbri-gateway`）

1. **`TokenCache` 脱离单请求取消生命周期 + 单点带代次写回**
   - 在 [token_cache.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-upstream-runtime/src/token_cache.rs) 中，`Group::work` 选出 Leader 后将换票异步闭包提交至独立的 `tokio::spawn` 任务运行（绑定自身独立的 `token_fetch_timeout`，不受发起请求中途断连影响），任务内挂载 `DropGuard` 确保无论成功、报错或 Panic 均从 `in_flight` 移除键并广播结果。
   - `entries.insert(cache_key, cached)` 收敛至 Leader 任务内部完成（写入前校验 `generation` 未被并发 `invalidate` 推进）；Follower 仅消费广播结果不写缓存；同时将 `client_secret` / 私钥及 `kid` 摘要纳入 `token_cache_key` 与 `jwt_cache_key`，使轮换上游密钥后即时换取新票。
2. **`WsPool` 默认脏标记归池、服务端随机帧关联与后台巡检**
   - 将 [ws_pool.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-upstream-runtime/src/ws_pool.rs) 的 `WsConnGuard::checkout` 初始状态设为 **`broken: true`**；仅在 [ws_unary.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-upstream-runtime/src/adapters/ws_unary.rs) 完整读到匹配响应帧后显式调用 `guard.mark_clean()`，中途超时取消被 Drop 的连接自动关闭销毁。
   - `ws_unary`：生成服务端随机关联号 `corr_id = Uuid::now_v7().to_string()` 覆盖外发帧 `corr_field`（收到响应后还原租户原始值），响应匹配严格要求 `parsed_corr_id == corr_id`，删除 `|| id.is_empty()` 分支并支持数字与字符串关联号归一匹配。
   - 在 `WsPool::new` 启动 30s 定时清理协程驱逐超 `idle_ttl` 空闲连接并回收空 `KeySlot`；握手接入 `UpstreamTlsSpec`（自定义根 CA 与客户端 mTLS）并限制单帧最大 `16 MiB`。
3. **Gateway 连接池单飞建连、流式限长读取与 gRPC 帧上限对齐**
   - 在 [http_call.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-upstream-runtime/src/http_call.rs)、`grpc.rs` 与 `auth.rs` 废除 `resp.bytes().await.unwrap_or_default()`，封装流式限长读取 `read_body_bounded(resp, max_bytes)`（业务响应上限 `16 MiB`，换票响应上限 `1 MiB`），超限返回 `UpstreamError::ResponseTooLarge`，中途流读取错误原样返回 `UpstreamError::Transport`。
   - 对齐 [server.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-bypass/src/server.rs) 与 [mod.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-gateway/src/http2_upstream_client/mod.rs) 的编解码上限：双端显式配置 `.max_decoding_message_size(16 * 1024 * 1024).max_encoding_message_size(16 * 1024 * 1024)`；用 RAII `GaugeGuard` 维护 `GATEWAY_UPSTREAM_H2_STREAMS_IN_USE`；对无幂等指纹的非幂等方法（`POST`/`PATCH`/`DELETE`）在请求字节发出后禁止传输层自动重发。
   - 为 `UpstreamHttp2ClientPool` 冷启动与 `TarpcClientPool` 过期重建补充按 `endpoint` 分片的异步单飞锁（并行建立 16 条 H2 通道，钳制 `ttl_secs.max(1)`），新连接就绪后原子替换旧连接；将 `auth_timeout_ms` 注入 `tarpc::context::Context.deadline`。
   - [service.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-bypass/src/service.rs)：收到全量配置写入缓存前重算 SHA-256 校验与请求 `hash` 一致；为 `:8890` `mgmt_probe` 实例化独立的轻量 `RuntimeEngine`，与生产数据面 `TokenCache`/`WsPool`/`RetryBudgets` 物理隔离。

### 3.2 响应缓存租户隔离、防降级污染与可缓存语义收敛

1. **强制租户命名空间与完整缓存键（`mazbri-gateway` & `mazbri-cache`）**
   - `build_proxy_cache_key` 与 `build_cbpro_cache_key` 统一纳入 `tenant_id`（默认强制按租户隔离；仅当管理员显式开启无状态公共共享且当前产品无租户级专属路由覆盖时使用 `__public__` 分区）、`product_id`、内层业务 `method`、`path`、规范化路由头以及带长度前缀编码 `len(k):k=len(v):v` 的排序 Query 与 Body SHA-256。
   - `CacheTarpcService::clear` 在清除 Redis 租户键的同时清空进程内 L1 `MemoryCache` 对应命名空间，并通过 `config_watch` 广播 `cache_l1_version` 驱动多副本同步清空本地 L1。
2. **阻断 CBPro 降级污染与规范可缓存方法**
   - 在 [handler.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-gateway/src/gateway/handler.rs) 的 `spawn_cache_populate` 入口增加守卫：`if fetched.from_cache || fetched.from_cbpro_fallback || !should_attempt_cache_rpc(cache_enabled) { return; }`。
   - 区分网关外层固定 `POST /api/v1/gateway` 与内层业务 `GatewayRequest.method`：未配置具体路径规则 `api_ttl` 的通配产品缓存仅缓存内层 `GET`/`HEAD`；显式配置了具体查询端点 `api_ttl` 规则时允许按配置的 `cacheable_methods`（含只读查询类 `POST`）执行缓存。

### 3.3 通道级单调时钟熔断器、幂等配置广播与异步回调租约恢复

1. **通道级熔断、半开并发控制与可观测指标对齐（`mazbri-core`、`mazbri-gateway` & `mazbri-analytics`）**
   - 将网关熔断器键由 `product:{product_id}` 改为 **`channel:{channel_id}`**（标签携带受控 `channel_code`），并将 `GatewayConfig` 中 `circuit_breaker.max_cache_size` 默认上限调至 `10,000`，主通道熔断后同一请求立即切流至健康备用通道。
   - 外层请求超时 `Err(Elapsed)` 或上游返回匹配 `switch_condition` 的 `5xx` 状态码时显式调用 `breaker.record_failure()`；[breaker.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-core/src/circuit_breaker/breaker.rs) 改用单调 `Instant` 计时，冷却届满当次经 `compare_exchange(Open, HalfOpen)` 直接放行首笔探测，并通过 `half_open_in_flight: AtomicU32` 限制半开最大并发探测数（默认 `2`）。
   - `DistributedCircuitBreaker` 在 `Closed` 且 `failure_count == 0` 的稳态成功路径跳过数据库 `UPSERT`；同步更新 [metrics.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/greptimedb/metrics.rs) 中熔断器概览与时序 PromQL 聚合规则，对齐新的通道维度标签。
2. **修复 `mazbri-registry`、`config_watch`（幂等 `UPSERT`）、`AdminState`、`RedisClient`、`cbpro-ext`、`mazbri-callback` 与协议适配器**
   - `mazbri-registry`：`cert-service` 在 `GET /configs/{service}` 响应中返回真实 `version` 字段，`HttpConfigClient::refresh()` 同步更新本地版本号供心跳上报；`fetch_config()` 失败时直接中止本轮更新保留现网配置，严禁传入空串 `""`；修复 `attempt.min(30)`、未注册自动重注册并接入配置的 `heartbeat_interval`。
   - `config_watch`（[config_watch.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-core/src/config_watch.rs)）：① `notify_rx.recv()` 返回 `None` 时将 `listen_active` 置为 `false` 消除空转死循环，`PgListener` 重连成功后立即触发一次版本补拉；② 将 `bump_config_version_and_notify` 改为幂等 `INSERT INTO mazbri_platform.route_sync_metadata (key, version, updated_at) VALUES ($1, 1, NOW()) ON CONFLICT (key) DO UPDATE SET version = route_sync_metadata.version + 1, updated_at = NOW()`，并在 [init.sql](file:///workspace/rust-proj/mazbri-max/init-dbs/src/sql/schemas/platform/init.sql) 预置 `auth_whitelist_version` 与 `cache_l1_version` 种子键，杜绝新增监听键报 `RowNotFound`。
   - `mazbri-service` & `mazbri-admin`：由 `AdminServer::state()` 返回挂载在路由上的唯一 `Arc<AdminState>`，删除 [bootstrap.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-service/src/bootstrap.rs) 中重复创建的脱节实例，修复 `:9000/routes/status` 404 并以 `(IpAddr, u16)` 兼容 IPv6 绑定。
   - `RedisClient`：拆分 `run_idempotent`（仅限只读命令）与 `run_once`（用于 `SET NX EX` 及有状态 `EVAL` 脚本），对连接密码做百分号编码并拦截空连接池调用。
   - `cbpro-ext`：统一主键为 `sha256("{tenant_id}\0{product_id}\0{method}\0{path}\0{canonical_query_body}")` 定长 64 字符 Hex，内存缓存改用有界 `moka::sync::Cache`，`CBProClient` 报错自动驱逐重连，`claim_validation_jobs` 纳入超时 `'running'` 回收并按 `worker_count` 并发执行。
   - `mazbri-callback`（[repository.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-callback/src/repository.rs) & [callback.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/callback.rs)）：在 `platform/init.sql` 的 `callback_delivery_logs` 表增加 `locked_at TIMESTAMPTZ` 与 `lease_seq BIGINT NOT NULL DEFAULT 0`；`fetch_pending_tasks` 自动回收 `status = 'processing' AND locked_at < NOW() - INTERVAL '5 minutes'` 的崩溃滞留任务并递增 `lease_seq`；`update_delivery_status` 校验 `lease_seq` 围栏；管理端 `retry_delivery` 同步支持重置超时滞留的 `'processing'` 记录。
   - 协议适配器：`soap_xml` 校验动态 XML 标签满足合法 `NCName`；`graphql` 与 `sign_form` 将 `ctx.is_reject_status(status)` 前置至 JSON 解析之前以恢复非 JSON `401` 自动换票；`grpc` 适配器通过 `BodyExt::collect().await` 读取 HTTP/2 `trailers()` 中的 `grpc-status`。

---

## 4. 二阶与三阶影响深度推演

| 影响阶次 | 潜在连锁反应场景 | 架构消解与防护设计 |
| :--- | :--- | :--- |
| **二阶影响 1：收敛 `POST` 缓存规则对只读查询型 `POST` 接口的影响** | 征信与数据核验上游常以 `POST` 承载只读查询（如 [dev19-mock-cache-seed.sql](file:///workspace/rust-proj/mazbri/docker/cwork/scripts/dev19-mock-cache-seed.sql) 为 `/dev19-mock` 配置 60s TTL）。若无差别禁掉所有内层 `POST` 缓存会使显式配置的查询缓存失效。 | 按“是否命中显式端点 `api_ttl` 规则”精确分流：命中显式路径规则的查询端点支持缓存 `POST`，无路径约束的通配缓存仅缓存 `GET`/`HEAD`，兼顾查询加速与写接口安全。 |
| **二阶影响 2：熔断维度改为 `channel:{channel_id}` 对运维看板熔断计数的影响** | [metrics.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/greptimedb/metrics.rs) 与运维总览实时熔断卡片会聚合 `mazbri_circuit_breaker_state == 2` 计数。 | 保持指标名与状态枚举值（`0=Closed, 1=HalfOpen, 2=Open`）不变，同步对齐 `analytics` 指标标签过滤与下钻链接，使通道熔断在运维总览实时准确反映。 |
| **三阶影响：新增 `config_watch` 监听键在跨模块调用时的可靠性** | 若未来新增配置广播键漏写初始化 SQL，纯 `UPDATE` 实现会在运行期直接抛错中断控制面写接口。 | `bump_config_version_and_notify` 改为原子 `INSERT ... ON CONFLICT DO UPDATE`，从基础设施原语层彻底消除键缺失导致的 `RowNotFound` 故障模式。 |

---

## 5. 验收门禁与实测清单

1. **关键回归测试**
   - 模拟 `TokenCache` Leader 换票耗时 500ms、首个请求在 100ms 超时取消：断言并发及后续请求正常获取新 Token，零 `"Singleflight sender dropped"`。
   - 模拟 `ws_unary` 请求 A 超时取消后上游延迟推回帧 A，随后租户 B 发起请求 B：断言旧连接已在取消瞬间关闭销毁，租户 B 建立洁净连接仅收到帧 B。
   - 模拟网关 `CallbackWorker` 认领回调任务置为 `'processing'` 后进程重启：断言超时阈值后新 Worker 自动重领投递完结，管理台对超时滞留记录可手动触发重投。
   - 模拟主通道连续超时或返回 `503` 达阈值：断言主通道熔断进入 `Open`，请求零等待切流至健康备通道返回 `200`，且网关 `:9000/routes/status` 返回 `200` 真实路由快照。
   - 运行 `mazbri-cert` 心跳循环 2 分钟：断言节点上报真实 `config_version >= 1`，无配置修改期间 `fetch_config` 调用次数为 `0`。
2. **部署与验证顺序**
   - 更新 `init-dbs` (`platform/init.sql`) 及各服务二进制（`cache`、`cbpro`、`cbpro-ext`、`bypass`、`gateway`、`analytics`、`mgmt`），执行 `redep rerun` 验证全链路通过。
