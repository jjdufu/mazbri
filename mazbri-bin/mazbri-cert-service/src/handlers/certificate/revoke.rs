use super::common::{CertificateState, extract_client_ip, update_certificate_inventory_metrics};
use crate::error::CertError;
use axum::{
    Json,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use mazbri_tls::CertMetricsRecorder;
use std::net::SocketAddr;
use uuid::Uuid;
pub async fn revoke_certificate(
    State(state): State<CertificateState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let metrics = CertMetricsRecorder::new("revoke");
    let _timer = metrics.start_timer();
    match revoke_certificate_inner(state.clone(), headers, Some(addr), id).await {
        Ok(response) => {
            metrics.record_revocation("manual");
            update_certificate_inventory_metrics(&state.repo).await;
            response
        }
        Err(e) => e.into_response(),
    }
}
async fn revoke_certificate_inner(
    state: CertificateState,
    headers: HeaderMap,
    socket_addr: Option<SocketAddr>,
    id: Uuid,
) -> Result<Response, CertError> {
    let actor_ip_str = extract_client_ip(&headers, socket_addr);
    if actor_ip_str.is_none() {
        return Err(CertError::Unauthorized(
            "无法识别请求者身份，需要可用的连接来源地址".into(),
        ));
    }
    let actor_ip = actor_ip_str.and_then(|ip| ip.parse::<std::net::IpAddr>().ok());
    let record = state.repo.revoke_certificate(id).await?;
    crate::audit::note_detail("subject", record.subject.clone());
    tracing::info!(
        subject = % record.subject, cert_id = % record.id, ip = ? actor_ip,
        "证书已作废"
    );
    Ok((
        StatusCode::OK,
        Json(serde_json::json!(
            { "message" : format!("证书 {} 已作废", id), "id" : id,
            "subject" : record.subject }
        )),
    )
        .into_response())
}
