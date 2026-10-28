# Changelog

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
