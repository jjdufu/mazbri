# mazbri 租户接入协议 v1（对外）

> 本文描述外部租户调用 mazbri 网关所需的全部线上协议：API Key 换 token、传输加密信封、请求绑定签名、回调签名、错误码。
> 所有字段、拼接顺序、编码方式均**以代码为准**；出处在每节末尾以「实现」标注，向量由 Rust 测试对实现逐字节校验。
>
> - 固定测试向量：[`docs/protocol-vectors/tenant-integration-v1.json`](protocol-vectors/tenant-integration-v1.json)
> - 向量校验测试：`mazbri-libs/mazbri-core/tests/protocol_vectors.rs`
> - 多语言示例：[`docs/protocol-examples/`](protocol-examples/)（Python / Go / Java，均读取同一份向量并断言）
>
> **向量中的所有密钥都是专门生成的测试密钥（`test_only: true`），公开可见，严禁用于任何真实租户或生产环境。**

## 0. 总览

```
租户                                           mazbri-gateway
 │ POST /auth/token {api_key}                    │
 │ ─────────────────────────────────────────────▶│  校验 API Key → 签发 JWT（scope 含 api:write）
 │ ◀───────────── {access_token, expires_in} ────│
 │                                               │
 │ POST /api/v1/gateway                          │
 │   Authorization: Bearer <access_token>        │  ① JWT 校验 + scope
 │   [X-Mazbri-* 请求绑定头]                       │  ② 请求绑定（按租户 profile，可选/必选）
 │   body = 明文 GatewayRequest | 加密信封          │  ③ 信封：时间窗 → nonce 去重 → MAC → 解密
 │ ─────────────────────────────────────────────▶│  ④ 转发上游
 │ ◀──── 明文响应 | 加密信封（mode=full）──────────│  ⑤ mode=full 时封装响应
```

租户侧需要从平台运营方拿到的材料（均通过管理端 mgmt 生成，平台**只展示一次**的值需租户自行妥善保存）：

| 材料 | 用途 | 来源（mgmt） |
|---|---|---|
| API Key | 换 token | 租户 API Key 管理 |
| 传输密钥 `tenantSecretKey`（32 字节，base64）+ `keyVersion` | 加密信封的数据密钥与 MAC 密钥 | `POST /tenants/{code}/crypto-keys`、`.../rotate`（`shownOnce: true`） |
| 传输 profile（mode / 算法 / 时间窗等） | 决定信封用什么算法 | `GET /tenants/{code}/crypto` |
| 请求绑定 `kid` + 密钥 | 请求绑定签名 | `POST /tenants/{code}/request-keys`：HMAC 由平台生成 `hmacKey`（只展示一次）；非对称由租户自持私钥、只上传公钥（SPKI PEM） |
| 回调 secret | 校验平台回调 | 回调订阅配置 |

## 1. API Key 换 token

```
POST /auth/token
Content-Type: application/json

{"api_key": "<API Key>", "grant_type": "client_credentials"}
```

- `grant_type` 可省略，缺省即 `client_credentials`；其他值按 API Key 非法处理。
- 成功（200）：

```json
{
  "success": true,
  "access_token": "<JWT>",
  "token_type": "Bearer",
  "expires_in": 3600,
  "tenant_info": {"tenant_id": "<uuid>", "tenant_code": "<code>", "tenant_name": "<name>", "account_type": "<type>"}
}
```

  `expires_in` 单位秒，由租户配置决定；示例值仅示意。调用网关前须确认 token 未过期，过期后重新换取。

- 失败：`{"success": false, "error": "<原因>"}`，HTTP 状态：

| 错误 | HTTP | 说明 |
|---|---|---|
| InvalidApiKey | 401 | API Key 错误 / 超长 / grant_type 不支持 |
| TenantNotFound | 404 | 租户不存在 |
| TenantSuspended | 403 | 租户已停用 |
| RpcError | 503 | 依赖服务暂不可用，可重试 |
| InternalError | 500 | 内部错误 |

实现：`mazbri-bin/mazbri-gateway/src/auth/service.rs`（`TokenRequest` / `TokenResponse` / `token_handler`）。

