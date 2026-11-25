use crate::http_config_client::HttpConfigClient;
use crate::registry::pipeline::ConfigPipeline;
use crate::registry::types::{ConfigPush, ServiceMetadata};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

fn is_not_registered_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("not registered") || message.contains("未注册")
}

async fn apply_heartbeat_config_change(
    client: &HttpConfigClient,
    metadata: &parking_lot::RwLock<ServiceMetadata>,
    pipeline: &ConfigPipeline,
    pipeline_mutex: &Mutex<()>,
    latest_version: i32,
) {
    info!(
        server_version = latest_version,
        "心跳检测到配置变更，触发管线处理"
    );
    let service_name = metadata.read().service_name.clone();
    let _guard = pipeline_mutex.lock().await;
    let config_resp = match client.fetch_config().await {
        Ok(config_resp) => config_resp,
        Err(e) => {
            warn!(
                error = %e,
                "心跳触发的配置获取失败，中止本次更新并保留当前生效配置"
            );
            return;
        }
    };
    let version = if config_resp.version > 0 {
        config_resp.version
    } else {
        latest_version
    };
    let push = ConfigPush {
        service_name,
        version,
        content: config_resp.content,
        content_hash: config_resp.content_hash,
        pushed_by: "heartbeat".to_string(),
    };
    match pipeline.process_config_change(&push).await {
        Ok(res) if res.applied => {
            client
                .update_cached_config(push.content, push.content_hash, version)
                .await;
            metadata.write().config_version = version;
        }
        Ok(res) => {
            warn!(
                accepted = res.accepted,
                message = ?res.message,
                "心跳触发的配置未通过管线应用，保留当前配置版本"
            );
        }
        Err(e) => {
            warn!(error = %e, "心跳触发的管线处理失败");
        }
    }
}

async fn attempt_reregister(
    client: &HttpConfigClient,
    metadata: &parking_lot::RwLock<ServiceMetadata>,
) {
    let (reg_ip, reg_port, health_url, started_at) = {
        let m = metadata.read();
        (m.ip.clone(), m.port, m.health_check_url.clone(), m.started_at)
    };
    match client
        .register_node(reg_ip, reg_port, health_url, started_at)
        .await
    {
        Ok(reg) => {
            info!(
                node_id = %reg.id,
                config_version = reg.config_version,
                "节点自动重注册成功"
            );
        }
        Err(e) => {
            warn!(error = %e, "节点自动重注册失败");
        }
    }
}

pub(crate) fn spawn_heartbeat_task(
    token: tokio_util::sync::CancellationToken,
    client: Arc<HttpConfigClient>,
    metadata: Arc<parking_lot::RwLock<ServiceMetadata>>,
    pipeline: Arc<ConfigPipeline>,
    pipeline_mutex: Arc<Mutex<()>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(client.heartbeat_interval());
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                () = token.cancelled() => {
                    info!("心跳任务收到关闭信号");
                    break;
                }
                _ = interval.tick() => {
                    let (ip, port) = {
                        let m = metadata.read();
                        (m.ip.clone(), m.port)
                    };
                    match client.send_heartbeat(&ip, port).await {
                        Ok(resp) if resp.success && resp.config_changed => {
                            apply_heartbeat_config_change(
                                &client,
                                &metadata,
                                &pipeline,
                                &pipeline_mutex,
                                resp.latest_version,
                            )
                            .await;
                        }
                        Ok(resp) if resp.success => {
                            debug!("心跳已发送");
                        }
                        Ok(resp) => {
                            warn!(error = %resp.error_message, "心跳失败");
                            if is_not_registered_error(&resp.error_message) {
                                attempt_reregister(&client, &metadata).await;
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "心跳请求失败");
                        }
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::is_not_registered_error;

    #[test]
    fn detects_not_registered_messages() {
        assert!(is_not_registered_error("Node is not registered"));
        assert!(is_not_registered_error("节点未注册"));
        assert!(!is_not_registered_error("database timeout"));
    }
}
