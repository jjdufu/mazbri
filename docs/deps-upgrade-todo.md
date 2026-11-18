# 依赖与构建链升级待办

截至 2026-11-18，`dev/chain-fix` 分支尚未完成的依赖与构建链事项。工具链、RustCrypto 组、rsa、p256/p384、argon2、tarpc、OpenTelemetry、redis、toml 已升级，不在此列。

| # | 事项 | 优先级 |
|---|---|---|
| 1 | sqlx 0.9 + ipnetwork 0.21 | 高 |
| 2 | release 编译 runner 的 glibc 上限 | 高（发版前定） |
| 3 | 推送后实测 CI / Release | 高（首次推送时） |
| 4 | 观望中的大版本：jsonwebtoken 12、zip 9、rust-i18n 4 | 中 |
| 5 | cipher 0.4 系列 | 中 |
| 6 | 跟进 rsa 0.10 补丁版本 | 低 |
| 7 | 删除 `.github/actionlint.yaml` | 低 |

## 1. sqlx 0.9 + ipnetwork 0.21

- **现状**：
  - `sqlx 0.8.6`（libs 和 bin 的 workspace 依赖都开了 `runtime-tokio-rustls` feature），`ipnetwork 0.20.0`。
  - `digest 0.10` 仍在依赖树里，其中 hmac 0.12、hkdf 0.12、md-5 0.10、sha2 0.10 由 `sqlx-postgres 0.8` 带入。
- **要做**：
  - libs 和 bin 一起升到 sqlx 0.9、ipnetwork 0.21。
  - 删除 `runtime-tokio-rustls`，改用 0.9 拆分后的 runtime / TLS feature。
  - 动态 SQL 适配 0.9 的 SQL 安全约束：约 163 个文件，约 128 处 `QueryBuilder` 或拼接 SQL。必须拼接的地方用 `AssertSqlSafe` 显式标注，并逐处确认没有外部输入进入 SQL 文本。
  - `query!` / `query_as!` 宏及类型映射按 0.9 的变更调整（`ipnetwork` 列、`Decimal`、`chrono`、`uuid`、`json`）。
- **影响 / 风险**：
  - 涉及全部直连 PG 的服务，是目前改动面最大的一项。
  - 动态 SQL 改错的后果是查询结果错误或注入风险，不只是编译失败。
  - 升级后 `digest 0.10` 不会完全消失：`rskafka 0.6 → rsasl` 引入 hmac/sha2/pbkdf2 的 0.10 版，axum ws 引入 `sha1 0.10`。
- **验证**：
  - libs 和 bin 的 `clippy --release --locked --all-targets -D warnings` 及全量 `cargo test` 通过。
  - init-dbs 全量初始化后，跑 x-loadgen 请求，核对调用日志条数与链路抽查。
  - 跑 mgmt 会话测试（口令 + TOTP step-up + 退出）。
  - 涉及 ipnetwork 的列（白名单、审计 IP）做读写抽查。
  - `cargo tree -d` 中 sqlx 带进来的 0.10 那一代消失。

## 2. release 编译 runner 的 glibc 上限

- **现状**：
  - 运行镜像是 `debian:trixie-slim`（glibc 2.41）。
  - release.yml 的 compile job 已改到 `ubuntu-26.04` / `ubuntu-26.04-arm`，这两个 runner 是 glibc 2.43。
  - 在 trixie（glibc 2.41）上编译的现有二进制，引用的最高符号版本是 `GLIBC_2.38`（`__isoc23_sscanf`）。
  - compile job 的 “Collect binaries” 步骤会用 `objdump -T` 检查，任一二进制需要高于 `GLIBC_2.41` 就失败。
- **要做**：发版前二选一：
  - compile job 改回 `ubuntu-24.04` / `ubuntu-24.04-arm`（glibc 2.39）；
  - 或者在 `debian:trixie` 容器里编译，使编译机和运行镜像的 glibc 一致。
- **影响 / 风险**：
  - 如果不处理，在 glibc 2.43 上编译的 C 依赖（aws-lc-sys、ring、zstd 等）可能引用 2.42 / 2.43 的符号。
  - 守卫会拦住，所以不会推出无法启动的镜像，但发版流程会中断。
- **验证**：compile job 日志里每个二进制的 `max GLIBC_x.y` ≤ 2.41；image job 推送的镜像能在 trixie 上启动并通过健康检查。

## 3. 推送后实测 CI / Release