## 2. 调用网关

```
POST /api/v1/gateway
Authorization: Bearer <access_token>
Content-Type: application/json
```

明文请求体 `GatewayRequest`：

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `product_id` | string | 是 | 产品 ID（由平台分配） |
| `path` | string | 是 | 上游路径 |
| `method` | string | 否 | 缺省 `POST` |
| `fingerprint` | string | 否 | 幂等指纹；同指纹重复请求重放已完成结果 |
| `query` | object | 否 | 上游 query |
| `headers` | object | 否 | 上游 header |
| `body` | any | 否 | 上游 body |

网关先去掉前导空白，按前缀走快速路径：以 `{"encrypted":` 开头按加密信封（§3）解析，以 `{"product_id":` 开头按明文解析；
其余情况先尝试按信封、再按明文解析，都失败 → 400。建议把这两个字段放在第一个键（向量中的 plaintext / envelope 即为该形式）。

- `encryption_mode=none` 的租户只能发明文；发信封 → 400 `E4000`。
- `encryption_mode=request|full` 的租户必须发信封；发明文 → 400 `E4000`。
- JWT 缺失/无效 → 401（带 `WWW-Authenticate: Bearer realm="mazbri", error="invalid_token"`）；
  请求体不可读 → 400；token 缺 `api:write` scope → 403。此三类响应体为 `{"success":false,"error":"...","status":<http>}`。

实现：`mazbri-bin/mazbri-gateway/src/auth/middleware.rs`（`jwt_auth_middleware`）、`gateway/handler.rs`。

## 3. 传输加密（profile 与信封）

### 3.1 Profile

| 字段 | 取值 | 说明 |
|---|---|---|
| `encryption_mode` | `none` / `request` / `full` | `request`：请求必须加密，响应明文；`full`：请求、响应都加密 |
| `suite` | `suite/tenant-default-2026-11`（aes-256-gcm）、`suite/gm-sm4-hmac-sm3`（sm4-cbc-pkcs7 + hmac-sm3）、`suite/tenant-chacha20`（chacha20-poly1305） | `data_alg`/`mac_alg` 为空时由套件决定 |
| `data_alg` | 见 §3.4 | 显式数据算法，优先于 suite |
| `mac_alg` | `hmac-sha256` / `hmac-sha384` / `hmac-sha512` / `hmac-sm3`（legacy：`hmac-md5`） | 外挂 MAC；AEAD 可不配，非 AEAD **必须**配（profile 校验报 `data_alg ... requires mac_alg`） |
| `require_signature` | bool | 为 true 时必须配置 `mac_alg`（信封必须带外挂 MAC） |
| `allow_insecure` | bool | 仅为 legacy 算法（ECB / DES / HMAC-MD5）开闸，默认 false |
| `current_key_version` | 如 `v1` | 响应加密使用的密钥版本；请求信封里的 `keyVersion` 可以是任一有效版本 |
| `clock_skew_seconds` | 默认 300 | 信封 `timestamp` 与请求绑定时间戳的允许偏差 |
| `nonce_ttl_seconds` | 默认 300 | nonce 去重窗口 |
| `require_request_binding`、`signature_version` | 见 §4 | 请求绑定 |

信封中的 `dataAlg` / `macAlg` 必须与 profile 生效算法**完全一致**，否则 400 `E4000`（`invalid transport envelope/profile: ...`）。
算法 id 为 kebab-case 闭集，不接受大小写变体或历史别名。

### 3.2 密钥派生

传输密钥 `tenantSecretKey` 恒为 32 字节（base64 解码后）：

- **数据密钥** = 前 `key_len(data_alg)` 字节（见 §3.4）；
- **MAC 密钥** = 完整 32 字节。

### 3.3 信封格式（JSON，camelCase）

```json
{
  "encrypted": true,
  "keyVersion": "v1",
  "dataAlg": "aes-256-gcm",
  "macAlg": "hmac-sha256",
  "timestamp": 1767225600,
  "nonce": "LS4vMDEyMzQ1Njc4OTo7PA",
  "iv": "oKGio6Slpqeoqaqr",
  "ciphertext": "<标准 base64，不含 tag>",
  "tag": "<标准 base64；非 AEAD 为空串>",
  "signature": "<小写 hex；无 macAlg 时为空串>"
}
```

