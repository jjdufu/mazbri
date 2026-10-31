//! 告警 webhook 通知：触发与恢复时向规则配置的地址 POST JSON。
use super::rule::WebhookPayload;
use std::time::Duration;

/// webhook 发送器。
#[derive(Clone)]
pub struct Notifier {
    client: reqwest::Client,
    console_base_url: Option<String>,
}

impl Notifier {
    /// # Errors
    ///
    /// HTTP 客户端构建失败时返回错误。
    pub fn new(timeout: Duration, console_base_url: Option<String>) -> reqwest::Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            client,
            console_base_url: console_base_url.map(|u| u.trim_end_matches('/').to_string()),
        })
    }

    /// 控制台事件详情链接（SPA 为 hash 路由）。
    #[must_use]
    pub fn console_url(&self, event_id: uuid::Uuid) -> Option<String> {
        self.console_base_url
            .as_ref()
            .map(|base| format!("{base}/#/ops/alerts/events?event_id={event_id}"))
    }

    /// 异步发送到全部地址；失败只记日志，不影响评估循环。
    pub fn dispatch(&self, webhooks: &[String], payload: &WebhookPayload) {
        for url in webhooks {
            let client = self.client.clone();
            let url = url.clone();
            let payload = payload.clone();
            tokio::spawn(async move {
                match client.post(&url).json(&payload).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        tracing::info!(target: "alerting", event_id = %payload.event_id, status = payload.status, "告警 webhook 已送达");
                    }
                    Ok(resp) => {
                        tracing::warn!(target: "alerting", event_id = %payload.event_id, http_status = resp.status().as_u16(), "告警 webhook 返回非成功状态");
                    }
                    Err(error) => {
                        tracing::warn!(target: "alerting", event_id = %payload.event_id, %error, "告警 webhook 发送失败");
                    }
                }
            });
        }
    }
}
