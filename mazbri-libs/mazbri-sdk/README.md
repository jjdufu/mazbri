# mazbri-sdk

Official Rust client for tenant / partner integration with **mazbri-gateway**.

## Status

| Item | Policy |
|------|--------|
| **Audience** | Loadgen, internal tools, and external integrators who prefer Rust |
| **Versioning** | Follows workspace `version` (semver with other `mazbri-*` crates) |
| **Gateway contract** | Token: `POST /auth/token`. Invoke: `POST /api/v1/gateway` with JSON `GatewayRequest` or `TransportEnvelope`. Optional HMAC request-binding headers. |
| **Stability** | API surface is small (`MazbriClient`, `ClientConfig`); breaking changes bump minor/major with changelog |

## Features

- API-key → access token exchange
- Encrypted / signed transport envelopes (when tenant crypto profile requires it)
- Optional HTTP request binding (`X-Mazbri-*` HMAC-SHA256 headers) when `request_hmac_key` is set
- Request/response codecs via `mazbri_core::PayloadCodec` (JSON / Fury)
- Propagates standard HTTP client stack (`reqwest`)

## Quick start

```rust
use mazbri_sdk::{ClientConfig, MazbriClient, PayloadCodec};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = MazbriClient::new(ClientConfig {
        gateway_base_url: "https://gateway.example.com".into(),
        tenant_id: "your-tenant-uuid".into(),
        api_key: std::env::var("MAZBRI_API_KEY")?,
        tenant_key: /* 32-byte key from crypto export */ vec![],
        key_version: "1".into(),
        request_codec: PayloadCodec::Json,
        response_codec: PayloadCodec::Json,
        request_hmac_key: None,
        request_key_id: None,
    })
    .await?;

    // See MazbriClient methods for product call helpers / encrypted invoke.
    let _ = client;
    Ok(())
}
```

## Auth & errors

- Prefer **API keys** (`mazbri_<prefix>_<secret>`) for machine clients.
- Token errors surface as `Error::Token`; HTTP/transport as `Error::Http` / `Error::Crypto`.
- Gateway JSON envelopes may include `request_id` — log it for support correlation (`x-request-id` / `traceparent` on the wire).

## Version strategy

1. Pin a crate version matching the gateway release train you deploy against.
2. After gateway transport or auth contract changes, bump SDK + publish release notes.
3. `some/mazbri-loadgen` is the reference consumer in this monorepo.

## Related

- Gateway conf: `mazbri-conf/mazbri-gateway.toml`
- Loadgen: `some/mazbri-loadgen`
- Core codecs/crypto: `mazbri-core`
