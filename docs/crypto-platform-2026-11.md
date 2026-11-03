# Mazbri 通用密码学平台（2026-11）

对内实现说明。算法闭集、Profile 契约与接入方式以**本仓库代码**为准；`/workspace/analysis/upupt-crypto-mazbri-gap.md` 仅作历史证据索引，**不是**产品目标。

## 目标与非目标

| 做 | 不做 |
|---|---|
| 软件实现的可配置加解密平台（租户传输 / 上游报文 / 落库信封） | 密码机 / SDF / HSM / 远程密码服务 |
| 统一 `mazbri-crypto` 注册表 + kebab-case `AlgorithmId` | SM9、首期后量子 |
| 两类可版本化 Profile + 命名套件 | 厂商品牌专用 adapter / 核心枚举（如 `UpuptAdapter`） |
| Legacy 默认拒绝，显式 `allow_insecure` 才放行 | 旧算法双栈、别名兼容层、feature 开关「旧信封」 |

**无用户绿场**：破坏性一次切干净；init-dbs 以权威 SQL 为准重建，不做读时兼容列。

## 分层

```
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

业务 crate **禁止**直接 `use aes_gcm` / 国密 crate 做业务加密；一律经注册表。

## Wire / DB：kebab-case

- 算法 id 字符串一律注册表 kebab-case，例如 `aes-256-gcm`、`hmac-sm3`、`sm4-cbc-pkcs7`。
- 未知 id **严格报错**；不做大小写或历史别名（如旧 `AES-256-GCM`）兼容。
- Profile / OpenAPI / UI / 落库前缀共用同一套 id。

## 算法闭集（摘要）

实现见 `mazbri_crypto::AlgorithmId::all()`。

- **推荐**：`aes-256-gcm`、`chacha20-poly1305`、`hmac-sha256|384|512`、`rsa-oaep-sha256`、`ecdsa-p256-sha256`、`ed25519`
- **国密**：`sm3` / `hmac-sm3`、`sm4-cbc-pkcs7` / `sm4-ctr`、`sm2-encrypt` / `sm2-sign`；**无**假 `sm4-gcm`，国密套件用 `suite/gm-sm4-hmac-sm3`（CBC + HMAC-SM3）
- **主流兼容**：AES-CBC/CTR/CFB（非 AEAD 须配 MAC）、RSA PKCS#1 v1.5
- **Legacy（默认拒绝）**：`aes-*-ecb`、`sm4-ecb-pkcs7`、`des-cbc-pkcs5`、`md5` / `hmac-md5`、`rsa-pkcs1v15-private-encrypt`

安全分级见 `SecurityClass`；唯一门闩 `legacy::assert_not_legacy_or_allowed`。

### 命名套件

| 名称 | 数据算法 | MAC |
|---|---|---|
| `suite/tenant-default-2026-11` | `aes-256-gcm` | — |
| `suite/gm-sm4-hmac-sm3` | `sm4-cbc-pkcs7` | `hmac-sm3` |
| `suite/tenant-chacha20` | `chacha20-poly1305` | — |

未知名套件返回 `UnknownSuite`，禁止静默回落 AES。

## Profile

### `TenantTransportProfile`（租户 ↔ 网关）

| 字段 | 说明 |
|---|---|
| `version` | schema ≥ 1 |
| `mode` | `none` \| `request` \| `full` |
| `suite` / `data_alg` / `mac_alg` | 可并用；冲突则校验失败；`mode≠none` 须有数据算法 |
| `key` | `latest` \| `fixed`；加密模式必填 |
| `require_signature` / `binding` | 绑定与外层签名开关 |
| `allow_insecure` | Legacy 放行 |
| `nonce_ttl_secs` / `clock_skew_secs` | 默认 300；须 > 0 |

规则要点：`mode=none` 不得带 suite/算法或 `require_signature`；非 AEAD 对称必须有 `mac_alg`。

热路径：**gateway + SDK**。Mgmt 只配 Profile / UI，不跑报文密码。

### `UpstreamCryptoProfile`（平台 ↔ 上游）

| 字段 | 说明 |
|---|---|
| `order` | `sign_then_encrypt` \| `encrypt_then_sign` \| `encrypt_only` \| `sign_only` \| `aead_only` |
| `sign` / `crypt` / `wrap` / `response_crypt` | 子规格；算法为 kebab-case id |
| `canon` | `sorted_kv` \| `concat` \| `raw` \| `jws_style` |
| `encodings` | payload / signature / iv：`base64` \| `base64_url` \| `hex`(+case) |
| `slots` | header / query / form / json 放置角色字段 |
| `iv` / `scope` | IV 来源；`whole_body` \| `named_fields` |
| `allow_insecure` | Legacy 放行 |

`encrypt_only` 禁止裸 CBC 等无完整性组合；`aead_only` 要求 `crypt.alg` 为 AEAD。

执行：`mazbri-upstream-runtime` 流水线（`seal_request` / `open_response`），由 **bypass** 与同源适配器接线。

## Bypass vs 微服务自接入

| 方式 | 谁跑加解密 | 何时用 |
|---|---|---|
| **Bypass** | 平台 bypass / upstream-runtime 按渠道 `UpstreamCryptoProfile` 封包 | 运营配置上游即可，业务服务**免写**密码代码 |
| **微服务自接入** | 服务进程依赖同一 `mazbri-crypto`（+ Profile / SDK）本地 seal/open | 服务要自己控报文；与 bypass **共用闭集与画像契约**，不经 bypass 亦可 |

二者不是两套算法；禁止各写一套闭集或租户页 / Workbench 各一套选择器。

## 落库信封（L2c）

- 格式：`enc:v2:<kebab-alg>:` + base64(nonce ‖ ciphertext‖tag)
- 默认算法：`aes-256-gcm`（亦可注册表内其它 AEAD，如 `chacha20-poly1305`）
- 密钥：`MAZBRI_SUPPLIER_SECRETS_KEY`；旧前缀密文**一律拒绝**（无双读）

## Legacy 政策

1. `SecurityClass::Legacy` 算法在 Profile `validate` 与执行路径均走 `assert_not_legacy_or_allowed`。
2. 未设 `allow_insecure=true` → `CryptoError::LegacyNotAllowed`。
3. UI / OpenAPI 应默认隐藏或强提示；样例见 `tests/fixtures/legacy-aes-ecb-hmac-sha256.json`。

## 样例 fixture

路径：`mazbri-libs/mazbri-crypto/tests/fixtures/`（集成测试自动加载目录下全部 `*.json`）。

| 文件 | 用途 |
|---|---|
| `aes-gcm-aead-only.json` | 现代 AEAD-only 上游样例 |
| `gm-sm4-hmac-sm3-encrypt-then-sign.json` | 国密 SM4-CBC + HMAC-SM3，`encrypt_then_sign` |
| `legacy-aes-ecb-hmac-sha256.json` | Legacy ECB + HMAC；`allow_insecure=true` 才通过校验 |

文件名与类型保持**通用**；可将历史现场组合译制为样例，但**不**引入 upupt 专用类型或「为兼容某厂商优先」的文档口径。

仓库内若无独立 loadgen 场景目录，以上 fixture + 向量测试即为互操作夹具；压测场景应按通用 profile 组织，勿以厂商端口号为唯一命名。

## 破坏性变更备忘（发版写入 CHANGELOG）

- 传输信封字段改为 `data_alg` / `mac_alg`（kebab-case）；删除写死 `AES-256-GCM` / `HMAC-SHA256` 常量校验。
- 上游 egress 以 `UpstreamCryptoProfile` 为准；`sign_form` 不再限制「仅 HMAC-SHA256」。
- 落库前缀改为自描述 `enc:v2:<alg>:`；旧密文须重置。
- init-dbs 租户传输 / wrapping CHECK 与闭集对齐；本地用 `--reset --force` 重建。

## 相关代码入口

- crate：`mazbri-libs/mazbri-crypto`
- 租户传输：`mazbri-core` transport + gateway / `mazbri-sdk`
- 上游流水线：`mazbri-upstream-runtime`
- 落库：`mazbri-core` `supplier_secrets`
- FE：共享套件 / Profile 选择器（租户页与 Workbench 同源闭集）
