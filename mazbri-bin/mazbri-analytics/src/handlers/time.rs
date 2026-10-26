use chrono::{DateTime, Utc};

/// 解析请求中的时间参数：Unix 秒、RFC3339 或 `%Y-%m-%d[ %H:%M:%S]`（按 UTC）。
pub fn parse_time_string(s: &str) -> Option<DateTime<Utc>> {
    let s = s.trim();
    if let Ok(ts) = s.parse::<i64>()
        && let Some(dt) = DateTime::from_timestamp(ts, 0)
    {
        return Some(dt);
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    let formats = ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d"];
    for fmt in formats {
        if let Ok((naive_dt, _)) = chrono::NaiveDateTime::parse_and_remainder(s, fmt) {
            return Some(naive_dt.and_utc());
        }
        if let Ok((naive_date, _)) = chrono::NaiveDate::parse_and_remainder(s, fmt)
            && let Some(naive_dt) = naive_date.and_hms_opt(0, 0, 0)
        {
            return Some(naive_dt.and_utc());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::parse_time_string;

    #[test]
    fn parses_unix_seconds_rfc3339_and_plain_formats() {
        let expected = chrono::DateTime::from_timestamp(1_700_000_000, 0);
        assert_eq!(parse_time_string("1700000000"), expected);
        assert_eq!(parse_time_string("2023-11-14T22:13:20Z"), expected);
        assert_eq!(parse_time_string("2023-11-14 22:13:20"), expected);
        assert_eq!(parse_time_string(" 2023-11-14T22:13:20 "), expected);
        assert_eq!(
            parse_time_string("2023-11-14"),
            chrono::DateTime::from_timestamp(1_699_920_000, 0)
        );
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(parse_time_string("not-a-time"), None);
        assert_eq!(parse_time_string(""), None);
    }
}
