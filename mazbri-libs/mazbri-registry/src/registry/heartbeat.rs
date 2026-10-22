use crate::http_config_client::HttpConfigClient;
use crate::registry::pipeline::ConfigPipeline;
use crate::registry::types::{ConfigPush, ServiceMetadata};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};
const HEARTBEAT_INTERVAL_SECS: u64 = 30;
pub(crate) async fn spawn_heartbeat_task(
    token: tokio_util::sync::CancellationToken,
    client: Arc<HttpConfigClient>,
    metadata: Arc<parking_lot::RwLock<ServiceMetadata>>,
    pipeline: Arc<ConfigPipeline>,
    pipeline_mutex: Arc<Mutex<()>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(HEARTBEAT_INTERVAL_SECS));
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
                            info!(
                                server_version = resp.latest_version,
                                "心跳检测到配置变更，触发管线处理"
                            );
                            let service_name = {
                                let metadata_read = metadata.read();
                                metadata_read.service_name.clone()
                            };
                            let _guard = pipeline_mutex.lock().await;
                            let fetched = client.fetch_config().await;
                            let (content, content_hash, version) = match fetched {
                                Ok(config_resp) => (
                                    config_resp.content,
                                    config_resp.content_hash,
                                    config_resp.version,
                                ),
                                Err(e) => {
                                    warn!(error = %e, "心跳触发的配置获取失败，使用空内容");
                                    (String::new(), String::new(), resp.latest_version)
                                }
                            };
                            let push = ConfigPush {
                                service_name,
                                version,
                                content,
                                content_hash,
                                pushed_by: "heartbeat".to_string(),
                            };
                            if let Err(e) = pipeline.process_config_change(&push).await {
                                warn!(error = %e, "心跳触发的管线处理失败");
                            }
                        }
                        Ok(resp) if resp.success => {
                            debug!("心跳已发送");
                        }
                        Ok(resp) => {
                            warn!(error = %resp.error_message, "心跳失败");
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
