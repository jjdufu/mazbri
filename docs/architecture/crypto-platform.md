# Mazbri 通用密码学平台架构

算法闭集、Profile 契约与接入方式以仓库代码实现为准：

- 算法原语与注册表：`mazbri-libs/mazbri-crypto`
- 租户传输与落库加密：`mazbri-libs/mazbri-core`（`transport`、`supplier_secrets`）
- 上游报文加解密流水线：`mazbri-libs/mazbri-upstream-runtime`

## 1. 范围与边界

| 支持范围 | 明确不做 |
|---|---|
| 纯软件可配置加解密（租户传输 / 上游报文 / 落库信封） | 硬件密码机 / SDF / HSM / 远程密码服务 |
| 统一 `mazbri-crypto` 注册表 + kebab-case `AlgorithmId` | SM9 与后量子算法 |
| 版本化 `TenantTransportProfile` / `UpstreamCryptoProfile` + 命名套件 | 厂商专用硬编码 Adapter 枚举 |
| `SecurityClass::Legacy` 默认拒绝，显式 `allow_insecure=true` 放行 | 历史算法大小写别名或旧信封双读兼容 |

## 2. 分层架构

```text
mazbri-crypto          算法注册表 + 原语（AEAD / 对称 / MAC / Digest / Asym）
        │
   ┌────┴────┐
   │         │
TenantTransportProfile   UpstreamCryptoProfile
（租户 ↔ 网关）            （平台 ↔ 上游）
   │         │
gateway + SDK            bypass + mazbri-upstream-runtime
   │
At-rest（supplier_secrets 等）：enc:v2:<kebab-alg>:…
```

业务 crate 不直接引用底层密码原语库做业务加解密，统一经 `mazbri-crypto` 注册表调用。

## 3. 算法闭集与命名套件

### 3.1 算法标识（kebab-case）

实现见 `mazbri_crypto::AlgorithmId::all()`，Profile、OpenAPI、前端选择器与落库前缀共用同一套 kebab-case ID，未知 ID 直接报错：

- **现代标准**：`aes-256-gcm`、`chacha20-poly1305`、`hmac-sha256`、`hmac-sha384`、`hmac-sha512`、`rsa-oaep-sha256`、`ecdsa-p256-sha256`、`ed25519`
- **国密标准**：`sm3`、`hmac-sm3`、`sm4-cbc-pkcs7`、`sm4-ctr`、`sm2-encrypt`、`sm2-sign`（国密对称推荐组合使用 `suite/gm-sm4-hmac-sm3`）
- **通用兼容**：AES-CBC / CTR / CFB（非 AEAD 对称算法必须搭配 MAC）、RSA PKCS#1 v1.5
- **Legacy（默认拒绝）**：`aes-*-ecb`、`sm4-ecb-pkcs7`、`des-cbc-pkcs5`、`md5`、`hmac-md5`、`rsa-pkcs1v15-private-encrypt`

Legacy 门闩统一走 `mazbri_crypto::legacy::assert_not_legacy_or_allowed`，未开启 `allow_insecure=true` 时返回 `CryptoError::LegacyNotAllowed`。

### 3.2 命名套件

| 套件名称 | 数据加密算法 | MAC 算法 |
|---|---|---|
| `suite/aes-256-gcm` | `aes-256-gcm` | — |
| `suite/gm-sm4-hmac-sm3` | `sm4-cbc-pkcs7` | `hmac-sm3` |
| `suite/tenant-chacha20` | `chacha20-poly1305` | — |

未知名套件返回 `UnknownSuite`，禁止静默回退。

## 4. Profile 契约

### 4.1 `TenantTransportProfile`（租户 ↔ 网关）

| 字段 | 说明 |
|---|---|
| `mode` | `none` \| `request` \| `full` |
| `suite` / `data_alg` / `mac_alg` | 冲突时校验失败；`mode ≠ none` 必须指定数据算法；非 AEAD 必须配 `mac_alg` |
| `require_signature` | 外层签名开关（`mode = none` 时禁止携带套件或 `require_signature`） |
| `allow_insecure` | Legacy 算法放行开关 |
| `nonce_ttl_secs` / `clock_skew_secs` | 防重放窗口与时钟偏差秒数（默认 `300`，须 `> 0`） |

数据面加解密在 `mazbri-gateway` 与租户侧执行，`mazbri-mgmt` 仅负责配置管理与校验。

### 4.2 `UpstreamCryptoProfile`（平台 ↔ 上游）

| 字段 | 说明 |
|---|---|
| `order` | `sign_then_encrypt` \| `encrypt_then_sign` \| `encrypt_only` \| `sign_only` \| `aead_only` |
| `sign` / `crypt` / `wrap` / `response_crypt` | 签名、对称加密、密钥包装与响应解密子规格（kebab-case 算法 ID） |
| `canon` | 规范化拼串方式：`sorted_kv` \| `concat` \| `raw` \| `jws_style` |
| `encodings` | `payload` / `signature` / `iv` 线编码：`base64` \| `base64_url` \| `hex`（含大小写） |
| `slots` | `header` / `query` / `form` / `json` 角色字段落位规则 |
| `iv` / `scope` | IV 来源；加密作用域 `whole_body` \| `named_fields` |
| `allow_insecure` | Legacy 算法放行开关 |

安全约束：`encrypt_only` 禁止裸 CBC 等无完整性校验组合；`aead_only` 要求 `crypt.alg` 为 AEAD 算法。

| 接入模式 | 执行位置 | 适用场景 |
|---|---|---|
| **透明代理（Bypass）** | `mazbri-bypass` 经 `mazbri-upstream-runtime`（`seal_request` / `open_response`）按渠道配置自动封拆包 | 免代码接入外部供应商接口 |
| **微服务自接入** | 业务微服务直接依赖 `mazbri-crypto` 按同一套闭集与 Profile 契约本地封拆包 | 服务需自主控制复杂业务报文编排 |

## 5. 落库凭据信封与测试向量

- **落库密文格式**：`enc:v2:<kebab-alg>:` + `base64(nonce ‖ ciphertext ‖ tag)`，默认算法 `aes-256-gcm`。
- **主密钥来源**：环境变量 `MAZBRI_SUPPLIER_SECRETS_KEY`（64 位 hex 或标准 base64 编码的 32 字节密钥）。
- **互操作样例向量**：位于 `mazbri-libs/mazbri-crypto/tests/fixtures/`（集成测试自动遍历加载全部 `*.json`）：

| 向量文件 | 覆盖场景 |
|---|---|
| `aes-gcm-aead-only.json` | 现代 AEAD-only 上游报文加解密 |
| `gm-sm4-hmac-sm3-encrypt-then-sign.json` | 国密 SM4-CBC + HMAC-SM3（`encrypt_then_sign`） |
| `legacy-aes-ecb-hmac-sha256.json` | Legacy AES-ECB + HMAC-SHA256（需 `allow_insecure=true`） |
