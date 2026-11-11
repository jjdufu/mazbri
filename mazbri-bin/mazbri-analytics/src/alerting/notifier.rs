//! 告警通知：触发与恢复时按规则配置发送邮件（默认渠道，SMTP）与 webhook（可选）。
use super::rule::{AlertNotification, NotifyConfig};
use lettre::message::header::ContentType;
use lettre::message::{Mailbox, Message};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};
use serde::Deserialize;
use std::fmt;
use std::fmt::Write as _;
use std::time::Duration;

/// SMTP 连接加密方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SmtpTls {
    /// 明文连接后升级 TLS（常见 587 端口）。
    Starttls,
    /// 直接 TLS（常见 465 端口）。
    Tls,
    /// 不加密，仅用于内网中继。
    None,
}

/// `[alerting.smtp]` 配置段。
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub tls: SmtpTls,
    /// 认证用户名；与 `password` 同时配置或同时省略。
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    /// 发件人，如 `Mazbri 告警 <alerts@example.com>`。
    pub from: String,
    /// 单封邮件发送超时（秒）。
    #[serde(default = "default_smtp_timeout")]
    pub timeout_secs: u64,
}

const fn default_smtp_timeout() -> u64 {
    10
}

impl fmt::Debug for SmtpConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SmtpConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("tls", &self.tls)
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "***"))
            .field("from", &self.from)
            .field("timeout_secs", &self.timeout_secs)
            .finish()
    }
}

impl SmtpConfig {
    /// # Errors
    ///
    /// 主机为空、发件人无法解析、认证字段不成对或超时越界时返回错误。
    pub fn validate(&self) -> Result<(), String> {
        if self.host.trim().is_empty() {
            return Err("alerting.smtp.host 不能为空".to_string());
        }
        if self.port == 0 {
            return Err("alerting.smtp.port 不能为 0".to_string());
        }
        self.from
            .parse::<Mailbox>()
            .map_err(|e| format!("alerting.smtp.from 无法解析: {e}"))?;
        if self.username.is_some() != self.password.is_some() {
            return Err("alerting.smtp.username 与 password 必须同时配置".to_string());
        }
        if !(1..=120).contains(&self.timeout_secs) {
            return Err("alerting.smtp.timeout_secs 必须在 1..=120 之间".to_string());
        }
        Ok(())
    }
}

#[derive(Clone)]
struct Mailer {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}

impl Mailer {
    fn new(config: &SmtpConfig) -> Result<Self, String> {
        config.validate()?;
        let builder = match config.tls {
            SmtpTls::Starttls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)
                .map_err(|e| format!("SMTP STARTTLS 配置失败: {e}"))?,
            SmtpTls::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host)
                .map_err(|e| format!("SMTP TLS 配置失败: {e}"))?,
            SmtpTls::None => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.host),
        };
        let builder = builder
            .port(config.port)
            .timeout(Some(Duration::from_secs(config.timeout_secs)));
        let builder = match (&config.username, &config.password) {
            (Some(user), Some(password)) => {
                builder.credentials(Credentials::new(user.clone(), password.clone()))
            }
            _ => builder,
        };
        Ok(Self {
            transport: builder.build(),
            from: config
                .from
                .parse()
                .map_err(|e| format!("alerting.smtp.from 无法解析: {e}"))?,
        })
    }
}

/// 告警通知发送器。
#[derive(Clone)]
pub struct Notifier {
    client: reqwest::Client,
    mailer: Option<Mailer>,
}

