//! Process resource metrics read from `/proc/self` (Linux).
//!
//! Names follow the Prometheus client conventions so dashboards and `PromQL` stay standard.
//! On non-Linux targets the reads fail and the gauges stay at 0.
use super::register_collector_once;
use crate::REGISTRY;
use prometheus::{Gauge, Opts};
use std::sync::LazyLock;
use std::time::Duration;

/// Sampling interval; vector scrapes every 15s, so 5s keeps each scrape fresh.
const SAMPLE_INTERVAL: Duration = Duration::from_secs(5);
/// `USER_HZ` exposed by `/proc` (fixed at 100 on every Linux ABI we ship: amd64, arm64).
const CLOCK_TICKS_PER_SEC: f64 = 100.0;

fn gauge(name: &str, help: &str) -> Gauge {
    Gauge::with_opts(Opts::new(name, help)).unwrap_or_else(|e| panic!("{name} metric: {e}"))
}

pub static PROCESS_CPU_SECONDS_TOTAL: LazyLock<Gauge> = LazyLock::new(|| {
    gauge(
        "process_cpu_seconds_total",
        "Total user and system CPU time spent in seconds (monotonic)",
    )
});
pub static PROCESS_RESIDENT_MEMORY_BYTES: LazyLock<Gauge> = LazyLock::new(|| {
    gauge(
        "process_resident_memory_bytes",
        "Resident memory size in bytes",
    )
});
pub static PROCESS_START_TIME_SECONDS: LazyLock<Gauge> = LazyLock::new(|| {
    gauge(
        "process_start_time_seconds",
        "Start time of the process since unix epoch in seconds",
    )
});
pub static PROCESS_OPEN_FDS: LazyLock<Gauge> =
    LazyLock::new(|| gauge("process_open_fds", "Number of open file descriptors"));

/// # Errors
///
/// Prometheus registration failed.
pub fn register_process_metrics() -> Result<(), prometheus::Error> {
    register_collector_once(&REGISTRY, Box::new(PROCESS_CPU_SECONDS_TOTAL.clone()))?;
    register_collector_once(&REGISTRY, Box::new(PROCESS_RESIDENT_MEMORY_BYTES.clone()))?;
    register_collector_once(&REGISTRY, Box::new(PROCESS_START_TIME_SECONDS.clone()))?;
    register_collector_once(&REGISTRY, Box::new(PROCESS_OPEN_FDS.clone()))?;
    Ok(())
}

/// CPU and start-time fields of `/proc/self/stat`, in clock ticks.
struct StatFields {
    utime: f64,
    stime: f64,
    starttime: f64,
}

fn parse_stat(raw: &str) -> Option<StatFields> {
    // comm may contain spaces/parens; everything after the last ')' is whitespace separated.
    let rest = raw.get(raw.rfind(')')? + 1..)?;
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // stat(5): utime=14, stime=15, starttime=22 (1-based) → index 11, 12, 19 after comm.
    Some(StatFields {
        utime: fields.get(11)?.parse().ok()?,
        stime: fields.get(12)?.parse().ok()?,
        starttime: fields.get(19)?.parse().ok()?,
    })
}

fn parse_boot_time(raw: &str) -> Option<f64> {
    raw.lines()
        .find_map(|line| line.strip_prefix("btime "))
        .and_then(|v| v.trim().parse().ok())
}

fn parse_rss_bytes(raw: &str) -> Option<f64> {
    let kb: f64 = raw
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    Some(kb * 1024.0)
}

fn count_open_fds() -> Option<f64> {
    let count = std::fs::read_dir("/proc/self/fd").ok()?.count();
    // Subtract the descriptor read_dir itself held open while listing.
    Some(f64::from(
        u32::try_from(count.saturating_sub(1)).unwrap_or(u32::MAX),
    ))
}

fn sample_start_time() {
    let stat = std::fs::read_to_string("/proc/self/stat")
        .ok()
        .and_then(|raw| parse_stat(&raw));
    let boot = std::fs::read_to_string("/proc/stat")
        .ok()
        .and_then(|raw| parse_boot_time(&raw));
    if let (Some(stat), Some(boot)) = (stat, boot) {
        PROCESS_START_TIME_SECONDS.set(boot + stat.starttime / CLOCK_TICKS_PER_SEC);
    }
}

fn sample() {
    if let Some(stat) = std::fs::read_to_string("/proc/self/stat")
        .ok()
        .and_then(|raw| parse_stat(&raw))
    {
        PROCESS_CPU_SECONDS_TOTAL.set((stat.utime + stat.stime) / CLOCK_TICKS_PER_SEC);
    }
    if let Some(rss) = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|raw| parse_rss_bytes(&raw))
    {
        PROCESS_RESIDENT_MEMORY_BYTES.set(rss);
    }
    if let Some(fds) = count_open_fds() {
        PROCESS_OPEN_FDS.set(fds);
    }
}

/// Register the process gauges, set the start time once and sample the rest every 5s.
/// Must be called from within a tokio runtime. Registration errors are logged, not fatal.
pub fn spawn_process_metrics() {
    if let Err(error) = register_process_metrics() {
        tracing::warn!(%error, "process metrics registration failed");
    }
    sample_start_time();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(SAMPLE_INTERVAL);
        loop {
            tick.tick().await;
            sample();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_parses_after_comm_with_spaces() {
        let raw = "4242 (my (odd) bin) S 1 4242 4242 0 -1 4194560 100 0 0 0 250 50 0 0 20 0 8 0 123456 1000 200 18446744073709551615";
        let stat = parse_stat(raw).expect("stat");
        assert!((stat.utime - 250.0).abs() < f64::EPSILON);
        assert!((stat.stime - 50.0).abs() < f64::EPSILON);
        assert!((stat.starttime - 123_456.0).abs() < f64::EPSILON);
    }

    #[test]
    fn boot_time_and_rss_parse() {
        assert_eq!(
            parse_boot_time("cpu 1 2\nbtime 1700000000\n"),
            Some(1_700_000_000.0)
        );
        assert_eq!(
            parse_rss_bytes("Name:\tx\nVmRSS:\t  2048 kB\n"),
            Some(2048.0 * 1024.0)
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn live_proc_self_is_readable() {
        let stat = std::fs::read_to_string("/proc/self/stat").expect("stat");
        assert!(parse_stat(&stat).is_some());
        assert!(count_open_fds().is_some_and(|n| n >= 1.0));
    }
}
