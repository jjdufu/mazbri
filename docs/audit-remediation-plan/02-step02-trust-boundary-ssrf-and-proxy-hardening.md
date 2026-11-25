# Step 02：出站 SSRF 统一底座、控制面代理强类型隔离与防混淆代理人治理

## 1. 检测出的问题清单与源码定位

| 编号 | 级别 | 问题描述 | 精确源码位置 |
| :--- | :--- | :--- | :--- |
| **S02-1** | Critical | `mazbri-mgmt` 证书代理使用 `Path(id): Path<String>` 接收路径参数并直接拼接转发，未校验 UUID 或拦截编码后的 `/`、`..`、`?`、`#`；`actor_header` 对归一化后的目标路径自动签发合法 `x-mazbri-actor` 并携带管理员级主令牌，未过 MFA 即可穿越访问 `/authorizations/root/current/materials`、`/certificates/{id}/full` 与 `PUT /configs/{service}` | [certificates.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/proxy/cert/certificates.rs#L32-L79)、[certificates.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/proxy/cert/certificates.rs#L160-L187)、[client.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/proxy/cert/client.rs#L42-L46) |
| **S02-2** | Critical | 上游连接器草稿反开 `draft_from_supplier` 硬编码 `is_super_admin = true` 绕过超管 TLS 私钥与跳过校验证书守卫；更新草稿、试跑与 `PUT /suppliers/{id}` 在凭据为掩码 `••••••••` 时自动回填原密文，未校验目标 `upstream_url` / `base_url` / `token_url` 的 Origin 是否变更，修改目标地址试跑即可外泄生产凭据 | [import.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/upstream_connector/import.rs#L944-L1032)、[draft.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/upstream_connector/draft.rs#L182-L282)、[supplier.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/supplier.rs#L300-L386) |
| **S02-3** | Critical | `ssrf.rs` 处理 IPv6 时遗漏 `0.0.0.0/8` 全段、CGNAT `100.64.0.0/10`、NAT64 `64:ff9b::/96` 等保留网段，且缺少跨内外网模式强制生效的底座黑名单；`mazbri-bypass` 主 HTTP 客户端未禁 3xx 重定向且未设 `.no_proxy()`，`ExecuteOptions.follow_redirects` 为死字段 | [ssrf.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-upstream-runtime/src/ssrf.rs#L264-L320)、[main.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-bypass/src/main.rs#L37-L55) |
| **S02-4** | Critical | `job_poll` 适配器将租户请求 JSON 中的 `job_id` 未编码直接替换进 `query_path` 并经 `join_url`（允许 `http(s)://` 完整覆盖 `base_url`）发出请求，租户可劫持出站目标窃取供应商高权限凭据或在供应商域名下路径穿越 | [job_poll.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-upstream-runtime/src/adapters/job_poll.rs#L151-L176)、[http_call.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-upstream-runtime/src/http_call.rs#L247-L260) |
| **S02-5** | Critical | `mazbri-cert-service` 代码默认 `ScopeMode::Audit`、`ActorMode::Lenient`，且部署模板 `cert-bootstrap.toml` 显式写死 `actor_mode = "lenient"` 并注释 `scope_mode`；业务口 `:8888/metrics` 无鉴权暴露遗留 `certdaemon_*` 查库指标；`subject_exists` 未过滤已过期证书阻断重签且 `update_certificate` 续期漏刷 `fingerprint` 列 | [scope.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/token/scope.rs#L18-L25)、[config.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/audit/config.rs#L20-L30)、[cert-bootstrap.toml](file:///workspace/rust-proj/mazbri-max/docker/mconfigs.tmpl/cert-bootstrap.toml#L60-L68)、[server.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/server.rs#L92)、[repository.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/db/repository.rs#L279-L363) |
| **S02-6** | High | 5 条后台出站链路缺失连接期防 DNS 重绑定 SSRF 保护：异步回调 `CallbackWorker`、告警 `Notifier` Webhook、对账告警 Webhook（无超时且跟随重定向）、节点注册 `health_check_url` 探活、工作台 `microservice` 模式 `TcpStream::connect` 端口探测，以及供应商/通道版本回滚与合并后 `token_url` 保存期 URL 预检 | [worker.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-callback/src/worker.rs#L51-L62)、[notifier.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-analytics/src/alerting/notifier.rs#L131-L170)、[main.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/main.rs#L334-L357)、[run.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/upstream_connector/run.rs#L1159-L1189)、[node.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/handlers/node.rs#L36-L66) |
| **S02-7** | High | `mazbri-core::resolve_client_ip` 无条件采信客户端请求头中的 `X-Forwarded-For` / `X-Real-IP` 且不校验合法 IP 格式：伪造随机头可绕过 IP 限流并撑爆无界 `buckets` 内存，传入非 IP 字符串导致限流服务 `::inet` 整批事务回滚，且单语句 `15 * deltas.len()` 在高吞吐下突破 PG 65,535 参数上限 | [client_ip.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-core/src/client_ip.rs#L4-L23)、[engine.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-gateway/src/ratelimit/engine.rs#L485-L512)、[tarpc_service.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-ratelimit-service/src/tarpc_service.rs#L180-L222) |

---

## 2. 第一性原理本质归因

本步全部漏洞均是 **公理 A3（信任边界强类型解析与防混淆代理人公理）** 的直接映射：
1. **语义层与字节层解析错位**：HTTP 路由参数、URL 模板占位符与转发头在入口仅是不可信字节序列。上层以裸字符串拼接转发，交由下游 `url::Url`、Axum 路由树或 PostgreSQL `::inet` 二次解析时，解析器之间的语法差异（`%2e%2e%2f` 折叠路径、`?` 注入查询参数）直接构成越权通道。
2. **混淆代理人（Confused Deputy）**：`mazbri-mgmt` 与 `mazbri-bypass` 同时承担“执行操作员指令”与“持有平台最高机密（主令牌、凭据解密权）”双重职责。密文凭据的自动回填使用权必须与创建该凭据时的**授信目标域（Origin = Scheme + Host + Port）**强绑定，目标域变更即意味着授信上下文失效。
3. **混合网络下的最小可达性边界**：在内网联调（`upupt` 位于 `192.168.50.50`）或企业专网部署中开启 `allow_private_network = true` 是合法业务需求，但平台自身的数据底座主机（`db51` `192.168.50.51`）与微服务容器内部网络在任何模式下均不属于合法上游目标，必须在二元内外网开关之上建立不可逾越的底座黑名单。

---

## 3. 详尽可落地解决方案

### 3.1 收敛全仓唯一安全出站底座（`SsrfChecker` + `deny_cidrs`）

1. **补齐 IP 封禁完备集与跨模式黑名单（`mazbri-core` & `mazbri-upstream-runtime`）**
   - 将零重依赖的纯 IP/URL/DNS 校验核心下沉至 `mazbri-core::net::ssrf`（仅依赖标准库、`url`、`ipnetwork`、`tokio::net::lookup_host`），并通过可选 Cargo feature `http-ssrf` 提供 `reqwest::dns::Resolve` 适配器，供 `bypass`、`mgmt`、`analytics`、`callback` 共用，避免纯 tarpc 服务引入 `reqwest`。
   - 在 `is_blocked_ip(ip: IpAddr)` 入口统一解包 `v6.to_ipv4_mapped()`，补齐拦截 `0.0.0.0/8`、CGNAT `100.64.0.0/10`、基准测试段 `198.18.0.0/15`、组播与保留段 `224.0.0.0/4` / `240.0.0.0/4`、NAT64 `64:ff9b::/96`、6to4 `2002::/16` 与文档段 `2001:db8::/32`。
   - 在 `SsrfConfig` 新增 `#[serde(default)] pub deny_cidrs: Vec<ipnetwork::IpNetwork>` 与 `#[serde(default)] pub deny_hosts: Vec<String>`：默认内置拦截 `.pgsql.local`、`.clickhouse.local`、`.greptimedb.local`、`.redpanda.local`、`.mazbri.local` 及 `mazbri-*` 容器主机名；命中 `is_always_blocked_ip(ip)` 或 `deny_cidrs` / `deny_hosts` 时，**无论 `allow_private_network` 为何值均无条件拒绝**。
   - 同步更新 `mazbri-max/docker/up.sh`、[mgmt-bootstrap.toml](file:///workspace/rust-proj/mazbri-max/docker/mconfigs.tmpl/mgmt-bootstrap.toml) 与 `mazbri-max/dev-conf/mazbri-bypass.toml`：自动将 `${EXT_DEPS_IP}/32`（联调栈 `192.168.50.51/32`）与容器控制网段注入 `deny_cidrs`，放行 `upupt`（`192.168.50.50`）的同时硬封禁 `db51`。
2. **统一 `build_safe_http_client` 工厂覆盖全部出站链路**
   - 统一强制挂载 `SsrfSafeResolver`、`.redirect(reqwest::redirect::Policy::none())`、`.no_proxy()` 及连接与总超时上限；删除死字段 `ExecuteOptions.follow_redirects`。
   - 替换全部 5 处裸客户端：[main.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-bypass/src/main.rs) 主客户端、`mazbri-callback` `CallbackWorker`、`mazbri-analytics` 告警 `Notifier`、`mazbri-mgmt` 对账 Webhook（设 10s 超时）以及连接器本地 `auth_only` 试跑（强制 `.min(max_timeout_ms)`）。
   - 工作台 `probe_connectivity_stage` 在 `TcpStream::connect` 前先调用 `ssrf_checker.resolve_checked_socket_addrs(host, port).await?`，仅对通过校验的 `SocketAddr` 建连；供应商/通道直接保存、版本回滚及合并后的 `built.spec.auth.token_url` 全部执行 `ssrf_checker.validate_url` 预检。
   - 节点注册 [node.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/handlers/node.rs)：校验 `ip` 为合法 `IpAddr`，若提供 `health_check_url` 则强制 `scheme` 为 `http`/`https`、禁止用户信息与查询参数、`url.host()` 必须与注册 `ip` 一致且路径仅限 `/health` 或 `/ready`；网关 `ServiceHealthCache` 探活客户端禁用重定向并设 2s 硬超时。
3. **修复 `job_poll` 路径替换与 `join_url` 越界**
   - [job_poll.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-upstream-runtime/src/adapters/job_poll.rs)：校验 `job_id` 非空、长度 `<= 128` 且仅含 `[A-Za-z0-9._:-]`（显式拒绝 `.` 与 `..`），替换时做单段百分号编码。
   - [http_call.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-upstream-runtime/src/http_call.rs)：移除 `join_url` 中允许 `path` 以 `http://`/`https://` 覆盖 `base_url` 的逻辑，出站 `scheme://authority` 严格锁定为渠道或供应商配置的 `base_url`。

### 3.2 控制面代理强类型化、Origin 绑定凭据回填与 `cert-service` 强制收口

1. **证书代理路径强类型化（`mazbri-mgmt/src/proxy/cert/`）**
   - 将 [certificates.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/proxy/cert/certificates.rs) 与 `common.rs` 宏中所有按证书 ID 操作的 Handler 参数改为强类型 `Path(id): Path<uuid::Uuid>`；按 `service` 或 `subject` 查询的接口通过 `mazbri_core::validate_config_entry_name` 校验并对单段做 `urlencoding::encode`。
   - 在 [client.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/proxy/cert/client.rs) 增加请求构造断言：`path` 严禁包含 `..`、`\`、`?`、`#` 或连续 `//`，且禁止超 1MiB 请求体 `unwrap_or_default` 变空体（超限直接返回 `413 Payload Too Large`）。
2. **凭据掩码回填强制绑定原始目标域（Origin Pinning）与前端联动**
   - 提取归一化函数 `canonical_origin(url: &str) -> Result<(String, String, u16), _>`（解析小写 `scheme`、小写 `host` 与显式或默认端口）。
   - 在 `supplier::update_supplier`、`upstream_connector::draft::update_draft` 与试跑 `materialize_snapshot_row` 中：对比新配置与原记录的 `(upstream_origin, token_origin, endpoint_origins, auth_profile)`。一旦任一目标 Origin 变更或认证协议切换，拒绝回填掩码 `••••••••` 对应的历史密文，返回 `400 Bad Request`（`"目标地址或认证方式变更，须重新录入凭据"`）。
   - 同步更新前端 [index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/suppliers/[id]/index.vue) 与 [workbench.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/workbench.vue)：编辑态检测到接入地址或 Token 地址的 Origin 变更时，自动清空处于掩码态的凭据输入框并标记必填校验，避免提交后报错。
   - 修复 [import.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/upstream_connector/import.rs) 的 `draft_from_supplier`：传入调用者真实的 `is_admin(&claims)` 替代硬编码 `true`；源供应商含自定义客户端私钥或 `insecure_skip_verify = true` 且调用者为 `R_FINANCE` 时直接拒绝反开。
3. **`mazbri-cert-service` 强制鉴权、证书仓储修复与删除 `:8888/metrics`**
   - 将 [scope.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/token/scope.rs) 的 `ScopeMode::default()` 设为 `ScopeMode::Enforce`、[config.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/audit/config.rs) 的 `ActorMode::default()` 设为 `ActorMode::Strict`，并同步将 [cert-bootstrap.toml](file:///workspace/rust-proj/mazbri-max/docker/mconfigs.tmpl/cert-bootstrap.toml#L60-L68) 显式改为 `scope_mode = "enforce"` 与 `actor_mode = "strict"`。
   - 在 `token/scope.rs` 根保护材料读取白名单中加入 `Caller::Service(ServiceName::Bypass)`（与 `Admin`、`Gateway` 并列）；在 `audit/route.rs` 将 `POST /issue` 的 `mgmt_only` 设为 `true`。
   - 修复 [repository.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/db/repository.rs)：① `subject_exists` 条件改为 `WHERE subject = $1 AND revoked = false AND expires_at > NOW()`，对齐 `get_by_subject` 口径，消除“证书自然过期后既拉不到有效证书又因主体已存在无法重签”的死锁；② `update_certificate` 续期更新 `cert_pem` 时同步重算并写入新证书的 SHA-256 `fingerprint` 列，保证 `/verify` 与指纹检索一致。
   - 从 [server.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/server.rs#L92) 删除未鉴权的业务口 `.route("/metrics", ...)` 并移除 [health.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/handlers/health.rs#L43-L69) 中遗留的 `metrics` 处理器（Vector 固定采集 `:9000/metrics` 标准 `mazbri-tls` 指标）；同步删除未参与编译的死文件 [auto_renewal.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/auto_renewal.rs)。

### 3.3 下沉可信代理 IP 提取器与加固限流批量落库

1. **统一 `TrustedProxyIpExtractor`（`mazbri-core`）**
   - 将 [ip.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/audit/ip.rs) 中基于 `trusted_proxies: Vec<IpNetwork>` 自右向左剥离可信代理跳、默认空列表仅取 TCP `SocketAddr` 的实现提升至 `mazbri-core::client_ip`，替换旧的无条件信任 `X-Forwarded-For` 函数；在 `GatewayConfig` 增加 `[server] trusted_proxies = []`，输出强类型 `IpAddr`。
2. **限流桶有界化与批量上报 `UNNEST` 容错**
   - 将 [engine.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-gateway/src/ratelimit/engine.rs) 的 `buckets` 从无界 `DashMap` 替换为 `moka::sync::Cache<BucketKey, Arc<TokenBucket>>`（`max_capacity = 200_000` + `time_to_idle = 600s`）。
   - 在 [tarpc_service.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-ratelimit-service/src/tarpc_service.rs) 的 `insert_usage_deltas` 中：使用 `delta.client_ip.parse::<IpAddr>().ok()` 绑定 `Option<IpAddr>`（非法值入库 `NULL` 不回滚整批事务），并改为 `UNNEST($1::uuid[], $2::inet[], ...)` 单次 15 个数组参数批量写入，消除 PostgreSQL 65,535 参数上限溢出。

---

## 4. 二阶与三阶影响深度推演

| 影响阶次 | 潜在连锁反应场景 | 架构消解与防护设计 |
| :--- | :--- | :--- |
| **二阶影响 1：`cert-service` 开启 `Enforce` + `Strict` 对微服务冷启动与运行期心跳的影响** | 8 个 mTLS 微服务在进程启动阶段并无管理员会话（不携带 `x-mazbri-actor`），需顺序调用 `GET /configs/{service}`、`GET /certificates/by-subject/{subject}`（临期调 `POST /renew/{id}`）及 `POST /nodes/register`、`/nodes/heartbeat`（网关与 `wait-deps` 另调 `GET /nodes`）。 | 逐项核对路由分类表：`GET /configs/{service}` 与 `/nodes/*` 在 `audit/route.rs` 中不属管理审计写操作，在 `scope.rs` 按派生令牌归属精确放行；`GET /certificates/by-subject/{subject}` 为 `Access::Shared`（免操作人头，按 `Caller::Service` 校验自身 `subject`）；`POST /renew/{id}` 为 `Access::MgmtOrOwner`（允许无操作人头的服务令牌调用并注入 `OwnerCheckEnforced` 校验归属）。同时阶段一 `mazbri-mgmt` 全部代理写操作均通过 `actor_header::send_to_cert` 自动签发 v2 HMAC 声明，全链路启动零阻断。 |
| **二阶影响 2：前置 L7 反向代理集群启用 `trusted_proxies = []` 后客户端 IP 汇聚** | 若生产网关前置了 Nginx 或 Ingress 但未配置 `trusted_proxies`，所有外部流量在网关侧共享负载均衡器内网 IP，触发单 IP QPS 限流误伤。 | 在网关配置模板显式提供 `trusted_proxies` 网段配置说明；当检测到请求携带 `X-Forwarded-For` 但 TCP 对端不在 `trusted_proxies` 且触发 `client_ip_qps` 拒绝时，输出限频 `WARN` 诊断日志提示配置可信代理网段。 |
| **三阶影响：供应商切换环境域名时的交互变化** | 启用 Origin Pinning 后，将供应商地址从测试域名改为正式域名时无法沿用旧掩码凭据直接保存。 | 前端详情页与工作台在检测到 Origin 变更时即时清空掩码框并触发必填校验，使操作员在同一表单内录入目标域名对应凭据后一次提交成功。 |

---

## 5. 验收门禁与实测清单

1. **安全与功能回归测试**
   - 请求 `GET /api/v1/mgmt/certificates/..%2Fauthorizations%2Froot%2Fcurrent%2Fmaterials` 与 `PUT /api/v1/mgmt/certificates/..%2Fconfigs%2Fmazbri-gateway%3Fx/auto-renewal`，断言路由层直接返回 `400 Bad Request`，不触达 `cert-service`。
   - 将 `[::ffff:127.0.0.1]`、`[::ffff:192.168.50.51]`、`0.0.0.1`、`100.64.0.1`、`[64:ff9b::7f00:1]` 输入 `SsrfChecker`（`allow_private_network = false`），断言 100% 拦截；开启 `allow_private_network = true` 且配置 `deny_cidrs = ["192.168.50.51/32"]`，断言 `192.168.50.50`（`upupt`）放行而 `192.168.50.51`（`db51`）拦截。
   - 预置一条已过期未吊销的微服务证书记录，测试重签该 `subject` 成功（不报冲突），续期后调用 `POST /verify` 校验新证书返回 `valid = true`；访问 `cert-service` `:8888/metrics` 断言返回 `404` 而 `:9000/metrics` 正常导出证书指标。
   - 反开含凭据供应商草稿后修改 `upstream_url` 保留 `••••••••` 提交保存或试跑，断言后端返回 `400` 拒绝解密，前端切换 Origin 时自动清空掩码输入框。
   - 发送 `X-Forwarded-For: not-an-ip` 流量并构造 6,000 条增量单次调用 `ingest_usage_deltas`，断言 `UNNEST` 批量入库成功零报错。
2. **部署与验证顺序**
   - 同步更新 `mconfigs.tmpl/{cert,mgmt}-bootstrap.toml`、`dev-conf/mazbri-bypass.toml` 与前端构建产物后，按序更新验证：`mazbri-cert` ➔ `mazbri-ratelimit` ➔ `mazbri-analytics` ➔ `mazbri-bypass` ➔ `mazbri-gateway` ➔ `mazbri-mgmt`，并执行两阶段启动验证全量节点注册就绪。
