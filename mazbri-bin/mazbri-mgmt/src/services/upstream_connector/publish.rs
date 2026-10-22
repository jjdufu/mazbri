use super::{
    ApiError, AuditAction, AuditLogRequest, AuditResourceType, AuditService, Decimal,
    DraftGateItem, FromStr, GATE_REASON_SKIPPED, GATE_TTL_HOURS, PgPool, PublishChannel,
    PublishRequest, PublishResult, RecentGateRun, Row, SupplierConfig, Uuid, apply_env_str,
    apply_env_to_supplier_config, build_workbench_spec, channel_config_with_egress,
    connector_metrics, endpoint_by_id, evaluate_endpoint_gates, info, mask_config_credentials,
    parse_workbench_env, repo, ssrf,
};
use mazbri_core::num::SaturatingFrom;

// ---------- Publish ----------

/// `publish_draft` 的入参。
pub struct PublishDraftParams<'a> {
    pub audit: &'a AuditService,
    pub id: Uuid,
    pub req: PublishRequest,
    pub actor: &'a str,
    pub user_id: Uuid,
    pub is_admin: bool,
    pub client_ip: Option<String>,
}

/// # Errors
///
/// 底层依赖（数据库、网络、序列化或参数校验）失败时返回错误。
pub async fn publish_draft(
    pool: &PgPool,
    params: PublishDraftParams<'_>,
) -> Result<PublishResult, ApiError> {
    let PublishDraftParams {
        audit,
        id,
        req,
        actor,
        user_id,
        is_admin,
        client_ip,
    } = params;
    let draft = repo::get_draft(pool, id)
        .await
        .map_err(|e| ApiError::internal(format!("get draft: {e}")))?
        .ok_or_else(|| ApiError::not_found("draft".into(), id.to_string()))?;

    let mut cfg = SupplierConfig::parse_and_validate(&draft.config)
        .map_err(|e| ApiError::validation(format!("CONFIG_INVALID: {e}")))?;
    cfg = cfg.normalize_disabled_sections();
    cfg.validate_upstream_url_for_type(&draft.upstream_url)
        .map_err(|e| ApiError::validation(format!("CONFIG_INVALID: {e}")))?;

    let env = parse_workbench_env(&draft.workbench_env);
    let mut publish_cfg = cfg.clone();
    apply_env_to_supplier_config(&mut publish_cfg, &env);
    let expanded_url = apply_env_str(&draft.upstream_url, &env);

    let recent = repo::list_recent_gate_runs(pool, id, GATE_TTL_HOURS)
        .await
        .map_err(|e| ApiError::internal(format!("gate check: {e}")))?;
    let recent: Vec<RecentGateRun> = recent
        .into_iter()
        .map(|r| RecentGateRun {
            endpoint_key: r.endpoint_key,
            spec_fingerprint: r.spec_fingerprint,
        })
        .collect();
    let eval = evaluate_endpoint_gates(
        &draft.endpoints,
        &env,
        Some(&cfg),
        &draft.upstream_url,
        &recent,
    );
    if !eval.leftover.is_empty() {
        return Err(ApiError::validation(format!(
            "UNRESOLVED_VARS: {}",
            eval.leftover.join(", ")
        )));
    }

    if publish_cfg.is_transparent() {
        ssrf::validate_public_http_url(&expanded_url)
            .await
            .map_err(|e| ApiError::validation(format!("SSRF_BLOCKED: {e}")))?;
    }

    check_publish_gates(&eval.items, &req, is_admin)?;

    // Credentials stay in encrypted slots; workbench {{vars}} in auth/transform
    // become literals. workbench_env is never copied into suppliers.config.
    let config_value = publish_cfg
        .to_json_value()
        .map_err(|e| ApiError::validation(e.to_string()))?;

    let supplier_code = req.supplier.supplier_code.trim();
    let name = req.supplier.name.trim();
    if supplier_code.is_empty() || name.is_empty() {
        return Err(ApiError::validation(
            "supplier.supplier_code / name 不能为空",
        ));
    }

    let mut warnings: Vec<String> = Vec::new();

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| ApiError::internal(format!("begin tx: {e}")))?;

    let supplier_id = upsert_supplier(
        &mut tx,
        supplier_code,
        name,
        &expanded_url,
        &req.supplier.status,
        &config_value,
    )
    .await?;

    let ctx = ChannelBuildCtx {
        endpoints: &draft.endpoints,
        env: &env,
        cfg: &publish_cfg,
        upstream_url: &expanded_url,
    };
    let (channel_ids, channels_snapshot) =
        publish_channels(&mut tx, supplier_id, &req, &ctx).await?;

    let (route_rule_id, binding_ids) = if req.mode == "full_onboarding" {
        let (rid, bids) = publish_route(&mut tx, id, &req, &channel_ids).await?;
        (Some(rid), bids)
    } else {
        (None, Vec::new())
    };

    let (version_id, version_no) = record_publish_version(
        &mut tx,
        &draft,
        supplier_id,
        &config_value,
        serde_json::Value::Array(channels_snapshot),
        &req,
        actor,
    )
    .await?;

    tx.commit()
        .await
        .map_err(|e| ApiError::internal(format!("commit: {e}")))?;

    let route_version: i64 = sqlx::query_scalar(
        "SELECT version FROM route_sync_metadata WHERE key = 'global_route_version'",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    if req.options.force.force_publish {
        warnings.push(format!(
            "force_publish: {}",
            req.options.force.force_reason.clone().unwrap_or_default()
        ));
    }

    audit.log(AuditLogRequest {
        user_id,
        action: if req.options.force.force_publish {
            AuditAction::ConnectorForcePublish
        } else {
            AuditAction::ConnectorPublish
        },
        resource_type: AuditResourceType::Supplier,
        resource_id: Some(supplier_id.to_string()),
        details: Some(serde_json::json!({
            "draft_id": id,
            "supplier_id": supplier_id,
            "channel_ids": channel_ids,
            "version_id": version_id,
            "version_no": version_no,
            "mode": req.mode,
            "route_version": route_version,
            "force": req.options.force.force_publish,
            "force_reason": req.options.force.force_reason,
        })),
        ip_address: client_ip,
        new_value: Some(serde_json::json!({
            "supplier_code": supplier_code,
            "config": mask_config_credentials(config_value),
        })),
        ..Default::default()
    });

    info!(
        supplier_id = %supplier_id,
        version_no,
        "Connector publish completed"
    );

    let sample_curl = Some(
        "curl -sS -X POST 'https://<gateway-host>/api/v1/gateway' \\\n  -H 'Authorization: Bearer <access_token>' \\\n  -H 'Content-Type: application/json' \\\n  -d '{\"product_id\":\"<product_id>\",\"method\":\"POST\",\"path\":\"<path>\",\"body\":{}}'".to_string(),
    );

    connector_metrics::record_publish(&req.mode, req.options.force.force_publish, true);

    Ok(PublishResult {
        supplier_id,
        channel_ids,
        route_rule_id,
        binding_ids,
        route_version: Some(route_version),
        version_id,
        version_no,
        warnings,
        sample_curl,
    })
}

