# 依赖与构建链升级待办与完成清单

截至 2026-11-21，`mazbri` 依赖与构建链升级全量台账（已完成事项以 `[x]` 标记并附提交记录，未完成或待运行态闭环事项以 `[ ]` 标记）。

## 总览矩阵

| 原编号 | 事项 | 当前状态 | 优先级 | 对应提交 / 版本 |
|---|---|---|---|---|
| 前置基线 | Rust `1.100.0`、Node `26.13.1` / pnpm `12.21.0`、RustCrypto `digest 0.11` 代、`rsa 0.10.0`、`p256`/`p384 0.14`、`argon2 0.6`、`tarpc 0.38`、`OpenTelemetry 0.32`、`redis 1.8`、`toml 1.2` | `[x]` 已完成 | 高 | PR #83 / `v0.6.0` |
| 1 | `sqlx 0.9.1` + `ipnetwork 0.21.1` | `[x]` 代码与单测已完成（`[ ]` 待运行态 `redep` 回归） | 高 | `b4da4e5`（本地已提交） |
| 2 | `release.yml` 编译 runner 的 glibc 上限收敛 | `[x]` 配置已完成（`[ ]` 待下次发版日志复核） | 高 | `94a2003`（本地已提交） |
| 3 | 推送后实测 CI / Release 与部署验证 | `[x]` 已完成 | 高 | PR #83、PR #84（`v0.6.0`） |
| 4a | 观望大版本：`zip 9`（`0.6.6` → `9.8.1`） | `[x]` 已完成 | 中 | `7ebb4ba`（本地已提交） |
| 4b | 观望大版本：`rust-i18n 4`（`3.1.5` → `4.2.4`） | `[x]` 主体已完成（`[ ]` 待上游去 `toml 0.8`） | 中 | `643059d`（本地已提交） |
| 4c | 观望大版本：`jsonwebtoken 12`（当前 `10.4.0`） | `[ ]` 未做（上游仍为 `12.0.0` 首版） | 中 | — |
| 5 | `cipher 0.4` 系列升 `0.5` 代（`aes`/`sm4`/`des`/模式 crate） | `[ ]` 未做（`sm4 0.6.0`、`des 0.9.0` 仍处首版） | 中 | — |
| 6 | 跟进 `rsa 0.10` 补丁版本 | `[ ]` 待跟进（上游最新仍为 `0.10.0`） | 低 | — |
| 7 | 删除 `.github/actionlint.yaml` | `[ ]` 未做（`actionlint v1.7.12` 未收录 `26.04`） | 低 | — |

---

## 一、 已完成事项（含待闭环的运行态/线上复核子项）

### 1. `[x]` `sqlx 0.9.1` + `ipnetwork 0.21.1`（本地提交 `b4da4e5`）

- **完成内容**：
  - `[x]` `mazbri-libs` 与 `mazbri-bin` 同步从 `sqlx 0.8.6` 升至 `0.9.1`，移除废弃的 `runtime-tokio-rustls`，切换为 `runtime-tokio` + `tls-rustls-aws-lc-rs`；`ipnetwork` 从 `0.20.0` 升至 `0.21.1`。
  - `[x]` 全仓动态 SQL 适配 `sqlx 0.9` 的 `SqlSafeStr` 静态约束（**零 `AssertSqlSafe` 绕过**）：静态列清单拼接统一改为 `macro_rules! + concat!` 编译期常量，动态 `WHERE` 过滤全面收敛为 `QueryBuilder + push_bind` 参数化绑定（复核全部 19 处 `QueryBuilder::push` 确保零外部输入拼入 SQL），顺手修复 `tenant_crypto::update_key_status` 重复赋值 `updated_at = NOW()` 的缺陷。
  - `[x]` 双仓 `Cargo.lock` 中由旧版 `sqlx 0.8` 带入的 `rsa 0.9.10`、`num-bigint-dig 0.8.6`、`hkdf 0.12.4`、`md-5 0.10.6` 以及 `der`/`pkcs1`/`pkcs8`/`spki` 旧族彻底清除，删除已无匹配包的 `[profile.dev.package.num-bigint-dig]`。
  - `[x]` 静态门禁与单测：前端 `pnpm build` 与 `*.test.ts` 通过；`mazbri-libs` 与 `mazbri-bin` 双仓 `cargo clippy --release --locked --workspace --all-targets -- -D warnings` 零警告；双仓 `cargo test --release --locked --workspace --no-fail-fast`（73 个套件 1154 项测试）全部通过，`ts-rs` 绑定零漂移。
