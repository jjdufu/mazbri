# 依赖与构建链升级待办与完成清单

截至 2026-11-21，`mazbri` 及其联调配套仓（`mazbri-max`、`upupt`、`x-loadgen`）依赖与构建链升级全量台账（已完成事项以 `[x]` 标记并附提交记录，待上游发布或下次远端发版复核事项以 `[ ]` 标记）。

## 总览矩阵

| 原编号 | 事项 | 当前状态 | 优先级 | 对应提交 / 版本 |
|---|---|---|---|---|
| 前置基线 | Rust `1.100.0`、Node `26.13.1` / pnpm `12.21.0`、RustCrypto `digest 0.11` 代、`rsa 0.10.0`、`p256`/`p384 0.14`、`argon2 0.6`、`tarpc 0.38`、`OpenTelemetry 0.32`、`redis 1.8`、`toml 1.2` | `[x]` 已完成 | 高 | PR #83 / `v0.6.0` |
| 1 | `sqlx 0.9.1` + `ipnetwork 0.21.1` 及运行态全量闭环 | `[x]` 已完成（含热替换运行态验收） | 高 | `b4da4e5` |
| 2 | `release.yml` 编译 runner 的 glibc 上限收敛 | `[x]` 配置已完成（`[ ]` 待下次发版日志复核） | 高 | `94a2003` |
| 3 | 推送后实测 CI / Release 与部署验证 | `[x]` 已完成 | 高 | PR #83、PR #84（`v0.6.0`） |
| 4a | `zip 9.8.1`（原 `0.6.6`） | `[x]` 已完成（含运行态导出/导入验收） | 中 | `7ebb4ba` |
| 4b | `rust-i18n 4.2.4`（原 `3.1.5`） | `[x]` 主体已完成（`[ ]` 待上游移除 `toml 0.8`） | 中 | `643059d` |
| 4c | `jsonwebtoken 12.0.0` + `base64 0.23` | `[x]` 已完成（含运行态会话签发/续期/登出验收） | 中 | `60082dc` |
| 5 | `cipher 0.5` 系列 + `aes-gcm`/`chacha20poly1305 0.11` | `[x]` 已完成（含金标准向量与 21 租户实测） | 中 | `2f02875` |
| 配套仓 | `mazbri-max`、`upupt`、`x-loadgen` 依赖同步升级与热替换实测 | `[x]` 已完成 | 中 | `b5055bd` / `86ccb14` / `1696d4b` |
| 6 | 跟进 `rsa 0.10` 后续补丁版本 | `[ ]` 待上游发布（当前最新为 `0.10.0`） | 低 | — |
| 7 | 删除 `.github/actionlint.yaml` | `[ ]` 待上游支持（`actionlint v1.7.12` 未收录 `26.04`） | 低 | — |

---

## 一、 已完成事项

### 1. `[x]` `sqlx 0.9.1` + `ipnetwork 0.21.1`（本地提交 `b4da4e5`）

- **完成内容**：
  - `[x]` `mazbri-libs` 与 `mazbri-bin` 使用 `sqlx 0.9.1`（`runtime-tokio` + `tls-rustls-aws-lc-rs`）与 `ipnetwork 0.21.1`。
  - `[x]` 全仓动态 SQL 满足 `sqlx 0.9` 的 `SqlSafeStr` 静态约束（零 `AssertSqlSafe` 绕过）：静态列清单采用 `macro_rules! + concat!` 编译期常量，动态 `WHERE` 过滤采用 `QueryBuilder + push_bind` 参数化绑定。
  - `[x]` 双仓 `Cargo.lock` 无 `rsa 0.9`、`num-bigint-dig 0.8`、`hkdf 0.12`、`md-5 0.10` 及 `der 0.7` / `pkcs1 0.7` / `pkcs8 0.10` / `spki 0.7` 残留。
  - `[x]` 静态门禁与单测：前端 `pnpm build` 与 `*.test.ts` 通过；`mazbri-libs` 与 `mazbri-bin` 双仓 `cargo clippy --release --locked --workspace --all-targets -- -D warnings` 零警告；双仓 `cargo test --release --locked --workspace --no-fail-fast` 全部通过，`ts-rs` 绑定零漂移。
  - `[x]` 运行态闭环验收：11 个 `mazbri-*:dev-local` 容器、`x-loadgen-local:0.6.0` 与宿主机 `upupt` 热替换运行健康；`init-dbs` 对 `db51` Redpanda 幂等同步与存量库预检通过；`xlg-verify.py` 21/21 租户拨测、异步任务、smoke（200/200）、load（3000/3000，`99.75 QPS`，零失败）、调用日志核对（`3224/3224`）及 ClickHouse 5/5 链路抽查全部通过。
  - `[x]` 管理面与网络地址列实测：`mgmt` 口令登录、TOTP Step-up、`/auth/refresh` 续期、`/auth/logout` 吊销闭环通过；`mazbri_platform.ip_whitelist`（`inet` 列，CIDR 与单 IP 增查删）及 `mazbri_admin.public.audit_logs` 客户端 IP 真实库表读写验证通过。