pub(super) fn auto_channels_from_endpoints(endpoints: &serde_json::Value) -> Vec<PublishChannel> {
    let Some(arr) = endpoints.as_array() else {
        return vec![PublishChannel {
            endpoint_id: None,
            channel_code: "DEFAULT".into(),
            name: "default".into(),
            path_prefix: "/".into(),
            cost_price: None,
            qps_limit: Some(0),
        }];
    };
    if arr.is_empty() {
        return vec![PublishChannel {
            endpoint_id: None,
            channel_code: "DEFAULT".into(),
            name: "default".into(),
            path_prefix: "/".into(),
            cost_price: None,
            qps_limit: Some(0),
        }];
    }
    arr.iter()
        .enumerate()
        .map(|(i, ep)| {
            let id = ep
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let name = ep
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("endpoint")
                .to_string();
            let path = ep
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("/")
                .to_string();
            let code = ep.get("channel_code").and_then(|v| v.as_str()).map_or_else(
                || {
                    let base: String = name
                        .chars()
                        .filter(char::is_ascii_alphanumeric)
                        .collect::<String>()
                        .to_uppercase();
                    if base.is_empty() {
                        format!("EP{}", i + 1)
                    } else {
                        base.chars().take(20).collect()
                    }
                },
                std::string::ToString::to_string,
            );
            let path_prefix = ep
                .get("path_prefix")
                .and_then(|v| v.as_str())
                .unwrap_or(path.as_str())
                .to_string();
            PublishChannel {
                endpoint_id: if id.is_empty() { None } else { Some(id) },
                channel_code: code,
                name,
                path_prefix,
                cost_price: None,
                qps_limit: Some(0),
            }
        })
        .collect()
}

