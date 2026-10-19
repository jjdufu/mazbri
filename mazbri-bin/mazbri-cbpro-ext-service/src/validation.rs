use crate::cbpro_client::CBProClient;
use crate::db::{CbProExtRepository, ValidationJobRecord};
use anyhow::Result;
use mazbri_rpc::{
    FallbackRequest, StoreRequest, ValidationEnqueueRequest, ValidationResultRequest,
};
use serde_json::Value;
use sha2::Digest;
use std::sync::Arc;
use tracing::{debug, warn};

pub async fn process_validation_job(
    repo: Arc<CbProExtRepository>,
    cbpro_client: Option<Arc<CBProClient>>,
    job: ValidationJobRecord,
    timeout_secs: u64,
) -> Result<()> {
    let Some(cbpro_client) = cbpro_client else {
        let request = ValidationResultRequest {
            job_id: job.id.to_string(),
            cache_key: job.cache_key,
            tenant_id: job.tenant_id,
            product_id: job.product_id,
            method: job.method,
            path: job.path,
            success: false,
            compare_hash: None,
            response_status: None,
            message: Some("cbpro client unavailable".to_string()),
        };
        repo.mark_validation_result(&request, "failed").await?;
        return Ok(());
    };

    let fallback = cbpro_client
        .get_fallback(FallbackRequest {
            product_id: job.product_id.clone(),
            tenant_id: job.tenant_id.clone(),
            cache_key: job.cache_key.clone(),
            path: job.path.clone(),
            body: None,
        })
        .await;

    if !fallback.available {
        let request = ValidationResultRequest {
            job_id: job.id.to_string(),
            cache_key: job.cache_key,
            tenant_id: job.tenant_id,
            product_id: job.product_id,
            method: job.method,
            path: job.path,
            success: false,
            compare_hash: None,
            response_status: None,
            message: Some("snapshot missing".to_string()),
        };
        repo.mark_validation_result(&request, "failed").await?;
        return Ok(());
    }

    let data = fallback.data.unwrap_or(Value::Null);
    let store_response = cbpro_client
        .store_response(StoreRequest {
            product_id: job.product_id.clone(),
            tenant_id: job.tenant_id.clone(),
            cache_key: job.cache_key.clone(),
            data: data.clone(),
            ttl_secs: timeout_secs.max(300),
            tags: vec![job.method.clone(), job.path.clone()],
        })
        .await;
    if !store_response.success {
        warn!(
            cache_key = % job.cache_key,
            "Validation refresh store_response failed"
        );
    }
    let compare_hash = serde_json::to_vec(&data)
        .ok()
        .map(|bytes| hex::encode(sha2::Sha256::digest(bytes)));
    let request = ValidationResultRequest {
        job_id: job.id.to_string(),
        cache_key: job.cache_key,
        tenant_id: job.tenant_id,
        product_id: job.product_id,
        method: job.method,
        path: job.path,
        success: true,
        compare_hash: compare_hash.clone(),
        response_status: None,
        message: None,
    };
    repo.mark_validation_result(&request, "completed").await?;
    debug!(cache_key = % request.cache_key, "Validation job completed");
    Ok(())
}

pub fn build_validation_request(
    cache_key: impl Into<String>,
    tenant_id: impl Into<String>,
    product_id: impl Into<String>,
    method: impl Into<String>,
    path: impl Into<String>,
    reason: impl Into<String>,
) -> ValidationEnqueueRequest {
    ValidationEnqueueRequest {
        cache_key: cache_key.into(),
        tenant_id: tenant_id.into(),
        product_id: product_id.into(),
        method: method.into(),
        path: path.into(),
        reason: reason.into(),
        priority: 100,
        policy_version: None,
    }
}