- **现状**：以下改动只做了静态检查（actionlint、YAML 解析）和本地等价验证，没有在 GitHub 上跑过：
  - 工具链改由 `rust-toolchain.toml` 提供；
  - runner 换成 `ubuntu-26.04` / `ubuntu-26.04-arm`；
  - Node 26.13.1、pnpm 12.21.0；
  - `dorny/paths-filter` v4.0.3、`pnpm/action-setup` v6.1.1、`docker/login-action` v4.8.0、`Swatinem/rust-cache` v2.9.3。
- **要做**：首次推送后逐项确认：
  - 各 job 的 `rustc -vV` 是 1.100.0；
  - plan job 的 `cargo metadata` 能自动安装 toolchain 文件指定的版本（runner 自带 1.99.1、rustup 1.29.3）；
  - pnpm 缓存命中；
  - paths-filter 输出与预期一致；
  - compile / image / retag 全绿。
- **影响 / 风险**：ubuntu-26.04 镜像有未关闭的问题：
  - [runner-images#15233](https://github.com/actions/runner-images/issues/15233)：Docker 构建磁盘不足；
  - [runner-images#15343](https://github.com/actions/runner-images/issues/15343)：CPU / 内存密集步骤卡死、不响应超时取消。
  - compile job（release 全量编译）和 image job 最容易受影响。
- **验证**：CI 与 Release 运行全部成功；release 产出的 `linux/amd64` + `linux/arm64` 镜像在部署环境健康。

## 4. 观望中的大版本

| crate | 现状 | 目标 | 说明 |
|---|---|---|---|
| jsonwebtoken | 10.4.0（`aws_lc_rs` + `use_pem`） | 12 | API 与 feature 有破坏性变更；网关 JWT 签发与校验要回归 |
| zip | 0.6.6（mgmt，仅 `deflate`） | 9 | 跨多个大版本，读写 API 有变；mgmt 导入导出要回归 |
| rust-i18n | 3.1.5（mazbri-i18n → mgmt） | 4 | 升级后可去掉 `rust-i18n-support` 带入的 `toml 0.8`，依赖树里只剩 `toml 1.x` |

- **要做**：等各自的新大版本稳定（至少出过一个补丁版本，下游普遍跟进）后逐个升级，每个单独提交。
- **影响 / 风险**：jsonwebtoken 影响鉴权链路；zip 影响 mgmt 文件导入导出；rust-i18n 影响 mgmt 文案加载。
- **验证**：
  - jsonwebtoken：现有令牌签发 / 校验测试通过，并做一次 mgmt 登录。
  - zip：导入导出文件与升级前逐字节一致，或能被旧版本解析。
  - rust-i18n：中英文案键全覆盖；`cargo tree -d` 中没有 `toml 0.8`。

## 5. cipher 0.4 系列

- **现状**：`cipher 0.4.4`，`aes 0.8.4`、`sm4 0.5.1`、`des 0.8.1` 以及配套的 cbc / ctr / cfb / ecb 模式 crate 都还是 0.4 这一代，和已升到 digest 0.11 的 RustCrypto 组分属两代。
- **要做**：对应的新一代稳定后（cipher 0.5 及配套的 aes / sm4 / des / 模式 crate），整体一起升级。
- **影响 / 风险**：涉及租户传输信封、上游报文和落库信封的对称加解密，任何偏差都会让已有密文无法解密。
- **验证**：先用旧版本固化金标准（各算法 / 模式 / 填充的固定密钥、IV、明文 → 密文），升级后原值断言通过；再跑 x-loadgen 全套件探测（含 legacy 的 ecb / des 租户），全部成功。

## 6. 跟进 rsa 0.10 补丁版本

- **现状**：`mazbri-crypto` 使用 `rsa 0.10.0`，这是 0.10 系列的首个正式版本。
- **要做**：关注 0.10.x 补丁版本和安全公告，有发布就跟进。
- **影响 / 风险**：首个版本可能有缺陷；涉及 RSA 签名、OAEP / PKCS#1 v1.5 加解密和 legacy RSA。
- **验证**：`mazbri-crypto` 的升级金标准测试（`tests/vectors_upgrade_golden.rs`）与全量测试通过，x-loadgen 的 RSA 绑定租户探测成功。

## 7. 删除 `.github/actionlint.yaml`

- **现状**：actionlint 最新版 v1.7.12 的内置 runner 标签表里没有 `ubuntu-26.04` / `ubuntu-26.04-arm`，所以在 `.github/actionlint.yaml` 中显式登记了这两个标签。
- **要做**：actionlint 新版本收录这两个标签后，删除该文件。
- **影响 / 风险**：无运行时影响；提前删会导致 actionlint 报 `runner-label` 错误。
- **验证**：删除后 `actionlint` 对全部 workflow 报 0 个问题。