/// 构建渠道配置所需的上下文。
struct ChannelBuildCtx<'a> {
    endpoints: &'a serde_json::Value,
    env: &'a std::collections::HashMap<String, String>,
    cfg: &'a SupplierConfig,
    upstream_url: &'a str,
}

fn check_publish_gates(
    items: &[DraftGateItem],
    req: &PublishRequest,
    is_admin: bool,
) -> Result<(), ApiError> {
    if req.options.require_gate {
        let missing: Vec<&DraftGateItem> = items
            .iter()
            .filter(|i| i.reason != GATE_REASON_SKIPPED && !i.met)
            .collect();
        if !missing.is_empty() {
            if req.options.force.force_publish {
                if !is_admin {
                    return Err(ApiError::Forbidden);
                }
                if req
                    .options
                    .force
                    .force_reason
                    .as_deref()
                    .map(str::trim)
                    .as_ref()
                    .is_none_or(|s| s.is_empty())
                {
                    return Err(ApiError::validation("force_publish 需要 force_reason"));
                }
            } else {
                let detail = missing
                    .iter()
                    .map(|i| format!("{} {}", i.endpoint_key, i.reason))
                    .collect::<Vec<_>>()
                    .join("; ");
                return Err(ApiError::validation(format!("GATE_NOT_MET: {detail}")));
            }
        }
    }

    Ok(())
}

async fn upsert_supplier(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    supplier_code: &str,
    name: &str,
    expanded_url: &str,
    status: &str,
    config_value: &serde_json::Value,
) -> Result<Uuid, ApiError> {
    // Upsert supplier by code
    let existing_id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM suppliers WHERE supplier_code = $1")
            .bind(supplier_code)
            .fetch_optional(&mut **tx)
            .await
            .map_err(|e| ApiError::internal(format!("lookup supplier: {e}")))?;

    let supplier_id: Uuid = if let Some(sid) = existing_id {
        sqlx::query(
            r"
            UPDATE suppliers SET
                name = $2, upstream_url = $3,
                status = $4, config = $5, updated_at = NOW()
            WHERE id = $1
            ",
        )
        .bind(sid)
        .bind(name)
        .bind(expanded_url)
        .bind(status)
        .bind(config_value)
        .execute(&mut **tx)
        .await
        .map_err(|e| ApiError::internal(format!("update supplier: {e}")))?;
        sid
    } else {
        sqlx::query_scalar(
            r"
            INSERT INTO suppliers (supplier_code, name, upstream_url, status, config)
            VALUES ($1,$2,$3,$4,$5) RETURNING id
            ",
        )
        .bind(supplier_code)
        .bind(name)
        .bind(expanded_url)
        .bind(status)
        .bind(config_value)
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| ApiError::internal(format!("insert supplier: {e}")))?
    };

    Ok(supplier_id)
}

/// 按发布模式写入渠道；返回渠道 id 与快照。
async fn publish_channels(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    supplier_id: Uuid,
    req: &PublishRequest,
    ctx: &ChannelBuildCtx<'_>,
) -> Result<(Vec<Uuid>, Vec<serde_json::Value>), ApiError> {
    let mut channel_ids = Vec::new();
    let mut channels_snapshot = Vec::new();
    if req.mode == "supplier_channels" || req.mode == "full_onboarding" {
        let channels = if req.channels.is_empty() {
            auto_channels_from_endpoints(ctx.endpoints)
        } else {
            req.channels.clone()
        };
        for ch in &channels {
            let (cid, snapshot) = publish_channel(tx, supplier_id, ch, ctx).await?;
            channel_ids.push(cid);
            channels_snapshot.push(snapshot);
        }
    }
    Ok((channel_ids, channels_snapshot))
}