| 字段 | 编码 | 说明 |
|---|---|---|
| `encrypted` | bool | 必须为 `true` |
| `keyVersion` | string | 传输密钥版本 |
| `dataAlg` / `macAlg` | string | 与 profile 一致；无 MAC 时省略 `macAlg` |
| `timestamp` | 整数（unix 秒） | 与服务器时间偏差 ≤ `clock_skew_seconds` |
| `nonce` | base64url 无填充 | 防重放；参考实现为 16 字节随机数（22 字符）。同租户 + 同 keyVersion 在 `nonce_ttl_seconds` 内只能用一次 |
| `iv` | 标准 base64 | 每次随机，长度见 §3.4 |
| `ciphertext` | 标准 base64 | 明文 = `GatewayRequest` JSON 的 UTF-8 字节 |
| `tag` | 标准 base64 | AEAD 的 16 字节认证标签；非 AEAD 为 `""` |
| `signature` | 小写 hex | 外挂 MAC，见下 |

**AAD**（仅 AEAD）= 租户 ID 字符串（UUID 小写带连字符，即 token 中 `tenant_info.tenant_id`）的 UTF-8 字节。非 AEAD 无 AAD。

**MAC canonical string**（7 个 `\n`，无结尾换行）：

```
{tenant_id}\n{http_path}\n{keyVersion}\n{timestamp}\n{nonce}\n{iv}\n{ciphertext}\n{tag}
```

- `http_path` = 本次 HTTP 请求的 path（含 query，如有），即 `/api/v1/gateway`；
- `iv` / `ciphertext` / `tag` / `nonce` 均使用信封中的**编码后字符串**原样拼接；
- `signature = hex(HMAC_{mac_alg}(MAC 密钥, canonical))`。

网关处理顺序：时间窗（超出 → 403 `E4030`）→ nonce 去重（Redis `SET NX`，重复 → 409 `E4090`）→ 取密钥版本 → MAC 校验（不符 → 403 `E4030 Invalid encrypted request signature`）→ 解密（失败 → 400 `E4000`）→ 解析 `GatewayRequest`。

**响应（仅 `full`）**：凡经上游处理的响应（含上游返回的非 2xx）都以同样格式封装：使用 `current_key_version`、新的 timestamp/nonce/iv、相同 AAD 与 `http_path`；HTTP 状态码保持上游状态。租户应校验 MAC 后解密。
网关自身在鉴权、绑定、信封校验、限流等阶段产生的错误**不加密**，按 §6 的明文错误体返回——客户端应先看 HTTP 状态与 body 是否为信封再处理。

实现：`mazbri-libs/mazbri-core/src/crypto/transport.rs`（`canonical_signature_payload`、`seal_envelope`、`open_envelope`、`iv_len_for`、`data_key_len`、`validate_transport_algorithms`）；`mazbri-bin/mazbri-gateway/src/crypto.rs`（`decrypt_request_value`、`encrypt_response_bytes`、`validate_timestamp`、`check_nonce`）。

### 3.4 数据算法参数

| `data_alg` | 类型 | 密钥字节 | IV 字节 | 填充 | tag | 需要 MAC |
|---|---|---|---|---|---|---|
| `aes-128-gcm` / `aes-192-gcm` / `aes-256-gcm` | AEAD | 16 / 24 / 32 | 12 | 无 | 16 字节 | 可选 |
| `chacha20-poly1305` | AEAD | 32 | 12 | 无 | 16 字节 | 可选 |
| `aes-128-cbc` / `aes-256-cbc` | 分组 | 16 / 32 | 16 | PKCS#7 | 无 | 必须 |
| `aes-128-ctr` / `aes-256-ctr` | 流 | 16 / 32 | 16（初始计数块，128 位大端计数） | 无 | 无 | 必须 |
| `aes-128-cfb` / `aes-256-cfb` | 流 | 16 / 32 | 16 | 无（CFB-128） | 无 | 必须 |
| `sm4-cbc-pkcs7` | 分组（国密） | 16 | 16 | PKCS#7 | 无 | 必须 |
| `sm4-ctr` | 流（国密） | 16 | 16 | 无 | 无 | 必须 |
| legacy `aes-128-ecb` / `aes-256-ecb` | 分组 | 16 / 32 | 0（`iv` 为 `""`） | PKCS#7 | 无 | 必须 |
| legacy `sm4-ecb-pkcs7` | 分组 | 16 | 0 | PKCS#7 | 无 | 必须 |
| legacy `des-cbc-pkcs5` | 分组 | 8 | 8 | PKCS#5 | 无 | 必须 |

