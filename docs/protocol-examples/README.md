# 租户接入协议多语言示例

每个示例都读取 [`../protocol-vectors/tenant-integration-v1.json`](../protocol-vectors/tenant-integration-v1.json)，
用各语言的**标准/主流密码库独立实现**协议（不调用 mazbri 代码），逐项断言与向量一致。协议说明见
[`../tenant-integration-protocol.md`](../tenant-integration-protocol.md)。

> 向量里的密钥全部是公开的测试密钥（`test_only: true`），不得用于真实租户。

| 目录 | 运行 | 依赖 | 覆盖 / 跳过 |
|---|---|---|---|
| `python/` | `python3 -m venv .venv && .venv/bin/pip install 'cryptography>=42' && .venv/bin/python verify_vectors.py` | cryptography | 传输 17/17（含 SM4、legacy；单 DES 经 decrepit TripleDES）、绑定 8/9、回调；跳过 sm2-v1 |
| `go/` | `go run .` | Go 1.22+ 标准库 | 传输 aes-gcm/cbc/ctr（8 条）、绑定 hmac sha2 / rsa-v1 / ecdsa-v1 / ed25519-v1（7 条）、回调；跳过 chacha20、cfb、国密、legacy |
| `java/` | `javac VerifyVectors.java && java VerifyVectors` | JDK 17+ | aes-gcm/cbc + MAC、Hmac*、SHA*withRSA（PKCS#1 v1.5）、Ed25519、ECDSA P1363 验签、回调 |

退出码 0 且末行 `PASS ...` 表示通过。可传入向量文件路径作为第一个参数。

注意：
- Python `cryptography` 不能解析 PKCS#8 v2 的 Ed25519 私钥，示例中从 DER 内层 OCTET STRING（`04 20 <seed>`）取 32 字节种子；也可用 `openssl pkey` 转成 v1。
- ECDSA / SM2 签名非确定性，只验签；RSA（PKCS#1 v1.5）、HMAC、Ed25519 可逐字节复现。