/// 校验并写入单个渠道；返回渠道 id 与快照。
async fn publish_channel(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    supplier_id: Uuid,
    ch: &PublishChannel,
    ctx: &ChannelBuildCtx<'_>,
) -> Result<(Uuid, serde_json::Value), ApiError> {
    let channel_code = ch.channel_code.trim();
    if channel_code.is_empty() {
        return Err(ApiError::validation("channel_code 不能为空"));
    }
    let cost = match &ch.cost_price {
        Some(s) => Decimal::from_str(s.trim())
            .map_err(|e| ApiError::validation(format!("cost_price: {e}")))?,
        None => Decimal::ZERO,
    };
    let qps = ch.qps_limit.unwrap_or(0);
    let path_prefix = if ch.path_prefix.trim().is_empty() {
        "/"
    } else {
        ch.path_prefix.trim()
    };
    let ep = endpoint_by_id(ctx.endpoints, ch.endpoint_id.as_deref());
    let empty_ep = serde_json::json!({});
    let built = build_workbench_spec(
        ep.unwrap_or(&empty_ep),
        ctx.env,
        Some(ctx.cfg),
        path_prefix,
        ctx.upstream_url,
    )?;
    let egress = built.spec;
    if ctx.cfg.is_transparent() {
        mazbri_core::validate_egress_spec(&egress)
            .map_err(|e| ApiError::validation(format!("CONFIG_INVALID: {e}")))?;
        if let Some(ref url) = egress.base_url {
            ssrf::validate_public_http_url(url)
                .await
                .map_err(|e| ApiError::validation(format!("SSRF_BLOCKED: {e}")))?;
        }
    }
    let channel_config = channel_config_with_egress(&serde_json::json!({}), &egress)?;

    let existing_ch: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM channels WHERE channel_code = $1")
            .bind(channel_code)
            .fetch_optional(&mut **tx)
            .await
            .map_err(|e| ApiError::internal(format!("lookup channel: {e}")))?;

    let cid = if let Some(id) = existing_ch {
        sqlx::query(
            r"
            UPDATE channels SET
                supplier_id = $2, name = $3, path_prefix = $4,
                cost_price = $5, qps_limit = $6, config = $7,
                status = 'active', updated_at = NOW()
            WHERE id = $1
            ",
        )
        .bind(id)
        .bind(supplier_id)
        .bind(&ch.name)
        .bind(path_prefix)
        .bind(cost)
        .bind(qps)
        .bind(&channel_config)
        .execute(&mut **tx)
        .await
        .map_err(|e| ApiError::internal(format!("update channel: {e}")))?;
        id
    } else {
        sqlx::query_scalar(
            r"
            INSERT INTO channels
                (supplier_id, channel_code, name, path_prefix, cost_price, qps_limit,
                 config, status)
            VALUES ($1,$2,$3,$4,$5,$6,$7,'active')
            RETURNING id
            ",
        )
        .bind(supplier_id)
        .bind(channel_code)
        .bind(&ch.name)
        .bind(path_prefix)
        .bind(cost)
        .bind(qps)
        .bind(&channel_config)
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| ApiError::internal(format!("insert channel: {e}")))?
    };
    let snapshot = serde_json::json!({
        "channel_id": cid,
        "channel_code": channel_code,
        "name": ch.name,
        "path_prefix": path_prefix,
        "endpoint_id": ch.endpoint_id,
    });
    Ok((cid, snapshot))
}

/// `full_onboarding` 模式：创建路由规则、渠道绑定与租户授权。
async fn publish_route(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    draft_id: Uuid,
    req: &PublishRequest,
    channel_ids: &[Uuid],
) -> Result<(Uuid, Vec<Uuid>), ApiError> {
    let route = req
        .route
        .as_ref()
        .ok_or_else(|| ApiError::validation("full_onboarding 需要 route 字段"))?;
    let (product_uuid, product_external_id) =
        resolve_route_product(tx, route, req.options.fail_if_route_exists).await?;
    let mut binding_ids = Vec::new();
    if channel_ids.is_empty() {
        return Err(ApiError::validation("full_onboarding 需要至少一个 channel"));
    }

    let metadata = serde_json::json!({
        "source": "upstream_connector",
        "product_external_id": product_external_id,
        "draft_id": draft_id,
    });
    let rid: Uuid = sqlx::query_scalar(
        r"
        INSERT INTO route_rules (tenant_id, product_id, priority, status, metadata)
        VALUES ($1,$2,$3,'active',$4) RETURNING id
        ",
    )
    .bind(route.tenant_id)
    .bind(product_uuid)
    .bind(route.priority)
    .bind(&metadata)
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| ApiError::internal(format!("insert route_rule: {e}")))?;

    let switch_condition = serde_json::json!({
        "max_retries": 1,
        "timeout_ms": 3000,
        "switch_on_timeout": true,
        "switch_on_bad_gateway": true,
        "switch_on_internal_error": false,
        "switch_on_status_codes": []
    });

    for (i, cid) in channel_ids.iter().enumerate() {
        let bid: Uuid = sqlx::query_scalar(
            r"
            INSERT INTO route_channel_bindings
                (route_rule_id, channel_id, channel_order, switch_condition, version_tag, weight)
            VALUES ($1,$2,$3,$4,$5,$6) RETURNING id
            ",
        )
        .bind(rid)
        .bind(cid)
        .bind(i32::saturating_from(i + 1))
        .bind(&switch_condition)
        .bind(&route.version_tag)
        .bind(route.weight)
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| ApiError::internal(format!("insert binding: {e}")))?;
        binding_ids.push(bid);
    }

    if let Some(ref auth) = req.authorization {
        if auth.enabled {
            for tid in &auth.tenant_ids {
                sqlx::query(
                    r"
                    INSERT INTO tenant_products (tenant_id, product_id, enabled, custom_config)
                    VALUES ($1,$2,true,'{}'::jsonb)
                    ON CONFLICT (tenant_id, product_id)
                    DO UPDATE SET enabled = true, updated_at = NOW()
                    ",
                )
                .bind(tid)
                .bind(product_uuid)
                .execute(&mut **tx)
                .await
                .map_err(|e| ApiError::internal(format!("grant tenant: {e}")))?;
            }
        }
    }
    Ok((rid, binding_ids))
}