legacy 算法（以及 `hmac-md5`）仅在 profile `allow_insecure=true` 时可用，未开闸时 profile 校验失败（`LegacyNotAllowed`），网关也拒绝此类信封。仅为兼容存量系统，新接入请勿使用。

## 4. 请求绑定（X-Mazbri-* 签名）

请求绑定把 HTTP 请求（路径、时间、nonce、原始 body）与租户持有的密钥绑在一起，防止 token 被截获后伪造/重放请求。它与 §3 的传输加密**相互独立**，可叠加：加密信封作为 body 时，签的是信封 JSON 原文。

### 4.1 何时校验

- profile `require_request_binding=true`（或网关全局开启）→ 必须带全部 5 个头；
- 否则：一个绑定头都不带 → 跳过；**只要带了任意一个**，就按完整规则校验。
- 校验发生在 JWT 校验之后、信封解密之前，仅针对 `POST /api/v1/gateway`。

### 4.2 请求头

| Header（大小写不敏感） | 取值 |
|---|---|
| `X-Mazbri-Timestamp` | unix 秒，十进制整数，不带前导零；与服务器偏差 ≤ `clock_skew_seconds` |
| `X-Mazbri-Nonce` | 16–128 个字符，字符集 `[A-Za-z0-9._-]`；同租户在 `nonce_ttl_seconds` 内只能用一次 |
| `X-Mazbri-Signature` | 签名字节的**标准 base64**（带填充） |
| `X-Mazbri-Signature-Input` | `{alg}=v1`，如 `hmac-sha256=v1`、`rsa-pkcs1v15-sign-sha256=v1` |
| `X-Mazbri-Key-Id` | mgmt 登记的 `kid` |

### 4.3 签名串（canonical string）

```
POST\n{path_and_query}\n{timestamp}\n{nonce}\n{body}
```

- `POST` 固定大写；`path_and_query` 为请求行中的 path（含 `?query`，如有），即 `/api/v1/gateway`；
- `timestamp` 为整数的十进制表示（与头一致）；
- `body` 为 HTTP 请求体**原始字节**按 UTF-8 解释（明文 JSON 或信封 JSON），**发送的字节必须与签名用的字节完全一致**（不要在签名后重新序列化）；
- 行间 `\n`，无结尾换行。

### 4.4 协议档与算法

profile `signature_version` 决定允许的协议档，`X-Mazbri-Signature-Input` 声明的 `alg` 必须等于该 `kid` 登记的算法，且属于该协议档：

| `signature_version` | 允许的 `alg` | 签名 | 密钥 | 确定性 |
|---|---|---|---|---|
| `hmac-v1` | `hmac-sha256` / `hmac-sha384` / `hmac-sha512` / `hmac-sm3` | HMAC 输出原始字节 | mgmt 返回的 `hmacKey` **base64 解码后**的字节 | 是 |
| `rsa-v1` | `rsa-pkcs1v15-sign-sha256` / `-sha384` / `-sha512` | **RSASSA-PKCS1-v1_5**（不是 PSS） | 租户私钥 PKCS#8 PEM；登记公钥 SPKI PEM（向量用 2048 位） | 是 |
| `ecdsa-v1` | `ecdsa-p256-sha256` / `ecdsa-p384-sha384` | IEEE P1363 `r‖s` 定长（P-256 64 字节、P-384 96 字节），**不是 DER** | PKCS#8 / SPKI PEM | 否（随机 k） |
| `ed25519-v1` | `ed25519` | RFC 8032 64 字节 | PKCS#8 / SPKI PEM | 是 |
| `sm2-v1` | `sm2-sign` | GB/T 32918 SM2 + SM3，**DER** 编码 `(r, s)`，用户 ID 为默认 `1234567812345678` | PKCS#8 / SPKI PEM | 否 |

