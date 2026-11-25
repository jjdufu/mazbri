# Step 01：进程级 Panic 隔离闭环、单调时钟解耦与探针就绪语义校准

## 1. 检测出的问题清单与源码定位

| 编号 | 级别 | 问题描述 | 精确源码位置 |
| :--- | :--- | :--- | :--- |
| **S01-1** | Critical | `DistributedCircuitBreaker` 将数据库 `DateTime<Utc>` 转换为本机单调时钟时直接执行 `Instant::now().checked_sub(duration).unwrap()`；宿主机重启或新节点扩容后 `uptime < duration` 必返回 `None` 触发 Panic，经 `panic_policy` 直接 `abort()` 终止进程 | [storage_sync.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-core/src/distributed_circuit_breaker/storage_sync.rs#L31-L46) |
| **S01-2** | Critical | `mazbri-cbpro-service` 的 `HybridFallbackCache::get` 从 Redis 回填 L1 缓存时同样基于年龄做单调时钟逆推或重置创建时间，节点刚开机或跨机时钟微偏存在下溢崩溃与年龄失真风险 | [cache.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cbpro-service/src/cache.rs#L73-L88) |
| **S01-3** | Critical | `mazbri-callback` 未校验 `retry_policy` 范围，`calculate_next_retry` 在较大乘数与重试次数下饱和返回 `i64::MAX`，传入 `chrono::Duration::seconds(delay)` 直接 Panic；因 `CallbackWorker` 运行于网关后台任务（非可恢复域），导致 `mazbri-gateway` 陷入启动即 `abort()` 的毒丸死循环 | [retry.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-callback/src/retry.rs#L4-L10)、[worker.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-callback/src/worker.rs#L261-L271) |
| **S01-4** | Critical | `mazbri-cert-service` 的 `GET /certificates?offset=10&limit=0` 执行 `(offset / limit) + 1` 触发除零 Panic，且 `issue`/`renew` 未校验极大或负数 `validity_days` 触发 `Duration::days` 溢出 Panic，单请求直接击穿终止 `cert-service` 进程 | [list.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/handlers/certificate/list.rs#L59-L68)、[issue.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/handlers/certificate/issue.rs#L51-L53)、[renew.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/handlers/certificate/renew.rs#L131-L143) |
| **S01-5** | High | `mazbri-analytics`、`mazbri-cert-service`、`mazbri-bypass`（gRPC 与管理探针服）均安装了全局 `panic_policy` 但未挂载 `CatchPanicLayer` + `recoverable_request_middleware`；且 `mazbri-gateway` / `mazbri-mgmt` 请求内派生的 `tokio::spawn`（如 `post_response`、`spawn_cache_populate`）与 `spawn_blocking` 脱离线程局部 `RECOVERABLE_DEPTH`，子任务单点异常即杀死整进程 | [panic_policy.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-core/src/panic_policy.rs#L36-L40)、[router.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/router.rs#L9-L73)、[server.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/server.rs#L150-L164)、[server.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-bypass/src/server.rs#L70-L93) |
| **S01-6** | High | `bootstrap.rs` 默认就绪探针与 `CertInfoBridge` 将 `is_near_expiry`（剩余天数 `<= renewal_threshold_days`，默认 30 天）直接判为服务未就绪（`/ready` 返回 `503` 且 `mazbri_rpc_ready = 0`）；当商业许可证进入 30 天 `grace` 宽限期禁止自动续期时，全集群微服务在证书实际到期前 29 天即集体摘流雪崩；同时 `PgPoolInfoBridge` 与 `CertInfoBridge` 使用 `block_in_place` 在单线程运行时必 Panic 且故障期阻塞工作线程 | [bootstrap.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-service/src/bootstrap.rs#L204-L290)、[health.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-core/src/metrics/health.rs#L110-L175) |
| **S01-7** | Medium | `mazbri-cert-service` 主停机逻辑与 `HealthMetricsUpdater` 仅监听 `ctrl_c()`（`SIGINT`），未监听容器编排标准停止信号 `SIGTERM`，导致容器重启时跳过 `audit_service.shutdown().await` 丢失内存队列中的安全审计记录 | [main.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/main.rs#L95-L99)、[health.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-core/src/metrics/health.rs#L183-L194) |

---

## 2. 第一性原理本质归因

本步全部缺陷源自 **公理 A5（时钟域混同与故障爆炸半径失控）** 与 **公理 A4（探针就绪态与预警态混同）**：
1. **时钟域不可通约性**：`std::time::Instant` 表示当前内核实例自启动以来的单调滴答计数（原点为本次开机时刻 $T_{\text{boot}}$），而数据库与 Redis 持久化的 `DateTime<Utc>` 原点为 Unix 纪元 $T_{1970}$。跨进程或跨重启把墙钟经过时长 $\Delta t_{\text{wall}}$ 投射到 `Instant::now() - \Delta t_{\text{wall}}$，隐含了不成立的前提 $\text{uptime} \ge \Delta t_{\text{wall}}$。跨进程持久化的过期与冷却判定必须在 Unix 时间戳域或“单调起始时刻 + 初始年龄偏移”域完成。
2. **故障隔离域与线程调度解耦**：`panic_policy` 的设计目标是“单请求异常转 5xx，常驻后台守护协程异常则快速退出由容器重建以防静默僵死”。仅以线程局部变量 `RECOVERABLE_DEPTH` 作为唯一判据时，遗漏了两类路径：① 未统一挂载标准中间件栈的 HTTP/gRPC 端点；② 请求处理链卸载至独立 Tokio Task 或 Blocking 线程池的单请求计算与后置任务，使局部请求异常被放大为整进程退出。
3. **传输可用性（Readiness）与运维预警（Warning）正交**：证书剩余 29 天时双向 TLS 握手完全有效，尤其在许可证 30 天宽限期（`grace`）内平台明确承诺保留存量证书通信与数据面正常服务。将续期预警阈值 `is_near_expiry` 耦合进 `/ready` 准入，违背了健康探针仅反映当前实例真实承载能力的定义。

---

## 3. 详尽可落地解决方案

### 3.1 根除跨时钟域逆算与算术溢出（`mazbri-core` / `cbpro` / `callback` / `cert-service`）

1. **`DistributedCircuitBreaker` 时钟重构（`mazbri-core`）**
   - 将 [storage_sync.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-core/src/distributed_circuit_breaker/storage_sync.rs) 中 `LocalCacheEntry` 的 `last_state_change: Instant` 替换为 `last_state_change_unix_ms: i64`（存储数据库读取的 `DateTime<Utc>::timestamp_millis()`），配合本机单调缓存刷入时刻 `fetched_at: Instant`（仅用于判断本地缓存条目自身 TTL）。
   - 冷却期判定统一改为 `Utc::now().timestamp_millis().saturating_sub(entry.last_state_change_unix_ms) >= cooldown_ms`，彻底删除 `instant_from_db_timestamp` 中的 `Instant::now().checked_sub(...).unwrap()`。
2. **`mazbri-cbpro-service` L1 缓存年龄计算重构**
   - 将 [cache.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cbpro-service/src/cache.rs) 中本地缓存条目的时间字段定义为 `stored_at: Instant` + `initial_age: Duration`（从 Redis 回填时，`stored_at = Instant::now()`，`initial_age = Duration::from_secs(redis_entry.age_secs())`）。
   - 存活时长统一按 `self.initial_age.saturating_add(self.stored_at.elapsed())` 计算，既消除减法下溢，又精准保留 L2 真实快照年龄。
3. **`mazbri-callback` 退避计算有界化与入参门禁**
   - 在 [retry.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-callback/src/retry.rs) 定义常量：`MAX_CALLBACK_RETRIES: i32 = 10`、`MIN_BACKOFF_MULTIPLIER: f64 = 1.0`、`MAX_BACKOFF_MULTIPLIER: f64 = 10.0`、`MAX_RETRY_DELAY_SECS: i64 = 86_400`（24 小时）。
   - `calculate_next_retry(attempt, initial_delay, multiplier)` 内部将 `initial_delay` 钳制在 `1..=3600`、`multiplier` 钳制在 `1.0..=10.0`（过滤 `NaN`/`Inf`/负数）、`attempt` 钳制在 `0..=16`，计算出的秒数执行 `.clamp(1, MAX_RETRY_DELAY_SECS)`，并通过 `chrono::TimeDelta::try_seconds(delay).unwrap_or_else(|| TimeDelta::seconds(MAX_RETRY_DELAY_SECS))` 相加。
   - 在 [callback.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/callback.rs) 创建订阅接口同步校验 `retry_policy` 边界，越界直接返回 `400 Bad Request`。
4. **`mazbri-cert-service` 分页与有效期算术加固**
   - [list.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/handlers/certificate/list.rs)：强制校验 `1 <= limit <= 200` 且 `offset >= 0`（`limit <= 0` 或 `offset < 0` 直接返回 `400 Bad Request`），页码计算使用 `checked_div`。
   - [issue.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/handlers/certificate/issue.rs) 与 [renew.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/handlers/certificate/renew.rs)：强制校验 `1 <= validity_days <= 3650` 且不超过 CA 剩余有效期上限，使用 `chrono::TimeDelta::try_days(validity_days)` 替代 `Duration::days`。

### 3.2 补齐全局请求级 Panic 隔离原语与服务全覆盖（`mazbri-core` / `mazbri-service` / 各微服务）

1. **在 `mazbri_core::panic_policy` 新增受控请求子任务原语**
   - 新增 `spawn_request_subtask<F>(name: &'static str, trace: TraceContext, fut: F) -> JoinHandle<Option<F::Output>>`：强制要求传入请求级 `TraceContext`，内部使用 `AssertUnwindSafe(recoverable(fut)).catch_unwind()` 捕获单次请求派生子任务（`post_response` 异步落账、`spawn_cache_populate` 写缓存、单次告警通知外发）的 Panic，记录带 `request_id` 的结构化错误日志并递增 `RECOVERED_PANICS_TOTAL` 后返回 `None`，不触发 `std::process::abort()`。
   - 新增 `spawn_blocking_recoverable<F, R>(f: F) -> JoinHandle<Result<R, PanicError>>`：在阻塞闭包入口进入 `DepthGuard::enter()`，外层 `std::panic::catch_unwind(AssertUnwindSafe(f))` 接住异常并转换为 `Err(PanicError)` 返回异步调用方映射为 HTTP `500`，隔离 Argon2、RSA/SM2、ZIP 解析等单请求阻塞任务异常。
   - **守护协程严格边界**：常驻后台循环（`route_reload`、`config_watch`、`CallbackWorker` 外层轮询、计费 `ChargeWorker` / `UsageOutboxWorker` 外层循环）严禁调用 `spawn_request_subtask`，维持致命退出由容器重启自愈。
2. **统一 HTTP 与 gRPC 服务入口隔离层**
   - 在 `mazbri-service` 提供标准组合层 `http_resilience_layers()`：按外到内固定挂载 `CatchPanicLayer::custom(...)`（返回携带 `request_id` 的标准 JSON `500`）紧邻 `recoverable_request_middleware`，并以单测锁死两层相邻顺序。
   - 在 [router.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/router.rs)、[server.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/server.rs)、[mgmt_probe.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-bypass/src/mgmt_probe.rs) 统一挂载 `http_resilience_layers()`。
   - 在 [server.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-bypass/src/server.rs) 的 `BypassGrpcServer::call` 中，将 `ExecuteMethod` 与 `HealthMethod` 包裹在 `AssertUnwindSafe(recoverable(...)).catch_unwind()` 内，捕获 Panic 后返回 `tonic::Status::internal("internal bypass error")`，保护同一 HTTP/2 连接上其余并发租户流不断连。

### 3.3 解耦证书临期预警与 `/ready` 就绪判定，移除 `block_in_place` 并补齐 `SIGTERM`

1. **校准证书就绪探针语义（`mazbri-service` & `mazbri-core`）**
   - 修改 [bootstrap.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-service/src/bootstrap.rs#L224-L290) 与 [health.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-core/src/metrics/health.rs)：`/ready` 探针与 `mazbri_rpc_ready` 指标**仅在 `info.is_expired == true` 时判为未就绪（返回 `503`）**；当 `info.is_near_expiry == true && !info.is_expired` 时保持 `/ready` 返回 `200 OK` 与 `mazbri_rpc_ready = 1`，仅通过 `mazbri_cert_expiry_days` 指标与限频 `WARN` 日志触发控制面证书临期徽标与告警规则，保障商业许可证 30 天宽限期内集群零中断运行。
2. **无锁快照与原生异步探针替换 `block_in_place`**
   - 在 `ManagedCertHandle` 内同步维护无锁原子过期时间戳 `expires_at_unix: AtomicI64`（初始化与续期时同步更新），`CertExpiryInfoProvider` 直接无锁读取计算剩余秒数，彻底移除异步 `RwLock` 阻塞桥接。
   - 将 `DbPoolInfo::is_healthy` 改为原生 `async fn`（包裹 `tokio::time::timeout(Duration::from_secs(2), ...)`），`HealthMetricsUpdater` 直接 `.await` 调用，删除 `PgPoolInfoBridge` 与 `CertInfoBridge` 中的全部 `tokio::task::block_in_place`。
3. **统一 `SIGTERM` 优雅停机监听**
   - 将 [main.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/main.rs#L95-L99) 的 `shutdown_signal` 替换为 `mazbri_service::wait_for_termination_signal()`（同时监听 `SIGINT` 与 Unix `SIGTERM`），确保 `docker restart` / `docker stop` 时完整执行 `audit_service.shutdown().await`；`HealthMetricsUpdater` 改为接收 `CancellationToken` 随主服务协同退出，不再独立抢占进程信号处理器。

---

## 4. 二阶与三阶影响深度推演

| 影响阶次 | 潜在连锁反应场景 | 架构消解与防护设计 |
| :--- | :--- | :--- |
| **二阶影响 1：锁毒化（Mutex Poisoning）导致服务假活持续返 500** | 启用请求级 `catch_unwind` 后，若单请求在持有 `std::sync::Mutex` 临界区期间发生 Panic，后续所有正常请求调用 `.lock().unwrap()` 会产生连环 Poison Panic，因处于可恢复域而不触发进程重启，形成 100% `500` 活锁。 | 全仓共享内存状态优先采用无毒化语义的 `parking_lot::Mutex` / `RwLock` 或 `DashMap`；对标准库 `Mutex` 统一使用 `.unwrap_or_else(PoisonError::into_inner)`，且临界区内仅执行纯内存无失败赋值。 |
| **二阶影响 2：异步落账子任务 Panic 被隔离后单笔计费事件漏入队** | `post_response` 子任务经 `spawn_request_subtask` 捕获 Panic 后保住了网关在途连接，但若在写入 Outbox 前崩溃会导致单笔实时计费漏记。 | 在进入 `spawn_request_subtask` 前预先构建确定性 `UsageOutboxEvent`；子任务返回 `None`（发生 Panic）时立即记录高优 `ERROR` 对账日志并递增 `mazbri_post_response_panics_total`，由 Step 06 的 L3 ClickHouse ↔ PG 对账作业自动根据网关访问日志回补入账。 |
| **二阶影响 3：将 `is_near_expiry` 移出 `/ready` 后的临期感知链路** | 过去依赖 `/ready` 变 `503` 暴露证书临期的粗暴信号被移除后，若缺少监控闭环可能导致运维忽略续期异常。 | `mazbri-cert-service` 全局库存指标 `mazbri_cert_min_expiry_days` 与各节点 `mazbri_cert_expiry_days` 保持实时上报，配合前端运维总览 `CertExpiryBadge`（`<= 30` 天高亮告警）与 Prometheus 告警规则形成非破坏性预警闭环。 |
| **三阶影响：防止后续迭代将常驻后台任务误用可恢复派生原语** | 若开发人员把 `route_reload` 或扣费 Worker 误传入请求级可恢复派生函数，主循环崩溃后将陷入静默停摆。 | `spawn_request_subtask` 在函数签名上强制绑定请求级 `TraceContext` 参数，无请求上下文的后台常驻任务在编译期即无法直接误调。 |

---

## 5. 验收门禁与实测清单

1. **自动化单测必测项**
   - 构造 `db_last_change = Utc::now() - Duration::days(30)`（大于宿主机开机时长）调用 `DistributedCircuitBreaker` 与 `cbpro` L1 回填，断言零 Panic 且正确判定冷却期与存活年龄。
   - 构造 `attempt = 100, multiplier = 100.0` 调用 `calculate_next_retry`，断言返回时间精确封顶在 `now + 86400s` 且无溢出。
   - 构造剩余有效期 `15` 天（`< renewal_threshold_days = 30` 但未过期）与 `-1` 天（已过期）两种证书状态：断言前者 `/ready` 返回 `200 OK` 且 `mazbri_rpc_ready == 1`，后者 `/ready` 返回 `503` 且 `mazbri_rpc_ready == 0`；在 `#[tokio::test(flavor = "current_thread")]` 单线程运行时调用探针断言零 `block_in_place` Panic。
   - 向 `mazbri-cert-service`、`mazbri-analytics`、`mazbri-bypass` 注入故障桩主动触发请求内 `panic!("boom")`，断言故障请求返回 `500` / `Status::internal`，同进程并发正常请求保持 `200 OK`，且 `mazbri_recovered_panics_total` 精确 `+1`。
   - 调用 `GET /certificates?offset=10&limit=0` 与 `validity_days = -5 / i64::MAX`，断言返回 `400 Bad Request`。
2. **容器热替换顺序**
   - 依次替换并重启：`mazbri-cert` ➔ `mazbri-analytics` ➔ `mazbri-cbpro` ➔ `mazbri-bypass` ➔ `mazbri-gateway` ➔ `mazbri-mgmt`，逐一核验 `:9000/health` 与 `:9000/ready` 返回 `200`。