async fn record_publish_version(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    draft: &repo::DraftRow,
    supplier_id: Uuid,
    config_value: &serde_json::Value,
    channels_snap: serde_json::Value,
    req: &PublishRequest,
    actor: &str,
) -> Result<(Uuid, i32), ApiError> {
    let version_no = {
        let n: i32 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(version_no), 0) + 1 FROM upstream_connector_versions WHERE supplier_id = $1",
        )
        .bind(supplier_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| ApiError::internal(format!("version no: {e}")))?;
        n
    };

    let version_id: Uuid = sqlx::query_scalar(
        r"
        INSERT INTO upstream_connector_versions
            (supplier_id, version_no, config_snapshot, channels_snapshot,
             endpoints_snapshot, comment, published_by)
        VALUES ($1,$2,$3,$4,$5,$6,$7)
        RETURNING id
        ",
    )
    .bind(supplier_id)
    .bind(version_no)
    .bind(config_value)
    .bind(&channels_snap)
    .bind(&draft.endpoints)
    .bind(&req.comment)
    .bind(actor)
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| ApiError::internal(format!("insert version: {e}")))?;

    sqlx::query(
        r"
        UPDATE upstream_connector_drafts
        SET linked_supplier_id = $2, status = 'ready', updated_at = NOW()
        WHERE id = $1
        ",
    )
    .bind(draft.id)
    .bind(supplier_id)
    .execute(&mut **tx)
    .await
    .map_err(|e| ApiError::internal(format!("link draft: {e}")))?;

    Ok((version_id, version_no))
}

/// 查询并校验路由目标产品（必须启用；可选冲突检查）。
async fn resolve_route_product(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    route: &super::PublishRoute,
    fail_if_route_exists: bool,
) -> Result<(Uuid, String), ApiError> {
    let product_external_id = route.product_ref.product_id.trim().to_string();
    let product_row = sqlx::query("SELECT id, enabled FROM products WHERE product_id = $1 LIMIT 1")
        .bind(&product_external_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| ApiError::internal(format!("query product: {e}")))?
        .ok_or_else(|| ApiError::not_found("product".into(), product_external_id.clone()))?;
    let product_uuid: Uuid = product_row.get("id");
    let enabled: bool = product_row.get("enabled");
    if !enabled {
        return Err(ApiError::validation(format!(
            "产品 {product_external_id} 未启用"
        )));
    }

    if fail_if_route_exists {
        let exists: bool = match route.tenant_id {
            Some(tid) => {
                sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM route_rules WHERE product_id=$1 AND tenant_id=$2 AND status='active')",
                )
                .bind(product_uuid)
                .bind(tid)
                .fetch_one(&mut **tx)
                .await
            }
            None => {
                sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM route_rules WHERE product_id=$1 AND tenant_id IS NULL AND status='active')",
                )
                .bind(product_uuid)
                .fetch_one(&mut **tx)
                .await
            }
        }
        .map_err(|e| ApiError::internal(format!("route conflict: {e}")))?;
        if exists {
            return Err(ApiError::validation(
                "ROUTE_CONFLICT: 同 tenant/product 已存在 active 路由",
            ));
        }
    }

    Ok((product_uuid, product_external_id))
}
