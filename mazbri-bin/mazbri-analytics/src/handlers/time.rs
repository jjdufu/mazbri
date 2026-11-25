use chrono::{DateTime, Utc};

/// 解析请求中的时间参数：Unix 秒（支持整数与小数秒）、RFC3339 或 `%Y-%m-%d[ %H:%M:%S[.f]]`（按 UTC）。
pub fn parse_time_string(s: &str) -> Option<DateTime<Utc>> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(ts) = s.parse::<i64>() {
        return DateTime::from_timestamp(ts, 0);
    }
    if let Some(dt) = parse_fractional_unix_seconds(s) {
        return Some(dt);
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    if let Ok(dt) = s.parse::<DateTime<Utc>>() {
        return Some(dt);
    }
    let dt_formats = [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
    ];
    for fmt in dt_formats {
        if let Ok(naive_dt) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
            return Some(naive_dt.and_utc());
        }
    }
    if let Ok(naive_date) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        && let Some(naive_dt) = naive_date.and_hms_opt(0, 0, 0)
    {
        return Some(naive_dt.and_utc());
    }
    None
}

fn parse_fractional_unix_seconds(s: &str) -> Option<DateTime<Utc>> {
    let (secs_part, frac_part) = s.split_once('.')?;
    if secs_part.is_empty() || frac_part.is_empty() || frac_part.len() > 9 {
        return None;
    }
    if !secs_part.bytes().all(|b| b.is_ascii_digit())
        || !frac_part.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let secs: i64 = secs_part.parse().ok()?;
    let mut padded = String::with_capacity(9);
    padded.push_str(frac_part);
    while padded.len() < 9 {
        padded.push('0');
    }
    let nanos: u32 = padded.parse().ok()?;
    DateTime::from_timestamp(secs, nanos)
}

#[cfg(test)]
mod tests {
    use super::parse_time_string;

    #[test]
    fn parses_unix_seconds_rfc3339_fractional_and_plain_formats() {
        let expected = chrono::DateTime::from_timestamp(1_700_000_000, 0);
        assert_eq!(parse_time_string("1700000000"), expected);
        assert_eq!(parse_time_string("2023-11-14T22:13:20Z"), expected);
        assert_eq!(parse_time_string("2023-11-14 22:13:20"), expected);
        assert_eq!(parse_time_string(" 2023-11-14T22:13:20 "), expected);
        assert_eq!(
            parse_time_string("2023-11-14"),
            chrono::DateTime::from_timestamp(1_699_920_000, 0)
        );

        let expected_ms = chrono::DateTime::from_timestamp(1_700_000_000, 123_000_000);
        assert_eq!(parse_time_string("1700000000.123"), expected_ms);
        assert_eq!(parse_time_string("2023-11-14 22:13:20.123"), expected_ms);
        assert_eq!(parse_time_string("2023-11-14T22:13:20.123Z"), expected_ms);
        assert_eq!(parse_time_string("2023-11-14T22:13:20.123"), expected_ms);
    }

    #[test]
    fn rejects_garbage_and_trailing_tokens() {
        assert_eq!(parse_time_string("not-a-time"), None);
        assert_eq!(parse_time_string(""), None);
        assert_eq!(parse_time_string("2023-11-14 22:13:20 extra"), None);
        assert_eq!(parse_time_string("1700000000."), None);
    }
}
