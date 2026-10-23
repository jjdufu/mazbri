# Changelog

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
