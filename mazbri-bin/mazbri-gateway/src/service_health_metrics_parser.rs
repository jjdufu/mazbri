pub(crate) fn parse_health_metrics_body(body: &str) -> anyhow::Result<bool> {
    if let Some(value) = find_metric_status_value(body, "healthy")? {
        return Ok(value == 1);
    }
    if let Some(value) = find_metric_status_value(body, "unhealthy")? {
        return Ok(value == 0);
    }
    anyhow::bail!("mazbri_rpc_health_check metric not found in /metrics response")
}
fn find_metric_status_value(body: &str, status: &str) -> anyhow::Result<Option<i64>> {
    let pattern = format!(r#"status="{status}""#);
    for line in body.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("mazbri_rpc_health_check{") {
            if rest.contains(&pattern) {
                if let Some(value_str) = rest.split('}').nth(1) {
                    let value: i64 = value_str
                        .trim()
                        .parse()
                        .map_err(|e| anyhow::anyhow!("Failed to parse gauge value: {e}"))?;
                    return Ok(Some(value));
                }
            }
        }
    }
    Ok(None)
}
