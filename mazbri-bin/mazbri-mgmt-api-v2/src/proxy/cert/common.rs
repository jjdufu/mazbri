use super::*;

pub(crate) const MAX_PROXY_BODY_BYTES: usize = 1024 * 1024;
pub(crate) const PRESET_IMPORT_AUDIT_HEADER: &str = "x-mazbri-audit-action";
pub(crate) const PRESET_IMPORT_AUDIT_VALUE: &str = "preset-import";
#[derive(Clone)]
pub struct CertProxyConfig {
    pub cert_service_url: String,
    pub timeout_secs: u64,
    pub cert_service_token: Option<String>,
}
impl Default for CertProxyConfig {
    fn default() -> Self {
        Self {
            cert_service_url: String::new(),
            timeout_secs: 30,
            cert_service_token: None,
        }
    }
}
pub(crate) async fn require_license_issuance(state: &AppState) -> Result<(), ApiError> {
    ensure_license_issuance_allowed(
        &state.cert_http_client,
        &state.config.cert_service.url,
        state.config.cert_service.token.as_deref(),
    )
    .await
}

macro_rules! proxy_handler {
    (
        name = $name:ident, method = $method:expr, path = $path:literal, forbidden =
        $forbidden:expr,
    ) => {
        pub async fn $name(
            State(state): State<AppState>,
            claims: axum::Extension<AdminClaims>,
            ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
            request: Request,
        ) -> impl IntoResponse {
            if !can_manage_certificates(&claims) {
                return forbidden($forbidden);
            }
            let config = get_cert_proxy_config(&state);
            proxy_request(
                &state.cert_http_client,
                &config,
                request,
                $path,
                $method,
                Some(addr),
            )
            .await
        }
    };
    (
        name = $name:ident, method = $method:expr, path = $path:literal, path_param =
        $param_name:ident, forbidden = $forbidden:expr,
    ) => {
        pub async fn $name(
            State(state): State<AppState>,
            claims: axum::Extension<AdminClaims>,
            ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
            Path($param_name): Path<String>,
            request: Request,
        ) -> impl IntoResponse {
            if !can_manage_certificates(&claims) {
                return forbidden($forbidden);
            }
            let config = get_cert_proxy_config(&state);
            proxy_request(
                &state.cert_http_client,
                &config,
                request,
                &$path.replace("{}", &$param_name),
                $method,
                Some(addr),
            )
            .await
        }
    };
}
macro_rules! proxy_handler_with_log {
    (
        name = $name:ident, method = $method:expr, path = $path:literal, forbidden =
        $forbidden:expr, log = $log_msg:literal,
    ) => {
        pub async fn $name(
            State(state): State<AppState>,
            claims: axum::Extension<AdminClaims>,
            ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
            request: Request,
        ) -> impl IntoResponse {
            if !can_manage_certificates(&claims) {
                return forbidden($forbidden);
            }
            info!($log_msg, claims.sub);
            let config = get_cert_proxy_config(&state);
            proxy_request(
                &state.cert_http_client,
                &config,
                request,
                $path,
                $method,
                Some(addr),
            )
            .await
        }
    };
    (
        name = $name:ident, method = $method:expr, path = $path:literal, path_param =
        $param_name:ident, forbidden = $forbidden:expr, log = $log_msg:literal,
    ) => {
        pub async fn $name(
            State(state): State<AppState>,
            claims: axum::Extension<AdminClaims>,
            ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
            Path($param_name): Path<String>,
            request: Request,
        ) -> impl IntoResponse {
            if !can_manage_certificates(&claims) {
                return forbidden($forbidden);
            }
            info!($log_msg, claims.sub, $param_name);
            let config = get_cert_proxy_config(&state);
            proxy_request(
                &state.cert_http_client,
                &config,
                request,
                &$path.replace("{}", &$param_name),
                $method,
                Some(addr),
            )
            .await
        }
    };
}
macro_rules! proxy_handler_with_audit {
    (
        name = $name:ident, method = $method:expr, path = $path:literal, audit_action =
        $audit_action:expr, resource_id_fn = $resource_id_fn:expr, audit_details =
        $audit_details:expr, log = $log_msg:literal,
    ) => {
        pub async fn $name(
            State(state): State<AppState>,
            claims: axum::Extension<AdminClaims>,
            ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
            request: Request,
        ) -> Result<Response, ApiError> {
            if !can_manage_certificates(&claims) {
                return Err(ApiError::Forbidden);
            }
            info!($log_msg, claims.sub);
            require_license_issuance(&state).await?;
            let client_ip = extract_client_ip(request.headers(), Some(addr));
            let config = get_cert_proxy_config(&state);
            let response = proxy_request(
                &state.cert_http_client,
                &config,
                request,
                $path,
                $method,
                Some(addr),
            )
            .await;
            if response.status().is_success() {
                let audit_user_id = extract_user_id(&claims)?;
                state.audit.log(create_audit_log(
                    audit_user_id,
                    $audit_action,
                    AuditResourceType::Certificate,
                    $resource_id_fn,
                    $audit_details,
                    client_ip,
                ));
            }
            Ok(response)
        }
    };
    (
        name = $name:ident, method = $method:expr, path = $path:literal, path_param =
        $param_name:ident, audit_action = $audit_action:expr, resource_id_fn =
        $resource_id_fn:expr, audit_details = $audit_details:expr, log =
        $log_msg:literal,
    ) => {
        pub async fn $name(
            State(state): State<AppState>,
            claims: axum::Extension<AdminClaims>,
            ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
            Path($param_name): Path<String>,
            request: Request,
        ) -> Result<Response, ApiError> {
            if !can_manage_certificates(&claims) {
                return Err(ApiError::Forbidden);
            }
            info!($log_msg, claims.sub, $param_name);
            require_license_issuance(&state).await?;
            let client_ip = extract_client_ip(request.headers(), Some(addr));
            let config = get_cert_proxy_config(&state);
            let resolved_path = $path.replace("{}", &$param_name);
            let response = proxy_request(
                &state.cert_http_client,
                &config,
                request,
                &resolved_path,
                $method,
                Some(addr),
            )
            .await;
            if response.status().is_success() {
                let audit_user_id = extract_user_id(&claims)?;
                state.audit.log(create_audit_log(
                    audit_user_id,
                    $audit_action,
                    AuditResourceType::Certificate,
                    $resource_id_fn,
                    $audit_details,
                    client_ip,
                ));
            }
            Ok(response)
        }
    };
}

pub(crate) const FORBIDDEN_CERT: &str = "只有 admin 可以访问证书管理";
pub(crate) const FORBIDDEN_SVC: &str = "只有 admin 可以访问服务名称管理";
