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
- **Profile-based** transport seal/open (`TenantTransportProfile`, `mode=none|request|full`)
- Closed-set kebab-case algorithm ids (e.g. `aes-256-gcm`); legacy uppercase aliases are **rejected**
- Optional HTTP request binding (`X-Mazbri-*` HMAC headers) when `request_hmac_key` is set
- JSON request/response payloads
- Propagates standard HTTP client stack (`reqwest`)

## Quick start

```rust
use mazbri_core::crypto::{
    KeySelection, TenantTransportProfile, TransportMode, SUITE_TENANT_DEFAULT_2026_11,
};
use mazbri_sdk::{ClientConfig, MazbriClient};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut transport =
        TenantTransportProfile::from_named_suite(SUITE_TENANT_DEFAULT_2026_11, TransportMode::Full)?;
    transport.key = Some(KeySelection::Latest {
        key_id: "tenant-default".into(),
    });

    let client = MazbriClient::new(ClientConfig {
        gateway_base_url: "https://gateway.example.com".into(),
        tenant_id: "your-tenant-uuid".into(),
        api_key: std::env::var("MAZBRI_API_KEY")?,
        tenant_key: /* raw key bytes matching key_version */ vec![],
        key_version: "v1".into(),
        transport,
        request_hmac_key: None,
        request_key_id: None,
    })
    .await?;

    let _ = client;
    Ok(())
}
```

For plaintext (`mode=none`) use `TenantTransportProfile::plaintext()` and skip envelope seal/open.

## Auth & errors

- Prefer **API keys** (`mazbri_<prefix>_<secret>`) for machine clients.
- Token errors surface as `Error::Token`; HTTP/transport as `Error::Http` / `Error::Crypto`.
- Unsupported / legacy algorithms surface as `Error::UnsupportedAlgorithm` / `Error::LegacyDenied` (no silent aliases).
- Gateway JSON envelopes may include `request_id` — log it for support correlation (`x-request-id` / `traceparent` on the wire).

## Version strategy

1. Pin a crate version matching the gateway release train you deploy against.
2. After gateway transport or auth contract changes, bump SDK + publish release notes.
3. `some/mazbri-loadgen` is the reference consumer in this monorepo.

## Related

- Gateway conf: `mazbri-conf/mazbri-gateway.toml`
- Loadgen: `some/mazbri-loadgen`
- Core codecs / transport: `mazbri-core` (`mazbri-crypto` registry)