### 2. `[x]` `release` 编译 runner 的 glibc 上限收敛（本地提交 `94a2003`）

- **完成内容**：
  - `[x]` `.github/workflows/release.yml` 的 `compile` 矩阵 runner 使用 `ubuntu-24.04` / `ubuntu-24.04-arm`（glibc 2.39 ≤ 运行镜像 `debian:trixie-slim` 的 glibc 2.41），非编译轻量 job 使用 `ubuntu-26.04`；本机 `dev-p50` 编译产出的全部 11 个二进制符号上限为 `GLIBC_2.38`。
  - `[x]` `actionlint` 对全量 `.github/workflows` 检查 0 告警 / 0 报错。
- **待远端复核子项**：
  - `[ ]` 下次推送触发 `release.yml` 发版时，确认双架构 `compile` job 日志中全部二进制 `max GLIBC_x.y ≤ 2.41` 且产出镜像健康启动。

### 3. `[x]` 推送后实测 CI / Release（PR #83 + `v0.6.0`）

- **完成内容**：
  - `[x]` `rust-toolchain.toml`（`1.100.0`）、Node `26.13.1`、pnpm `12.21.0` 及新版 GitHub Actions（`paths-filter` v4.0.3、`pnpm/action-setup` v6.1.1、`docker/login-action` v4.8.0、`rust-cache` v2.9.3）通过 GitHub CI 实测。
  - `[x]` `v0.6.0`（PR #84）完成 `linux/amd64` + `linux/arm64` 双架构镜像构建发版并通过部署验收。

### 4a. `[x]` `zip 9.8.1`（本地提交 `7ebb4ba`）

- **完成内容**：
  - `[x]` `mazbri-bin` 使用 `zip 9.8.1`（`default-features = false`，开启 `deflate-flate2`）。
  - `[x]` 固化 `ZIP_06_ARCHIVE_GOLDEN_HEX` 金标准单测，并完成运行态 `/config-center/export` 与 `/config-center/import` 8 份服务配置归档往返实测（标准 Deflate，`extract_version <= 20`，零错误导入）。

### 4b. `[x]` `rust-i18n 4.2.4`（本地提交 `643059d`）

- **完成内容**：
  - `[x]` `mazbri-libs` / `mazbri-i18n` 使用 `rust-i18n 4.2.4`（`serde-saphyr` 解析后端），`locales/{zh-CN,en-US}.yml` 键值结构、`%{seconds}` 占位符及 `build.rs` 重编译触发规则通过双语全键覆盖单测与运行态校验。

### 4c. `[x]` `jsonwebtoken 12.0.0` + `base64 0.23`（本地提交 `60082dc`）

- **完成内容**：
  - `[x]` `mazbri-libs` 与 `mazbri-bin` 统一锁定 `jsonwebtoken 12.0.0`（`aws_lc_rs` + `use_pem`，`pem 4.0.0`，移除 `signature 2.2`）与 `base64 0.23.1`。
  - `[x]` `mazbri-upstream-runtime`、`mazbri-mgmt`、`mazbri-gateway` 适配 `EncodingKey::from_secret` / `DecodingKey::from_secret` 空密钥返回 `Result` 的签名约束并补充空密钥拒绝单测。
  - `[x]` 运行态完成管理后台登录、TOTP Step-up、令牌刷新、登出吊销及网关 21 租户 Bearer 令牌签发校验回归。

### 5. `[x]` `cipher 0.5` 系列 + `aes-gcm`/`chacha20poly1305 0.11`（本地提交 `2f02875`）

