mod cert_client;
mod cert_manager;
mod config;
mod error;
pub mod metrics;
mod tarpc_transport;
mod transport;
pub use cert_client::{CertClient, RevokedSerialsResponse};
pub use cert_manager::{
    CertExpiryInfo, CertManager, ManagedCertHandle, ManagedCertificate, RenewalTaskHandle,
};
pub use config::TlsConfig;
pub use error::{TlsError, TlsResult};
pub use metrics::{
    CERT_ACTIVE_COUNT, CERT_ISSUANCE_TOTAL, CERT_MIN_EXPIRY_DAYS, CERT_OPERATION_DURATION_SECONDS,
    CERT_RENEWALS_TOTAL, CERT_REVOCATIONS_TOTAL, CertMetricsRecorder, register_cert_metrics,
};
pub use tarpc_transport::tls_connect_with_server_name;
pub use tarpc_transport::{TlsTarpcConnector, TlsTarpcListener, tls_connect, tls_listen};
pub use tokio_rustls::server::TlsStream as ServerTlsStream;
pub use transport::{
    ConnectTarget, DynamicCertResolver, TlsAcceptor, TlsConnector, normalize_serial_bytes,
    resolve_connect_target, verify_peer_san_acl,
};