- **待运行态闭环子项（本地提交尚未发版/热替换进运行容器）**：
  - `[ ]` 基于当前 `HEAD` 代码（容器热替换或新发版镜像）配合 `init-dbs` 全量初始化后，执行 `redep` / `x-loadgen` 全量跑批，核对调用日志条数与 ClickHouse 链路抽查；
  - `[ ]` 实测 `mgmt` 会话闭环（口令登录 + TOTP Step-up + 退出）及涉及 `ipnetwork` 列（IP 白名单、审计日志客户端 IP）的真实库表读写。

### 2. `[x]` `release` 编译 runner 的 glibc 上限收敛（本地提交 `94a2003`）

- **完成内容**：
  - `[x]` `.github/workflows/release.yml` 的 `compile` 矩阵 runner 已由 `ubuntu-26.04` / `ubuntu-26.04-arm`（glibc 2.43）回退至 `ubuntu-24.04` / `ubuntu-24.04-arm`（glibc 2.39 ≤ 运行镜像 `debian:trixie-slim` 的 glibc 2.41），其余非编译轻量 job 保留 `ubuntu-26.04`。
  - `[x]` `actionlint` 对全量 `.github/workflows` 检查 0 告警 / 0 报错。
- **待线上复核子项**：
  - `[ ]` 下次推送触发 `release.yml` 发版时，确认双架构 `compile` job 日志中全部二进制 `max GLIBC_x.y ≤ 2.41` 且产出镜像健康启动。

### 3. `[x]` 推送后实测 CI / Release（PR #83 + `v0.6.0`）

- **完成内容**：
  - `[x]` `rust-toolchain.toml`（`1.100.0`）、Node `26.13.1`、pnpm `12.21.0` 及新版 GitHub Actions（`paths-filter` v4.0.3、`pnpm/action-setup` v6.1.1、`docker/login-action` v4.8.0、`rust-cache` v2.9.3）已合入 `main` 并通过 GitHub CI 实测。
  - `[x]` `v0.6.0`（PR #84）已完成 `linux/amd64` + `linux/arm64` 双架构镜像构建发版并通过部署验收。

### 4a. `[x]` `zip` `0.6.6` → `9.8.1`（本地提交 `7ebb4ba`）

- **完成内容**：
  - `[x]` `mazbri-bin` 的 `zip` 由 `0.6.6` 升至 `9.8.1`（`default-features = false`，仅开 `deflate-flate2` 复用工作区 `flate2`）。
  - `[x]` 适配 `SimpleFileOptions::default()` 与 `ZipFile::name()` 返回 `Result` 的签名变更。
  - `[x]` 在 `mazbri-mgmt` 配置中心单测中固化 `ZIP_06_ARCHIVE_GOLDEN_HEX` 金标准，验证旧版 `0.6.6` 归档可原样导入，且 `9.8.1` 导出保持标准 Deflate（`version_needed <= 20`、`method == 8`）逐字节兼容。

### 4b. `[x]` `rust-i18n` `3.1.5` → `4.2.4`（本地提交 `643059d`）

- **完成内容**：
  - `[x]` `mazbri-libs` / `mazbri-i18n` 升级至 `rust-i18n 4.2.4`（YAML 解析器随上游由 `serde_yaml` 切换至 `serde-saphyr`）。
  - `[x]` 修复历史语言包 `locales/{zh-CN,en-US}.yml` 顶层多嵌一层语言前缀导致回退原始 key、以及 `api.rate_limit.exceeded` 占位符误写为 `{seconds}`（应为 `%{seconds}`）两处缺陷；补充 `build.rs` 的 `cargo:rerun-if-changed` 监听与双语全键覆盖单测。
- **遗留上游传递依赖尾项（归入下方待办 #4d）**：
  - `[ ]` 上游 `rust-i18n-support 4.2.4` 仍直接声明 `toml = "0.8.8"`，`Cargo.lock` 中的 `toml 0.8.23` / `toml_edit 0.22.27` 需待上游移除后清理。

---

## 二、 尚未完成的待办 / 观望事项

### 4c. `[ ]` 观望中的大版本：`jsonwebtoken 12`

