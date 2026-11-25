# Step 04：密码学域分离、落库密钥收敛与全集群 mTLS 动态热重载

## 1. 检测出的问题清单与源码定位

| 编号 | 级别 | 问题描述 | 精确源码位置 |
| :--- | :--- | :--- | :--- |
| **S04-1** | High | **全集群 mTLS 证书“假续期”**：`CertManager::spawn_renewal_task` 续期后仅更新内存 `ManagedCertificate`，而 `TlsAcceptor`、`TlsConnector`、`UpstreamHttp2ClientPool` 与 `BypassGrpcServer` 在启动时按值固化初始证书；初始证书到期后全集群 mTLS 断连瘫痪 | [bootstrap.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-service/src/bootstrap.rs#L362-L377)、[transport.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-tls/src/transport.rs#L28-L84)、[mod.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-gateway/src/http2_upstream_client/mod.rs#L101-L113)、[server.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-bypass/src/server.rs#L95-L120) |
| **S04-2** | High | **mTLS 认证边界松散与握手阻塞**：`build_server_config` 在 `verify_peer=true` 但 `ca_cert_pem` 解析为空时静默退化为无客户端认证；内部 RPC 服务（含暴露 `add_funds` 的计费服）不校验对端证书 SAN；证书吊销无数据面感知；`TlsTarpcListener` 在单协程 accept 循环内无超时等待握手，单慢连接即可阻塞全服新连接 | [transport.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-tls/src/transport.rs#L222-L269)、[tarpc_transport.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-tls/src/tarpc_transport.rs#L38-L50)、[billing.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-rpc/src/billing.rs#L238-L246) |
| **S04-3** | High | **上游密码流水线 Nonce 重用与请求/响应策略耦合**：`NamedFields` 多字段循环共享单次 IV；`IvSource::KeyDerive` 截断静态密钥充当 AEAD/CTR 固定 Nonce 未被拦截；`UpstreamCryptoProfile` 未显式区分单向请求签名/加密与双向响应验签/解密，导致 `crypto_wire` 用“验签解密失败静默回退明文 JSON”掩盖契约缺失；`SignThenEncrypt` 顺序与错误区分、多字段 `Raw` 仅签首字段、`Digest` 非常量时间比较 | [crypto_pipeline.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-upstream-runtime/src/crypto_pipeline.rs#L400-L497)、[upstream.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-crypto/src/profile/upstream.rs#L140-L240)、[crypto_wire.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-upstream-runtime/src/adapters/crypto_wire.rs#L235-L343) |
| **S04-4** | High | **落库密钥孤岛、硬编码弱种子与配置中心回滚导出失配**：`supplier_secrets` 未配环境变量时回退公开常量 `DEV_SEED` 且空 AAD；配置中心与管理端 MFA 共用 `"mazbri-mfa-v1"` 派生键且空 AAD，`rollback` 未恢复历史 `is_sensitive` 且将密文直接做 TOML 语法校验必败，导出 ZIP 原样写出不可跨集群导入的 `enc:` 密文 | [supplier_secrets.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-core/src/crypto/supplier_secrets.rs#L56-L160)、[encryption.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/config_center/encryption.rs#L106-L150)、[crud.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/config_center/repository/crud.rs#L120-L175) |
| **S04-5** | High | **商业许可证与 CA 生命周期缺陷**：`Rekey` 漏重加密 `certificates.key_pem` 且原地覆写唯一根保护材料导致存量证书与租户密钥报废，且根材料接口仅返回单版本材料无法支撑 `Rekey` 后重启解密历史版本；`decide_license` 忽略 `clock_rollback` 并将 `now < not_before` 误判为 `"expired"`；续期有效期忽略请求 `validity_days` 且重启重签 CA 改变序列号 | [license_service.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/license_service.rs#L581-L634)、[license.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-cert/src/license.rs#L592-L636)、[generator.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/ca/generator.rs#L34-L39)、[root_package.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/handlers/root_package.rs#L15-L45) |
| **S04-6** | Medium | **租户传输信封无方向/算法绑定与边界缺陷**：`TransportEnvelope` 请求与响应共用同一组密钥且 MAC/AAD 未绑定方向与算法标识；AEAD 开启 `require_signature=true` 且无 `mac_alg` 时入站放行但出站响应报错 `500`；未验签前抢耗 Redis Nonce；`sm4-ecb-pkcs7` 代码要求 16B IV 与协议定义 0B 矛盾；多处私钥与对称密钥未做 `Zeroize` 擦除 | [transport.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-core/src/crypto/transport.rs#L124-L160)、[transport.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-core/src/crypto/transport.rs#L518-L586)、[crypto.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-gateway/src/crypto.rs#L703-L775) |

---

## 2. 第一性原理本质归因

本步问题对应 **公理 A4（动态状态流引用）** 与 **公理 A6（密码学域分离与组合安全）**：
1. **句柄语义取代值语义**：TLS 证书、多版本根保护密钥环与吊销列表均随时间演进。将启动瞬间的 `ManagedCertificate` 或单版本根密钥按值固化进传输层切断了状态流；必须注入无锁原子句柄（`ArcSwap<CertifiedKey>` 与多版本 `RootMaterialKeyring`），使每次握手与加解密零锁读取当前权威状态。
2. **单一根熵源 + HKDF 正交域分离 + 显式方向契约**：平台所有落库与传输密码学材料必须从商业根保护材料（及管理库独立主种子）经互不相交的 `HKDF-SHA256(ikm, salt, info)` 派生，并将 `(direction, owner_kind, owner_id, field_or_alg, key_version)` 绑定至 AEAD AAD 与规范化待签串。同时，上游通道必须显式声明请求侧与响应侧各自的签名/加解密策略，消除靠“静默回退明文”猜测协议方向的隐患。

---

## 3. 详尽可落地解决方案

### 3.1 全集群零停机 mTLS 动态热重载、对端 SAN 授权与有界并发握手

1. **动态证书解析器 `DynamicCertResolver`（`mazbri-tls`）**
   - 在 [cert_manager.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-tls/src/cert_manager.rs) 内维护 `Arc<ArcSwap<CertifiedKey>>`（经 `rustls::crypto::aws_lc_rs::sign::any_supported_type` 构建）。`initialize` 与后台 `renew_certificate` 成功时原子 `swap` 最新 `CertifiedKey` 并同步更新原子过期时间戳。
   - 实现 `rustls::server::ResolvesServerCert` 与 `rustls::client::ResolvesClientCert` 直接委托 `ArcSwap::load_full()`；修复 [transport.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-tls/src/transport.rs)：`config.verify_peer == true` 且 `ca_certs.is_empty()` 时直接返回 `Err(TlsError::InvalidCertificate)` 拒绝启动。
   - **`mazbri-gateway` 与 `mazbri-bypass` 接入动态证书**：
     - `UpstreamHttp2ClientPool` 与 `TarpcClientPool` 共享绑定了 `DynamicCertResolver` 的动态 `ClientConfig`，连接轮换重建时自动呈递最新续期证书；网关 [main.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-gateway/src/main.rs) 将 `cert_handle` 接入 Admin 就绪探针（仅 `is_expired` 置未就绪）。
     - [server.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-bypass/src/server.rs) 使用绑定了 `DynamicCertResolver` 且显式设置 **`server_config.alpn_protocols = vec![b"h2".to_vec()]`** 的 `TlsAcceptor::new_h2`，经 `tonic::transport::Server::builder().serve_with_incoming(...)` 承载 mTLS HTTP/2 流。
2. **服务间 mTLS 对端身份授权（Peer SAN ACL）与吊销推送**
   - 在 `TlsTarpcListener` 与 `BypassGrpcServer` 握手完成后提取对端叶证书 DNS SAN：
     - `auth`、`ratelimit`、`cache`、`bypass`、`cbpro-ext` 仅允许 `mazbri-gateway` 连接；`cbpro` 仅允许 `mazbri-gateway` 与 `mazbri-cbpro-ext` 连接；
     - 从 [billing.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-rpc/src/billing.rs) 的公开 tarpc trait 中删除网关无需调用的 `add_funds` 接口（账户充值与退款统一收敛于 `mazbri-mgmt` 计费库存储过程链路），`mazbri-billing-service` 仅向 `mazbri-gateway` 开放用量入账、余额预检与查询；
     - [san.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/san.rs) 停止将 `extra_sans` 无差别注入内部 tarpc 服务证书，仅对边界网关按白名单追加。
   - **吊销实时感知**：在 `mazbri-cert` 心跳响应中附带已吊销证书序列号集合摘要，变更时 `CertManager` 拉取刷新本地 `ArcSwap<HashSet<Vec<u8>>>`，由包装 `WebPkiClientVerifier` 的自定义验证器实时拦截已吊销证书握手。
3. **修复 `TlsTarpcListener` 单协程阻塞**
   - 在 [tarpc_transport.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-tls/src/tarpc_transport.rs) 中，`listener.accept().await` 获取 TCP 连接后先从有界握手信号量（`Semaphore::new(max_connections)`）获取许可，再通过 `tokio::spawn` 独立任务执行 `tokio::time::timeout(Duration::from_secs(5), acceptor.accept(tcp_stream))`，握手成功的连接经有界 `mpsc::channel` 送入服务端分发循环。

### 3.2 收敛单一落库加密体系与两阶段冷启动协同（零 `DEV_SEED`、零历史双读）

1. **供应商与渠道凭据收敛为商业根材料密钥环派生的 `enc:v3:<key_ver>:<alg>:<b64>`**
   - 在 `mazbri-cert-service` 新增多版本密钥环接口 `GET /authorizations/root/materials/keyring`（`token/scope.rs` 授权 `Caller::Admin`、`ServiceName::Gateway`、`ServiceName::Bypass`）：在 `decryption_allowed` 成立时返回当前活跃 `current_key_version`（仅 `issuance_allowed` 时可用于新加密）及全部有效历史版本的 `Vec<RootProtectionMaterialBundle>`。
   - 重写 [supplier_secrets.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-core/src/crypto/supplier_secrets.rs)：**彻底删除 `DEV_SEED`、`MAZBRI_SUPPLIER_SECRETS_KEY` 与旧 `enc:v2:` 解析逻辑**，仅保留唯一密文格式 `enc:v3:<key_ver>:<alg>:<b64>`。密封与解封按 `key_version` 从根材料密钥环取出对应 `master_key`，经 `HKDF-SHA256(master_key, info = "mazbri-supplier-secrets-v3")` 派生 32 字节密钥，并强制绑定规范化 AAD：`format!("supplier_secret|{owner_kind}|{owner_id}|{field_name}|{alg}")`。
   - 所有接收明文凭据的外部 HTTP DTO 若检测到客户端提交值以 `"enc:"` 开头（非服务端内部掩码回填阶段），直接返回 `400 Bad Request` 拒绝。
   - **两阶段冷启动严格约束**：
     - 阶段一（`base`）启动的 `mazbri-mgmt` 在进程启动入口（`mgmt/src/main.rs`）**严禁预拉或强校验根保护材料**，确保空库未导入许可证时 WebUI 与管理 API 正常就绪以接收首次许可证上传；仅在管理员导入许可证后读写供应商、渠道或租户加密凭据时，由 [root_package.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/root_package.rs) 按需拉取并缓存多版本密钥环（许可证导入或 `Rekey` 后立即调用 `invalidate_root_material_cache`）。
     - 阶段二（`app`）启动的 `mazbri-gateway` 与 `mazbri-bypass` 在许可证已激活的前提下于启动期预热多版本密钥环，并在运行期遇到未知新 `key_version` 时单飞刷新密钥环。
2. **配置中心与管理端 MFA 密钥分域及回滚/导入导出闭环**
   - 重写 [encryption.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/config_center/encryption.rs)：删除与 MFA 共用的 `"mazbri-mfa-v1"` 派生上下文及旧 `enc:<hex>` 格式，单一采用 `enc:cfg:v2:<b64>` 格式，HKDF `info` 独立为 `"mazbri-config-center-v2"`，并以 `service_name.as_bytes()` 作为 AES-256-GCM AAD。
   - 更新 `mazbri-max/init-dbs/src/sql/schemas/cert/init.sql`：在 `config_history` 表与变更触发器中完整记录历史快照当时的 `is_sensitive` 列；修复 [crud.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/config_center/repository/crud.rs) 与 [rollback.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/handlers/config/rollback.rs)——回滚时若历史版本 `is_sensitive == true`，**先将历史密文解密还原为明文 TOML，对明文执行 `validate_toml` 与 `validate_service_config` 校验，通过后再按历史 `is_sensitive` 标志重新密封回写 `config_entries`**。
   - 修复配置中心跨环境导出导入（[import_export.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/config_center/import_export.rs)）：`export_configs`（已受 `R_ADMIN` + `Category::Platform` MFA 提权门禁保护）统一拉取解密后的明文 TOML 并在 ZIP 清单元数据中保留各服务的 `is_sensitive` 布尔属性；`import_configs` 拒绝任何以 `"enc:"` 开头的裸密文内容，校验明文 TOML 后按 `is_sensitive` 属性由目标集群重新加密落库。
3. **核心密钥全面接入 `Zeroize`**
   - 将 `ManagedCertificate::key_pem`、`KeyRef::Pem`、`CaManager::encryption_key`、`TokenTable` 主密钥、`ConfigEncryption` 缓存键及网关 `key_cache` 中的租户对称密钥统一封装为 `Zeroizing<String>` / `Zeroizing<Vec<u8>>` / `Zeroizing<[u8; 32]>`。

### 3.3 上游请求/响应加密方向解耦、`TransportEnvelope` v1 原位加固与许可证状态机修复

1. **上游密码流水线请求/响应显式分离与 Nonce 唯一性（`mazbri-crypto` & `mazbri-upstream-runtime`）**
   - 在 [upstream.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-crypto/src/profile/upstream.rs) 的 `UpstreamCryptoProfile` 显式拆分方向契约：新增布尔配置 `verify_response: bool`（默认 `false`），并将响应解密严格限定于显式配置了 `response_crypt`（或显式开启 `decrypt_response: bool`）的通道；同步更新前端 [cryptoProfileHelpers.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/cryptoProfileHelpers.ts)、[CryptoProfileEditor.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/CryptoProfileEditor.vue) 与联调脚本 `mazbri-max/scripts/wire-upupt-upstreams.py`。
     - 单向仅对出站请求签名或加密的上游通道（如 `upupt` 的 `SIGN-*`、`ENV-*` 通道）显式保持 `verify_response = false`、`decrypt_response = false`，响应按标准明文 JSON 解析；
     - 凡显式启用 `verify_response = true` 或响应解密的通道，在 [crypto_wire.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-upstream-runtime/src/adapters/crypto_wire.rs) 中彻底删除静默回退 `serde_json::from_slice(raw)` 的软失败分支，验签或解密失败一律 **Fail-Closed** 返回 `UpstreamError`（同时将响应 `headers` 传入 `SlotBags` 支持响应头验签）。
   - 加固 `UpstreamCryptoProfile::validate` 与 [crypto_pipeline.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-upstream-runtime/src/crypto_pipeline.rs)：
     - 严禁任何 AEAD（`GCM`、`ChaCha20-Poly1305`）与流式模式（`CTR`、`CFB`）使用 `IvSource::KeyDerive`；传统 `CBC` 使用 `IvSource::KeyDerive` 强制要求 `allow_insecure == true`（保留 `upupt` `des-cbc-pkcs5` + `key_derive` 遗留仿真能力）；
     - `scope == NamedFields` 且 `fields.len() > 1` 时：`IvSource::RandomPrefix` 为每个字段独立生成 CSPRNG Nonce；单槽位共享 `IvSource::Random` 配 AEAD/CTR 时经 `HKDF-SHA256(base_iv, info = field_name)` 派生正交子 Nonce 并将 `field_name` 拼入字段 AAD；
     - `Digest` 验签改用 `subtle::ConstantTimeEq::ct_eq`；`CanonStyle::Raw` 在 `fields.len() > 1` 时校验拒绝；`SignThenEncrypt` 严格执行出站先签后密、入站先解后验，并对解密与验签错误统一返回无差别的认证失败错误。
2. **`TransportEnvelope` v1 原位方向与算法绑定加固及四端同步（零并行版本）**
   - 直接在 [transport.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-core/src/crypto/transport.rs) 原位加固 `TransportEnvelope` v1：
     1. 新增枚举 `EnvelopeDirection::{ClientToGateway, GatewayToClient}`（线表示 `"c2g"` / `"g2c"`），从 32 字节租户传输主密钥经 `HKDF-SHA256(master_key, salt = key_version.to_be_bytes(), info = "mazbri-transport-v1|{tenant_id}|{dir}|{enc|mac}")` 派生四把严格单向隔离的子密钥；
     2. AEAD AAD 与外挂 MAC 规范化待签串统一纳入 `(version, direction, tenant_id, key_version, data_alg, mac_alg, timestamp, nonce, iv, ciphertext, tag)`，彻底阻断请求密文反射重放与算法降级混淆；
     3. 对齐 `TenantTransportProfile::validate`、`seal_envelope` 与 `open_envelope` 对 `require_signature == true && mac_alg.is_none()` 的对称校验；修正 `iv_len_for(Sm4EcbPkcs7)` 为 `0` 字节；
     4. 在 [crypto.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-gateway/src/crypto.rs) 中调整顺序为**先完成 `open_envelope` 密码学认证解密，验证通过后再消费 Redis `nonce`**，防止伪造报文抢耗合法 Nonce；租户密钥停用或退役时广播失效网关 `key_cache`。
   - **锁步更新协议向量、文档与 `x-loadgen`**：
     - 设置 `MAZBRI_REGEN_PROTOCOL_VECTORS=1` 运行 `protocol_vectors` 重生成 [tenant-integration-v1.json](file:///workspace/rust-proj/mazbri/docs/protocol-vectors/tenant-integration-v1.json)；
     - 同步更新 [tenant-integration-protocol.md](file:///workspace/rust-proj/mazbri/docs/tenant-integration-protocol.md)、[openapi.yaml](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-gateway/openapi.yaml)、[verify_vectors.py](file:///workspace/rust-proj/mazbri/docs/protocol-examples/python/verify_vectors.py)、[main.go](file:///workspace/rust-proj/mazbri/docs/protocol-examples/go/main.go)、[VerifyVectors.java](file:///workspace/rust-proj/mazbri/docs/protocol-examples/java/VerifyVectors.java) 以及压测仓 [transport.rs](file:///workspace/rust-proj/x-loadgen/x-loadgen-libs/x-loadgen-core/src/crypto/transport.rs)。
3. **商业许可证 `Rekey` 多版本持久环与 CA 生命周期修复**
   - [license.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-cert/src/license.rs) `decide_license`：按 `sticky` ➔ `now >= grace_until`（置粘滞过期）➔ `now < not_before`（返回 `"pending"` 且不打粘滞位）➔ `clock_rollback`（阻断签发但保留 `decryption_allowed`）➔ `in_grace` ➔ `in_active` 顺序判定。
   - 更新 `mazbri-max/init-dbs/src/sql/schemas/cert/init.sql` 中 `commercial_root_materials` 的唯一约束为 `(package_id, material_type, key_version)`；[license_service.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/license_service.rs) 执行 `Rekey` 时在同一 PG 事务内重加密 `ca_storage` 与全量 `certificates.key_pem`，并**追加写入新 `key_version` 的根材料行（同时用新存储密钥重封装历史版本行）而非覆盖删除历史 `key_version`**，保障存量租户密钥与供应商凭据在 `Rekey` 及进程重启后仍可按各自 `key_version` 无损解封。
   - 修复 [generator.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-cert-service/src/ca/generator.rs)：签发与续期按入参 `validity_days` 计算 X.509 `not_after` 并读取真实证书时间与 SHA-256 指纹落库；`CaManager::from_existing` 直接解析数据库持久化的 `ca_cert_pem` 原文（通过 `CertificateParams::from_ca_cert_pem` 恢复签发上下文），严禁重启重签改变根 CA 序列号；`AppState::current_root_material_bundle` 每次复核 `current_decision().issuance_allowed()`。

---

## 4. 二阶与三阶影响深度推演

| 影响阶次 | 潜在连锁反应场景 | 架构消解与防护设计 |
| :--- | :--- | :--- |
| **二阶影响 1：Tonic 切换自定义 `TlsAcceptor` 漏设 HTTP/2 ALPN 导致 Gateway↔Bypass 全量断连** | `tonic::transport::ServerTlsConfig` 隐式注入了 `ALPN = ["h2"]`；手写基于 `ResolvesServerCert` 的 `rustls::ServerConfig` 若未设置 `alpn_protocols`，TLS 握手后将回退无 ALPN，Hyper HTTP/2 连接器立即拒绝所有 gRPC 调用。 | `TlsAcceptor::new_h2` 与 `TlsConnector::new_h2` 显式固化 `alpn_protocols = vec![b"h2".to_vec()]`，并以真实 Tonic Client ↔ Server 证书热轮换集成测试验证 RPC 全程连通。 |
| **二阶影响 2：上游 `crypto_wire` 直接改为无条件验证响应导致单向签名/加密通道全败** | `upupt` 仿真矩阵与大量外部开放平台采用“请求签名/信封加密 + 响应返回普通明文 JSON”的非对称方向约定；若未拆分方向标志就将响应未带签名一律判错，单向通道将 100% 报 `502`。 | 在 `UpstreamCryptoProfile` 显式建立 `verify_response` 与响应解密配置项：单向通道声明仅保护请求，双向通道启用响应保护并严格 Fail-Closed，使安全确定性与业务拓扑精确匹配。 |
| **二阶影响 3：`Rekey` 后重启 `gateway` / `bypass` / `mgmt` 丢失历史 `key_version` 解密能力** | 若 `cert-service` 仅暴露 `current` 单版本根材料接口，服务重启后内存只拿到最新 `key_version`，所有 `Rekey` 前落库的租户密钥与供应商凭据将全部解密失败。 | 新增 `/authorizations/root/materials/keyring` 一次性下发受当前许可证 `decryption_allowed` 门禁管控的全版本密钥环，新加密固定取 `current_key_version`，解密按密文头 `key_version` 精确索引。 |
| **三阶影响：`TransportEnvelope` v1 原位方向绑定对跨语言接入与压测引擎的一致性约束** | 修改 `TransportEnvelope` v1 的子密钥派生与 AAD/MAC 规范串会改变线协议密文与认证标签输出。 | 在同一交付单元内自动重生成黄金测试向量 `tenant-integration-v1.json`，并通过 Rust 单测及 Python/Go/Java 独立向量校验脚本与 `x-loadgen` 全矩阵压测联合门禁验证。 |

---

## 5. 验收门禁与实测清单

1. **自动化单测与多语言向量验证**
   - 模拟 5 秒短有效期 mTLS 证书：启动 `tarpc` 与 `BypassGrpcServer`（`ALPN = h2`）及客户端池，触发 `CertManager` 续期，原证书过期后断言新建连接呈递新序列号证书且 RPC 返回成功；使用 `mazbri-cache` 证书连接 `mazbri-billing` 断言被 Peer SAN ACL 拒绝。
   - 执行商业许可证 `Rekey` + 服务重启集成测试：预置 `key_version = 1` 的微服务证书、租户密钥与 `enc:v3:1:...` 供应商凭据，导入 `Rekey`（`key_version = 2`）许可证并重启 `cert`/`mgmt`/`bypass`/`gateway`，断言存量 `v1` 凭据与密钥全部正常解密，新建凭据写入 `enc:v3:2:...`。
   - 执行配置中心敏感配置创建、修改、回滚与导出导入闭环测试：断言回滚敏感历史版本成功通过明文 TOML 校验，导出的 ZIP 在新集群导入后 `GET /configs/{service}` 正常解密读取。
   - 运行 `cargo test --package mazbri-core --test protocol_vectors`、`python3 docs/protocol-examples/python/verify_vectors.py`、`go run ./docs/protocol-examples/go`、`java docs/protocol-examples/java/VerifyVectors.java` 全部通过；验证将客户端请求 `TransportEnvelope` 直接反射作为响应输入客户端解封时因方向子密钥隔离被 100% 拒绝。
2. **部署与验证顺序**
   - 更新 `init-dbs` (`cert/init.sql`)、全套微服务二进制与 `x-loadgen` 后，执行 `redep rerun`（验证阶段一空库 `mgmt` 正常启动导证、阶段二数据面预热密钥环就绪），并运行 `upupt` 全通道试跑与 `xlg-verify.py` 明密文全矩阵验收。
