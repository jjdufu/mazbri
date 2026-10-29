# Changelog

## [0.2.10](https://github.com/jjdufu/mazbri/compare/v0.2.9...v0.2.10) (2026-10-29)


### Features

* **mfa:** 前端按六类提权拦截，并去掉顶栏倒计时 ([87a0b39](https://github.com/jjdufu/mazbri/commit/87a0b39dbd2eb96d42028c4d3e605355e5bd6290))
* **mfa:** 增加六类独立提权窗口的类型和 Redis 键 ([de90202](https://github.com/jjdufu/mazbri/commit/de902022d09e25e32452d7aa76359e7a2f3d230b))
* **mfa:** 把提权文案和审计展示改成六类各自计时 ([395d2ae](https://github.com/jjdufu/mazbri/commit/395d2ae16712493ce0b093d4717dabd80ea2ac2b))
* **mfa:** 敏感操作改为六类独立的 15 分钟窗口 ([99ac3ad](https://github.com/jjdufu/mazbri/commit/99ac3ad440f7744f66fd0cf1759140a475c5fb91))


### Bug Fixes

* **mfa:** 修六类提权验证里发现的缺口 ([9e835dd](https://github.com/jjdufu/mazbri/commit/9e835dda28a4205be431bb9546b7ac3dfeba82e2))
* **mfa:** 去掉已删除的全局 MFA 策略列依赖 ([c86a1e6](https://github.com/jjdufu/mazbri/commit/c86a1e6b14d883b958e57c394bed1f7e0765acc3))
* **mfa:** 绑定成功只启用开关，不再打开提权窗口 ([cfeabe2](https://github.com/jjdufu/mazbri/commit/cfeabe2fa92ff6323ef5f16e40dfa3a4b5af4edc))
* **mfa:** 补上 MfaStepUpDialog 的全局组件类型登记 ([7603731](https://github.com/jjdufu/mazbri/commit/7603731166a0bb7bd1d55d79378271b69690fa6e))

## [0.2.9](https://github.com/jjdufu/mazbri/compare/v0.2.8...v0.2.9) (2026-10-28)


### Bug Fixes

* **auth:** 阶段令牌写库失败可回滚，并隔离登录 401 ([89479db](https://github.com/jjdufu/mazbri/commit/89479dbb02c7623ab66396df06ae6c709e114766))
* **auth:** 阶段令牌写库失败可回滚，并隔离登录 401 ([43d47f2](https://github.com/jjdufu/mazbri/commit/43d47f2f32bc6536e21db22fd4d1e2603625f0b6))

## [0.2.8](https://github.com/jjdufu/mazbri/compare/v0.2.7...v0.2.8) (2026-10-28)


### Features

* **mfa:** 为提权窗口增加 bootstrap 时长并收紧锁定配置 ([4617b89](https://github.com/jjdufu/mazbri/commit/4617b89ac446a373081c5bf84f4298d6e3275851))
* **mfa:** 用会话提权窗口替换逐请求验证码 ([a4196d7](https://github.com/jjdufu/mazbri/commit/a4196d75e56ebd357b94cc510fbca6da6deb59d4))
* **mfa:** 用会话提权窗口替换逐请求验证码 ([4dea3d1](https://github.com/jjdufu/mazbri/commit/4dea3d143da9d2520e9bb48b928250b14856ef74))


### Bug Fixes

* **mfa:** 提权失败回滚窗口，启用关闭重签会话并补齐错误体 ([d4e7ac9](https://github.com/jjdufu/mazbri/commit/d4e7ac92c2537ca7d0a1c6a305af8e2ce0310b62))

## [0.2.7](https://github.com/jjdufu/mazbri/compare/v0.2.6...v0.2.7) (2026-10-27)


### Features

* **cert:** scoped service tokens, actor claims v2 (HMAC) and keyless recovery ([#20](https://github.com/jjdufu/mazbri/issues/20)) ([c000afa](https://github.com/jjdufu/mazbri/commit/c000afa9d92e9869ec1e9fa9c28234cd28a1bc9e))

## [0.2.6](https://github.com/jjdufu/mazbri/compare/v0.2.5...v0.2.6) (2026-10-26)


### Features

* **mgmt:** audit middleware, role model, MFA per-feature + unified cross-service audit ([#18](https://github.com/jjdufu/mazbri/issues/18)) ([99cee99](https://github.com/jjdufu/mazbri/commit/99cee9906ea3355c16c5258e6621f5d68ca88113))

## [0.2.5](https://github.com/jjdufu/mazbri/compare/v0.2.4...v0.2.5) (2026-10-26)


### Features

* **web:** flat sidebar, denser layout and live appearance preview ([#16](https://github.com/jjdufu/mazbri/issues/16)) ([98c0776](https://github.com/jjdufu/mazbri/commit/98c077688af361a0ea1be5984a0f8f0bb0c25658))

## [0.2.4](https://github.com/jjdufu/mazbri/compare/v0.2.3...v0.2.4) (2026-10-25)


### Features

* **ssrf:** add [ssrf].allow_private_network switch for bypass and mgmt ([#14](https://github.com/jjdufu/mazbri/issues/14)) ([88be850](https://github.com/jjdufu/mazbri/commit/88be85081f8ff950e6eae43772e77abef3ce28be))

## [0.2.3](https://github.com/jjdufu/mazbri/compare/v0.2.2...v0.2.3) (2026-10-24)


### Bug Fixes

* mTLS certs only for mTLS services; gateway /health decoupled from downstreams; probe 503 not logged as ERROR ([#11](https://github.com/jjdufu/mazbri/issues/11)) ([5e82d55](https://github.com/jjdufu/mazbri/commit/5e82d55adc2b59a076a5aa9c144289eb81b221bd))

## [0.2.2](https://github.com/jjdufu/mazbri/compare/v0.2.1...v0.2.2) (2026-10-24)


### Bug Fixes

* **analytics:** compute stats from request_logs FINAL, drop pre-aggregated MV reads ([#9](https://github.com/jjdufu/mazbri/issues/9)) ([ba2671f](https://github.com/jjdufu/mazbri/commit/ba2671f85f11dbbb0a5a8906ee6296a7ae40594f))

## [0.2.1](https://github.com/jjdufu/mazbri/compare/v0.2.0...v0.2.1) (2026-10-24)


### Features

* billing batch charge + gateway charge retry, L1/L3 reconciliation, perf fixes, ClickHouse log dedup ([#7](https://github.com/jjdufu/mazbri/issues/7)) ([1b88aa7](https://github.com/jjdufu/mazbri/commit/1b88aa7120a04d90a5b3b190f8610c9dba6dc4a6))

## [0.2.0](https://github.com/jjdufu/mazbri/compare/v0.1.0...v0.2.0) (2026-10-23)


### ⚠ BREAKING CHANGES

* remove Fury/payload codec (JSON only); delete execute/transform, billing_type, tenant status enum (→ enabled), BinaryEncoding; required egress fields and api:* scopes; strict supplier parsing; audit q-only + fixed export; reconciliation gateway-only; idempotency fail-closed on Redis error (503); deny_unknown_fields on owned inputs; pedantic lints clean with no allows.

### Features

* remove Fury codec and legacy compat layers ([#5](https://github.com/jjdufu/mazbri/issues/5)) ([2e25409](https://github.com/jjdufu/mazbri/commit/2e254099db8cbb3755b425bcc2ad253efc5535ae))

## [0.1.0](https://github.com/jjdufu/mazbri/compare/v0.0.1...v0.1.0) (2026-10-22)


### ⚠ BREAKING CHANGES

* service names are now prefixed (mgmt -> mazbri-mgmt, incl. HKDF context; admin DB must be rebuilt); RUNTIME_ENV, bootstrap port/ip, registration.port, presets, DatabaseConfig.url, unused tls fields and cbpro-ext `[clients.gateway]` are removed; every service requires `[cert_service] url` and configs reject unknown fields.

### Features

* unified service identity, single bootstrap parse, workspace pedantic lints ([#3](https://github.com/jjdufu/mazbri/issues/3)) ([31a19cb](https://github.com/jjdufu/mazbri/commit/31a19cb10db636f02a2f19375033cb9dc08c99c8))

## 0.0.1 (2026-10-20)


### Bug Fixes

* switch to path deps, clean strict clippy warnings, fix frontend type errors ([aed2808](https://github.com/jjdufu/mazbri/commit/aed28089ca430f33a545cf077ebc76ed558b1a96))