> **没有 RSA-PSS。** `rsa-v1` 固定为 PKCS#1 v1.5，签名对同一输入是确定的，因此 RSA 向量可逐字节复现；ECDSA / SM2 向量只能「可验签」，不能复现。

`hmac-md5` 等不安全算法不能用于请求绑定。

### 4.5 错误

全部返回 401，带 `WWW-Authenticate: Bearer realm="mazbri", error="invalid_token"`，body 为 §6 的网关错误体：

| 码 | 含义 |
|---|---|
| `40110` | 需要绑定但缺少任一头 |
| `40111` | 时间戳非整数 / ≤0 / 超出时间窗 |
| `40112` | nonce 已使用（重放） |
| `40113` | 签名不符；或 Signature-Input 格式非法、算法与 kid / 协议档不符；或 nonce 格式非法 |
| `40114` | kid 不存在或已停用 |

校验顺序：头齐全 → Signature-Input 格式 → nonce 格式 → 时间窗 → kid → 验签 → nonce 去重（验签通过后才占用 nonce）。

实现：`mazbri-libs/mazbri-core/src/crypto/request_binding.rs`（`canonical_http_binding`、`SignatureVersion::allows`、`validate_binding_nonce`、`sign_binding`）；`mazbri-bin/mazbri-gateway/src/gateway/request_binding.rs`（`enforce_request_binding`、`verify_binding_signature`）。

## 5. 回调签名（平台 → 租户）

平台向租户回调地址推送事件时带：

| Header | 取值 |
|---|---|
| `X-Mazbri-Timestamp` | unix 秒 |
| `X-Mazbri-Signature` | `hex(HMAC-SHA256(secret 的 UTF-8 字节, "{timestamp}.{payload}"))`，小写 hex |

- `payload` 为回调 JSON 的**紧凑序列化**（无空白，键按字典序，与请求体原文一致——租户应直接用收到的原始 body 计算）；
- 租户应拒绝与本地时间偏差超过 300 秒的回调，并用常量时间比较签名。

实现：`mazbri-libs/mazbri-callback/src/signer.rs`（`generate_signature`）。

## 6. 网关错误码

网关错误体（明文，`full` 模式也不加密）：

```json
{"success": false, "data": null, "error": "[E4030] Forbidden: Invalid encrypted request signature", "status": 403, "request_id": "<uuid>"}
```

| 码 | HTTP | 典型原因 |
|---|---|---|
| `E4000` | 400 | 请求体非法；加密要求不符（明文/信封）；算法与 profile 不符；解密失败 |
| `E4010` | 401 | 未认证 |
| `E4020` | 402 | 余额不足 |
| `E4030` | 403 | 无权访问；信封时间戳超窗；信封 MAC 不符 |
| `E4040` | 404 | 产品/路由不存在 |
| `E4090` | 409 | 信封 nonce 重复；同 fingerprint 请求仍在处理 |
| `E4130` | 413 | 请求体超限 |
| `E4290` | 429 | 限流，带 `Retry-After` |
| `E5000` | 500 | 内部错误 |
| `E5020` | 502 | 上游不可用 |
| `E5030` | 503 | 服务暂不可用，带 `Retry-After` |
| `IDEMPOTENCY_UNAVAILABLE` | 503 | 幂等存储暂不可用，带 `Retry-After: 1`，可原样重试 |
| `E5040` | 504 | 上游超时 |
| `40110`–`40114` | 401 | 请求绑定，见 §4.5 |

实现：`mazbri-bin/mazbri-gateway/src/gateway/response.rs`（`ProxyError::error_code`、`gateway_error_response`）。

## 7. 固定测试向量

文件：`docs/protocol-vectors/tenant-integration-v1.json`（`schema: mazbri-tenant-protocol-vectors/v1`，`test_only: true`）。