- **完成内容**：
  - `[x]` `mazbri-crypto` 与 `mazbri-cert` 使用 `cipher 0.5.2`、`aes 0.9.4`、`sm4 0.6.0`、`des 0.9.0`、`cbc 0.2.1`、`ctr 0.10.1`、`cfb-mode 0.9.1`、`cfb8 0.9.1`、`ecb 0.2.1`、`aes-gcm 0.11.1`、`chacha20poly1305 0.11.0`（`aead 0.6.1`），双仓 `Cargo.lock` 零 `cipher 0.4` / `aead 0.5` 族残留。
  - `[x]` `mazbri-crypto/tests/interop/upgrade_golden.json` 与 `tests/vectors_upgrade_golden.rs` 锁定 24 组 AEAD 与 42 组对称/国密/legacy 升级金标准向量。
  - `[x]` 运行态通过 `cert-service` 存量加密许可证解封与 `x-loadgen` 全量 21 租户探测（含国密 `SM4-CBC`/`SM4-CTR`/`SM2` 及 legacy `AES-ECB`/`SM4-ECB`/`DES-CBC`）。

### 6. `[x]` 联调配套仓同步升级与热替换实测

- **完成内容**：
  - `[x]` `mazbri-max`（本地提交 `b5055bd`）：修复 `init-dbs --reset --force` 在伪终端下的交互阻塞并补单测；`init-dbs`、`mazbri-cert`、`mazbri-license-issuer`、`mazbri-argon2` 同步对齐 `sqlx 0.9`、`redis 1.8`、`reqwest 0.13.5`、`toml 1.2`、`rsasl 2.3`、`aes-gcm 0.11`、`hkdf 0.13`、`sha2 0.11`、`base64 0.23`、`argon2 0.6`。
  - `[x]` `upupt`（本地提交 `86ccb14`）：升级 `cipher 0.5`、`aes 0.9`、`aes-gcm 0.11`、`des 0.9`、`rsa 0.10`、`hmac 0.13`、`sha2 0.11`、`md-5 0.11`、`jsonwebtoken 12`、`rand 0.10`、`toml 1.2`、`base64 0.23`，守护进程热替换后 14 个协议 front 探测与网关调用全绿。
  - `[x]` `x-loadgen`（本地提交 `1696d4b`）：升级全套密码学原语、`argon2 0.6`、`getrandom 0.4`、`reqwest 0.13.5`、`base64 0.23`、`zeroize 1.9`，热替换 `x-loadgen-local:0.6.0` 容器后存量 Argon2id 凭据鉴权与全量压测通过。

---

## 二、 待上游发布事项

### 4d. `[ ]` 跟进 `rust-i18n-support` 移除传递依赖 `toml 0.8`

- **现状**：`rust-i18n` 为 `4.2.4`，上游 `rust-i18n-support 4.2.4` 直接依赖 `toml = "0.8.8"`，双仓 `Cargo.lock` 中 `toml 0.8.23` / `toml_edit 0.22.27` 与工作区主用的 `toml 1.2` / `toml_edit 0.25` 并存。
- **待办**：
  - `[ ]` 待上游 `rust-i18n-support` 升级至 `toml 1.x`（或将 `toml` 依赖设为可选 feature）后执行 `cargo update -p rust-i18n` 消除重复版本。
- **验证**：`mazbri-i18n` 中英双语全键测试通过；`cargo tree -d` 中无 `toml 0.8` 与 `toml_edit 0.22`。

### 6. `[ ]` 跟进 `rsa 0.10` 补丁版本

- **现状**：`mazbri-crypto`、`upupt`、`x-loadgen-crypto` 统一使用私库最新版 `rsa 0.10.0`。
- **待办**：
  - `[ ]` 持续关注 `0.10.x` 补丁版本与安全公告，上游发布后同步更新。
- **验证**：`mazbri-crypto` 升级金标准测试（`tests/vectors_upgrade_golden.rs`）与全量测试通过，`x-loadgen` RSA 绑定租户探测成功。

### 7. `[ ]` 删除 `.github/actionlint.yaml`

- **现状**：`actionlint` 当前版本 `v1.7.12` 内置 runner 标签表未收录 `ubuntu-26.04`，仓库保留 `.github/actionlint.yaml` 显式登记。
- **待办**：
  - `[ ]` 待 `actionlint` 新版本内置收录 `ubuntu-26.04` / `ubuntu-26.04-arm` 标签后删除 `.github/actionlint.yaml`。
- **验证**：删除后 `actionlint` 对全部 workflow 报告 0 个问题。
