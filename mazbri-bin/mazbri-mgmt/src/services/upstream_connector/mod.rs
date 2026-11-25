//! Upstream Connector Engine service: drafts, runs, publish, versions, import.

use crate::audit::{AuditAction, AuditLogRequest, AuditResourceType, AuditService};
use crate::error::ApiError;
use crate::repositories::upstream_connector as repo;
use crate::services::connector_metrics;
use crate::services::ssrf;
use mazbri_core::{SupplierConfig, UPSTREAM_TYPE_TRANSPARENT, mask_config_credentials};
use mazbri_upstream_runtime::{
    Assertion, AssertionResult, ExecuteMode, ExecuteOptions, InvokeInput, OutboundResponse,
    RuntimeEngine, SsrfChecker, evaluate_assertions,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;
use uuid::Uuid;

mod artifacts;
mod common;
mod draft;
mod import;
mod publish;
mod run;
mod types;
mod versions;
mod workbench_spec;

pub use artifacts::*;
pub use common::{
    build_connector_runtime, canonical_origin, endpoints_have_tls_to_seal,
    guard_channel_origin_pinning, guard_channel_tls_privilege, guard_draft_origin_pinning,
    guard_endpoints_tls_privilege, guard_supplier_origin_pinning, guard_supplier_tls_privilege,
    has_privileged_tls_material, reject_client_enc_in_channel_config,
    reject_client_enc_in_endpoints, reject_client_enc_in_supplier_config,
    reject_client_enc_in_workbench_env, supplier_config_has_secrets_to_seal,
};
pub(crate) use common::*;
pub use draft::*;
pub use import::*;
pub use publish::*;
pub use run::*;
pub use types::*;
pub use versions::*;
pub(crate) use workbench_spec::*;