| 部分 | 内容 |
|---|---|
| `common` | `tenant_id`（AAD）、`http_path=/api/v1/gateway`、`key_version=v1`、`timestamp=1767225600`（2026-01-01 08:00:00 Asia/Shanghai，真实请求必须用当前时间） |
| `keys` | 传输密钥 `tenant_secret_key_b64`（字节 0x00..0x1f）、HMAC 绑定密钥 `hmac_binding_key_b64`（0x80..0x9f）、`asymmetric.*` 测试密钥对（RSA-2048、ECDSA P-256/P-384、Ed25519、SM2，PKCS#8 / SPKI PEM） |
| `transport[]`（17 条） | 全部数据算法（含 4 条 legacy）：`data_key_hex`、`mac_key_hex`、`iv_hex`、`aad_utf8`、`plaintext_utf8` → 期望 `iv_b64`、`ciphertext_b64`、`tag_b64`、`mac_canonical_string`、`signature_hex`、完整 `envelope_json`（全部可逐字节复现） |
| `binding[]`（9 条） | hmac-sha256（明文 body / 信封 body）、hmac-sm3、rsa sha256 / sha512、ecdsa p256 / p384、ed25519、sm2：期望 `canonical_string`、`signature_b64`、完整 `headers`；`deterministic=false` 的条目只验签 |
| `callback` | payload、secret、timestamp → 期望 `payload_compact`、`signed_message`、两个 header |

`key` 字段是对 `keys` 的引用路径（如 `keys.asymmetric.ecdsa-p256-sha256`）。

**重新生成**（仅当协议变更时）：

```bash
cd mazbri-libs
MAZBRI_REGEN_PROTOCOL_VECTORS=1 cargo test -p mazbri-core --test protocol_vectors   # 重新生成非对称测试密钥与全部期望值
cargo test -p mazbri-core --test protocol_vectors                                   # 校验（CI 默认）
```

校验模式下测试会用实现重算所有确定性值并与 JSON 逐项比对，对 ECDSA / SM2 用公钥验签并验证篡改后失败。

> 兼容提示：实现导出的 Ed25519 私钥是 PKCS#8 v2（RFC 5958，含 `[1] publicKey`）。部分库（如 Python `cryptography`）只解析 v1，
> 可用 `openssl pkey -in k.pem -out k-v1.pem` 转换，或直接取 32 字节种子（示例中有做法）。

## 8. 多语言示例

目录 `docs/protocol-examples/`，每个示例都读取上面的向量 JSON 并断言结果，不依赖任何真实密钥：

| 语言 | 文件 | 依赖 | 覆盖 | 运行 |
|---|---|---|---|---|
| Python | `python/verify_vectors.py` | `cryptography`（≥ 42） | 17 条传输（含 SM4 / legacy）、hmac-v1（含 sm3）、rsa-v1、ecdsa-v1（验签）、ed25519-v1、回调；sm2-v1 跳过（库不支持） | `python3 verify_vectors.py` |
| Go | `go/main.go` | 仅标准库 | aes-gcm / cbc / ctr 传输 + MAC、hmac-v1（sha2）、rsa-v1、ecdsa-v1（验签 + 篡改）、ed25519-v1、回调；chacha / cfb / SM 系列 / legacy 跳过 | `go run .` |
| Java | `java/VerifyVectors.java` | 仅 JDK 17+ | aes-gcm / cbc 传输 + MAC、hmac-v1（sha2）、rsa-v1（`SHA256withRSA`）、ecdsa-v1（`SHA256withECDSAinP1363Format` 验签）、ed25519-v1、回调 | `javac VerifyVectors.java && java VerifyVectors` |

国密（SM2 / SM3 / SM4）在 Go / Java 中可分别用 `github.com/tjfoc/gmsm`、BouncyCastle 补齐，按本文同样的拼接规则实现即可。

### 8.1 最小片段（Python）