- **现状**：`mazbri-libs` 与 `mazbri-bin` 当前锁定 `jsonwebtoken 10.4.0`（`aws_lc_rs` + `use_pem`；注：`mazbri-bin/Cargo.toml` 声明为 `"10.3.0"`）。核查私库索引，上游最新仍为首个大版本 `12.0.0`，尚无 `12.0.x` 补丁版。
- **要做**：
  - `[ ]` 待 `12.0.x` 补丁版本稳定、下游普遍跟进后单独提交升级，适配破坏性 API 与 feature 变更（同时统一双仓版本声明）。
- **影响 / 风险**：影响网关与管理后台 JWT 签发与校验链路。
- **验证**：现有令牌签发 / 校验测试全部通过，并完成管理后台登录与会话续期回归。

### 4d. `[ ]` 跟进 `rust-i18n-support` 移除传递依赖 `toml 0.8`

- **现状**：`rust-i18n` 已升至最新 `4.2.4`，但 `rust-i18n-support 4.2.4` 的 `Cargo.toml` 仍硬依赖 `toml = "0.8.8"`，导致双仓 `Cargo.lock` 中 `toml 0.8.23` / `toml_edit 0.22.27` 与工作区主用的 `toml 1.2` / `toml_edit 0.25` 并存。
- **要做**：
  - `[ ]` 待上游 `rust-i18n-support` 升级至 `toml 1.x`（或将 `toml` 依赖改为可选 feature）后执行 `cargo update -p rust-i18n` 消除重复版本。
- **验证**：`mazbri-i18n` 中英双语全键测试通过；`cargo tree -d` 中不再出现 `toml 0.8` 与 `toml_edit 0.22`。

### 5. `[ ]` `cipher 0.4` 系列升级至 `0.5` 代

- **现状**：仓库当前仍使用 `cipher 0.4.4`、`aes 0.8.4`、`sm4 0.5.1`、`des 0.8.1` 及配套 `cbc 0.1.2` / `ctr 0.9.2` / `cfb-mode 0.8.2` / `cfb8 0.8.1` / `ecb 0.1.2`，与已升到 `digest 0.11` 的 RustCrypto 组分属两代。核查私库最新索引：`cipher 0.5.2`、`aes 0.9.4`、`cbc 0.2.1`、`ctr 0.10.1`、`cfb-mode 0.9.1`、`cfb8 0.9.1`、`ecb 0.2.1` 均已发布补丁版本，**仅剩 `sm4 0.6.0` 与 `des 0.9.0` 仍处于 `.0` 首版**。
- **要做**：
  - `[ ]` 待 `sm4 0.6.x`、`des 0.9.x` 补丁版本稳定（或评估确认 `0.6.0` / `0.9.0` 满足金标准）后，将整套对称加解密与分组模式 crate 一次性整体升级。
- **影响 / 风险**：涉及租户传输信封、上游报文和落库信封的对称加解密，任何偏差都会导致既有密文无法解密。
- **验证**：先用旧版本固化金标准（各算法 / 模式 / 填充的固定密钥、IV、明文 → 密文），升级后原值断言通过；再跑 `x-loadgen` 全套件探测（含 legacy 的 `ecb` / `des` 租户）全部成功。

### 6. `[ ]` 跟进 `rsa 0.10` 补丁版本

- **现状**：`mazbri-crypto` 已使用 `rsa 0.10.0`（旧传递依赖 `rsa 0.9.10` 已在 `b4da4e5` 随 `sqlx 0.9` 升级彻底移除 `[x]`）。核查私库最新索引，`rsa` 当前最新仍为 `0.10.0`，尚无 `0.10.1` 补丁发布。
- **要做**：
  - `[ ]` 持续关注 `0.10.x` 补丁版本与安全公告，一有发布即跟进升级。
- **验证**：`mazbri-crypto` 的升级金标准测试（`tests/vectors_upgrade_golden.rs`）与全量测试通过，`x-loadgen` 的 RSA 绑定租户探测成功。

### 7. `[ ]` 删除 `.github/actionlint.yaml`

- **现状**：`actionlint` 最新正式版仍为 `v1.7.12`，内置 runner 标签表尚未收录 `ubuntu-26.04`（当前 `ci.yml`、`release-please.yml` 及 `release.yml` 轻量 job 仍使用 `ubuntu-26.04`），故保留 `.github/actionlint.yaml` 显式登记。
- **要做**：
  - `[ ]` 待 `actionlint` 新版本内置收录 `ubuntu-26.04` / `ubuntu-26.04-arm` 标签后删除 `.github/actionlint.yaml`。
- **验证**：删除后 `actionlint` 对全部 workflow 报告 0 个问题。