impl Notifier {
    /// # Errors
    ///
    /// HTTP 客户端或 SMTP 传输构建失败时返回错误。
    pub fn new(webhook_timeout: Duration, smtp: Option<&SmtpConfig>) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(webhook_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| format!("webhook 客户端构建失败: {e}"))?;
        Ok(Self {
            client,
            mailer: smtp.map(Mailer::new).transpose()?,
        })
    }

    /// 异步发送到规则配置的全部渠道；失败只记日志，不影响评估循环。
    /// 日志不输出收件人与 webhook 地址。
    pub fn dispatch(&self, notify: &NotifyConfig, notification: &AlertNotification) {
        if !notify.enabled {
            return;
        }
        if !notify.emails.is_empty() {
            self.dispatch_email(&notify.emails, notification);
        }
        for url in &notify.webhooks {
            let client = self.client.clone();
            let url = url.clone();
            let payload = notification.clone();
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

    fn dispatch_email(&self, recipients: &[String], notification: &AlertNotification) {
        let Some(mailer) = self.mailer.clone() else {
            tracing::warn!(target: "alerting", event_id = %notification.event_id, "规则配置了邮件收件人但未配置 alerting.smtp，邮件未发送");
            return;
        };
        let message = match build_message(&mailer.from, recipients, notification) {
            Ok(message) => message,
            Err(error) => {
                tracing::warn!(target: "alerting", event_id = %notification.event_id, %error, "告警邮件构建失败");
                return;
            }
        };
        let event_id = notification.event_id;
        let status = notification.status;
        let count = recipients.len();
        tokio::spawn(async move {
            match mailer.transport.send(message).await {
                Ok(_) => {
                    tracing::info!(target: "alerting", %event_id, status, recipients = count, "告警邮件已发送");
                }
                Err(error) => {
                    tracing::warn!(target: "alerting", %event_id, %error, "告警邮件发送失败");
                }
            }
        });
    }
}

fn build_message(
    from: &Mailbox,
    recipients: &[String],
    notification: &AlertNotification,
) -> Result<Message, String> {
    let mut builder = Message::builder()
        .from(from.clone())
        .subject(email_subject(notification));
    for recipient in recipients {
        let mailbox: Mailbox = recipient
            .parse()
            .map_err(|e| format!("收件人地址无法解析: {e}"))?;
        builder = builder.to(mailbox);
    }
    builder
        .header(ContentType::TEXT_PLAIN)
        .body(email_body(notification))
        .map_err(|e| e.to_string())
}

/// 邮件主题：`[告警][critical][触发] 规则名`。
#[must_use]
pub fn email_subject(notification: &AlertNotification) -> String {
    let status = if notification.status == "resolved" {
        "恢复"
    } else {
        "触发"
    };
    format!(
        "[告警][{}][{status}] {}",
        notification.severity, notification.rule
    )
}

/// 邮件纯文本正文。
#[must_use]
pub fn email_body(notification: &AlertNotification) -> String {
    let mut body = String::new();
    let _ = writeln!(body, "规则：{}", notification.rule);
    let _ = writeln!(body, "级别：{}", notification.severity);
    let _ = writeln!(body, "状态：{}", notification.status);
    let _ = writeln!(
        body,
        "当前值：{} {} {}",
        notification.value, notification.op, notification.threshold
    );
    if !notification.labels.is_empty() {
        let labels = notification
            .labels
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(body, "标签：{labels}");
    }
    let _ = writeln!(body, "开始时间：{}", notification.started_at.to_rfc3339());
    if let Some(resolved_at) = notification.resolved_at {
        let _ = writeln!(body, "恢复时间：{}", resolved_at.to_rfc3339());
    }
    let _ = writeln!(body, "事件 ID：{}", notification.event_id);
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use std::collections::BTreeMap;

    fn notification(status: &'static str) -> AlertNotification {
        AlertNotification {
            status,
            event_id: uuid::Uuid::nil(),
            rule_id: uuid::Uuid::nil(),
            rule: "服务健康异常".to_string(),
            severity: "critical".to_string(),
            value: 1.0,
            threshold: 1.0,
            op: ">=",
            labels: BTreeMap::from([("service".to_string(), "mazbri-cbpro".to_string())]),
            started_at: Utc.with_ymd_and_hms(2026, 10, 31, 10, 0, 0).unwrap(),
            resolved_at: (status == "resolved")
                .then(|| Utc.with_ymd_and_hms(2026, 10, 31, 10, 5, 0).unwrap()),
        }
    }

    fn smtp() -> SmtpConfig {
        SmtpConfig {
            host: "smtp.example.com".to_string(),
            port: 587,
            tls: SmtpTls::Starttls,
            username: Some("alerts".to_string()),
            password: Some("secret-value".to_string()),
            from: "Mazbri 告警 <alerts@example.com>".to_string(),
            timeout_secs: 10,
        }
    }

    #[test]
    fn subject_and_body_describe_firing_and_resolved() {
        let firing = notification("firing");
        assert_eq!(
            email_subject(&firing),
            "[告警][critical][触发] 服务健康异常"
        );
        let body = email_body(&firing);
        assert!(body.contains("当前值：1 >= 1"));
        assert!(body.contains("标签：service=mazbri-cbpro"));
        assert!(!body.contains("控制台"));
        assert!(!body.contains("恢复时间"));

        let resolved = notification("resolved");
        assert_eq!(
            email_subject(&resolved),
            "[告警][critical][恢复] 服务健康异常"
        );
        assert!(email_body(&resolved).contains("恢复时间：2026-10-31T10:05:00+00:00"));
    }

    #[test]
    fn message_lists_every_recipient_and_rejects_invalid_ones() {
        let from: Mailbox = "alerts@example.com".parse().unwrap();
        let recipients = vec!["a@example.com".to_string(), "b@example.com".to_string()];
        let message = build_message(&from, &recipients, &notification("firing")).unwrap();
        let raw = String::from_utf8(message.formatted()).unwrap();
        assert!(raw.contains("a@example.com"));
        assert!(raw.contains("b@example.com"));
        assert!(
            build_message(
                &from,
                &["not-an-address".to_string()],
                &notification("firing")
            )
            .is_err()
        );
    }

    #[test]
    fn smtp_config_validation_and_debug_redaction() {
        assert!(smtp().validate().is_ok());
        let debug = format!("{:?}", smtp());
        assert!(!debug.contains("secret-value"));

        let mut half_auth = smtp();
        half_auth.password = None;
        assert!(half_auth.validate().is_err());
        let mut bad_from = smtp();
        bad_from.from = "nobody".to_string();
        assert!(bad_from.validate().is_err());
        let mut empty_host = smtp();
        empty_host.host = " ".to_string();
        assert!(empty_host.validate().is_err());
        let mut slow = smtp();
        slow.timeout_secs = 0;
        assert!(slow.validate().is_err());
    }
}