```python
import base64, hashlib, hmac, json, os, time
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import padding
from cryptography.hazmat.primitives.ciphers.aead import AESGCM

# —— 传输信封：aes-256-gcm + hmac-sha256 ——
def seal(tenant_id: str, key32: bytes, key_version: str, plaintext: bytes, http_path="/api/v1/gateway") -> str:
    iv, nonce, ts = os.urandom(12), base64.urlsafe_b64encode(os.urandom(16)).rstrip(b"=").decode(), int(time.time())
    out = AESGCM(key32[:32]).encrypt(iv, plaintext, tenant_id.encode())          # AAD = tenant_id
    ct, tag = base64.b64encode(out[:-16]).decode(), base64.b64encode(out[-16:]).decode()
    ivb = base64.b64encode(iv).decode()
    canon = "\n".join([tenant_id, http_path, key_version, str(ts), nonce, ivb, ct, tag])
    sig = hmac.new(key32, canon.encode(), hashlib.sha256).hexdigest()           # MAC 密钥 = 完整 32 字节
    return json.dumps({"encrypted": True, "keyVersion": key_version, "dataAlg": "aes-256-gcm",
                       "macAlg": "hmac-sha256", "timestamp": ts, "nonce": nonce, "iv": ivb,
                       "ciphertext": ct, "tag": tag, "signature": sig}, separators=(",", ":"))

# —— 请求绑定 ——
def binding_headers(body: str, kid: str, alg: str, sign) -> dict:
    ts, nonce = str(int(time.time())), base64.urlsafe_b64encode(os.urandom(16)).rstrip(b"=").decode()
    canon = "\n".join(["POST", "/api/v1/gateway", ts, nonce, body]).encode()
    return {"X-Mazbri-Timestamp": ts, "X-Mazbri-Nonce": nonce, "X-Mazbri-Key-Id": kid,
            "X-Mazbri-Signature-Input": f"{alg}=v1", "X-Mazbri-Signature": base64.b64encode(sign(canon)).decode()}

hmac_v1 = lambda key: (lambda m: hmac.new(key, m, hashlib.sha256).digest())                   # hmac-sha256
rsa_v1 = lambda priv: (lambda m: priv.sign(m, padding.PKCS1v15(), hashes.SHA256()))           # rsa-pkcs1v15-sign-sha256
```

发送时 body 必须是**签名所用的同一字符串**（如 `requests.post(url, data=body.encode(), headers=...)`，不要用 `json=` 重新序列化）。

### 8.2 最小片段（Go，rsa-v1 / hmac-v1）

```go
canon := strings.Join([]string{"POST", "/api/v1/gateway", ts, nonce, string(body)}, "\n")
// hmac-v1 / hmac-sha256
m := hmac.New(sha256.New, hmacKey); m.Write([]byte(canon)); sig := m.Sum(nil)
// rsa-v1 / rsa-pkcs1v15-sign-sha256（PKCS#1 v1.5，不是 PSS）
h := sha256.Sum256([]byte(canon)); sig, _ = rsa.SignPKCS1v15(nil, rsaPriv, crypto.SHA256, h[:])
req.Header.Set("X-Mazbri-Signature", base64.StdEncoding.EncodeToString(sig))
```

### 8.3 最小片段（Java，rsa-v1 / hmac-v1）

```java
byte[] msg = String.join("\n", "POST", "/api/v1/gateway", ts, nonce, body).getBytes(UTF_8);
Mac mac = Mac.getInstance("HmacSHA256"); mac.init(new SecretKeySpec(hmacKey, "HmacSHA256"));
byte[] hmacSig = mac.doFinal(msg);                                        // hmac-v1
Signature s = Signature.getInstance("SHA256withRSA"); s.initSign(rsaPriv); // rsa-v1 = PKCS#1 v1.5
s.update(msg); byte[] rsaSig = s.sign();
```

## 9. 接入自检清单

1. 时钟同步（NTP），偏差远小于 `clock_skew_seconds`。
2. 每个请求新生成信封 `nonce` / `iv` 和绑定 nonce，绝不复用。
3. 信封的 `dataAlg` / `macAlg` 与 mgmt 中的 profile 一致；密钥版本轮换后更新 `keyVersion`。
4. 签名用的 body 字节 = 发送的 body 字节。
5. 先用本仓库的向量跑通示例，再联调。
6. `full` 模式下：先判断 HTTP 状态与 body 是否为信封；信封先验 MAC 再解密，网关错误为明文。
