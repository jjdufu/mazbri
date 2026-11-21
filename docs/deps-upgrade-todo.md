# 依赖与构建链升级待办

截至 2026-11-21，尚未完成的依赖与构建链观望/跟进事项。

已完成项（不在此列）：
- 工具链统一由 `rust-toolchain.toml` 固定为 `1.100.0`，CI Node `26.13.1` / pnpm `12.21.0` / Actions 升级，且经 PR #83 全量 CI 与 `v0.6.0` 双架构发版、单机 `redep` 全量实测通过；
- `release.yml` 的 `compile` 矩阵已回退至 `ubuntu-24.04` / `ubuntu-24.04-arm`（glibc 2.39 ≤ 运行镜像 `debian:trixie-slim` 的 glibc 2.41）；
- RustCrypto `digest 0.11` 代（`hmac 0.13`、`hkdf 0.13`、`sha1`/`sha2 0.11`、`md-5 0.11`、`sm3 0.5`）、`rsa 0.10`、`p256`/`p384 0.14`、`argon2 0.6`、`tarpc 0.38`、`OpenTelemetry 0.32`、`redis 1.8`、`toml 1.2` / `toml_edit 0.25`；
- `sqlx 0.9.1`（`runtime-tokio` + `tls-rustls-aws-lc-rs`）+ `ipnetwork 0.21.1`（`sqlx` 带入的 `hkdf 0.12`、`md-5 0.10`、`rsa 0.9`、`der 0.7`、`pkcs1 0.7`、`pkcs8 0.10`、`spki 0.7`、`num-bigint-dig` 已从依赖树移除）；
- `rust-i18n 4.2.4`、`zip 9.8.1`（`deflate-flate2`）。

| # | 事项 | 优先级 |
|---|---|---|
| 1 | 观望中的大版本：`jsonwebtoken 12` | 中 |
| 2 | `cipher 0.4` 系列（`cipher 0.5` + `aes 0.9` + `sm4 0.6` + `des 0.9` + 模式 crate） | 中 |
| 3 | 跟进 `rsa 0.10` 补丁版本 | 低 |
| 4 | 跟进 `rust-i18n-support` 移除 `toml 0.8` | 低 |
| 5 | 删除 `.github/actionlint.yaml` | 低 |

## 1. 观望中的大版本：`jsonwebtoken 12`

- **现状**：`mazbri-libs` 与 `mazbri-bin` 使用 `jsonwebtoken 10.4.0`（`aws_lc_rs` + `use_pem`）；上游最新为首个大版本 `12.0.0`。
- **要做**：待 `12.0.x` 补丁版本稳定、下游普遍跟进后单独提交升级，适配破坏性 API 与 feature 变更。
- **影响 / 风险**：影响网关与管理后台 JWT 签发与校验链路。
- **验证**：现有令牌签发 / 校验测试全部通过，并完成一次管理后台登录与会话续期回归。

## 2. `cipher 0.4` 系列

- **现状**：`cipher 0.4.4`、`aes 0.8.4`、`sm4 0.5.1`、`des 0.8.1` 以及配套的 `cbc 0.1.2` / `ctr 0.9.2` / `cfb-mode 0.8.2` / `cfb8 0.8.1` / `ecb 0.1.2` 仍停留在 `cipher 0.4` 代，与已升到 `digest 0.11` 的 RustCrypto 组分属两代（上游 `sm4 0.6.0`、`des 0.9.0` 目前仍处于 `.0` 首版）。
- **要做**：待新一代配套 crate（含 `sm4 0.6.x`、`des 0.9.x`）全部稳定后整体一起升级。
- **影响 / 风险**：涉及租户传输信封、上游报文和落库信封的对称加解密，任何偏差都会让已有密文无法解密。
- **验证**：先用旧版本固化金标准（各算法 / 模式 / 填充的固定密钥、IV、明文 → 密文），升级后原值断言通过；再跑 `x-loadgen` 全套件探测（含 legacy 的 `ecb` / `des` 租户），全部成功。

## 3. 跟进 `rsa 0.10` 补丁版本

- **现状**：`mazbri-crypto` 使用 `rsa 0.10.0`（当前 crates.io 最新仍为 `0.10.0`，旧传递依赖 `rsa 0.9.10` 已随 `sqlx 0.9` 升级一并移除）。
- **要做**：关注 `0.10.x` 补丁版本和安全公告，有发布就跟进。
- **影响 / 风险**：大版本首版可能有缺陷；涉及 RSA 签名、OAEP / PKCS#1 v1.5 加解密和 legacy RSA。
- **验证**：`mazbri-crypto` 的升级金标准测试（`tests/vectors_upgrade_golden.rs`）与全量测试通过，`x-loadgen` 的 RSA 绑定租户探测成功。

## 4. 跟进 `rust-i18n-support` 移除 `toml 0.8`

- **现状**：`rust-i18n` 已升至 `4.2.4`，但其内部 `rust-i18n-support 4.2.4` 的 `Cargo.toml` 仍声明 `toml = "0.8.8"`，导致双仓 `Cargo.lock` 中仍保留 `toml 0.8.23` 与 `toml_edit 0.22.27`（与工作区直接使用的 `toml 1.2` / `toml_edit 0.25` 并存）。
- **要做**：待上游 `rust-i18n-support` 升级至 `toml 1.x`（或将 `toml` 依赖彻底收敛为按需可选）后执行 `cargo update -p rust-i18n` 消除重复版本。
- **影响 / 风险**：仅影响编译期语言包加载传递依赖，无运行时行为变化。
- **验证**：`mazbri-i18n` 中英双语全键测试通过；`cargo tree -d` 中不再出现 `toml 0.8` 与 `toml_edit 0.22`。

## 5. 删除 `.github/actionlint.yaml`

- **现状**：`actionlint` 最新正式版仍为 `v1.7.12`，内置 runner 标签表尚未收录 `ubuntu-26.04`（当前 `ci.yml`、`release-please.yml` 及 `release.yml` 非编译任务仍使用 `ubuntu-26.04`），因此保留 `.github/actionlint.yaml` 显式登记。
- **要做**：`actionlint` 新版本收录 `ubuntu-26.04` / `ubuntu-26.04-arm` 标签后删除该文件。
- **影响 / 风险**：无运行时影响；提前删会导致 `actionlint` 报 `runner-label` 错误。
- **验证**：删除后 `actionlint` 对全部 workflow 报 0 个问题。
