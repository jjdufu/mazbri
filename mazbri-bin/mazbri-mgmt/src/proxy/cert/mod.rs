use crate::audit::{AuditAction, AuditResourceType, create_audit_log, extract_client_ip};
use crate::error::ApiError;
use crate::handlers::AppState;
use crate::middleware::auth::{AdminClaims, can_manage_certificates, extract_user_id};
use crate::services::license::ensure_license_issuance_allowed;
use axum::{
    Json, Router,
    extract::{ConnectInfo, Multipart, Path, Request, State},
    http::{HeaderMap, Method, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use reqwest::{
    Client,
    multipart::{Form as ReqwestMultipartForm, Part as ReqwestMultipartPart},
};
use serde_json::{Value, json};
use tracing::{error, info};

#[macro_use]
mod common;
mod authorization;
mod certificates;
mod client;
// Route builders reference handler fns expanded in authorization/certificates — load last.
mod routes;

use client::{
    enrich_audit_details, forbidden, get_cert_proxy_config, parse_json_body, proxy_request,
};
pub(crate) use common::*;
// Handlers must be in parent namespace for `routes` (`use super::*` + macro-expanded names).
pub(crate) use authorization::*;
pub(crate) use certificates::*;

pub use common::CertProxyConfig;
pub use routes::create_cert_proxy_routes;
