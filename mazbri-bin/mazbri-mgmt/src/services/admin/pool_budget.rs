//! mgmt `admin_database` 与 analytics 共用 `mazbri_admin` 角色。
//!
//! 诊断结论（pool timed out while waiting for an open connection）：
//! - `pg_stat_activity`：无 idle-in-transaction；LISTEN 已走独立 `-listen` 连接；
//! - 根因是角色 `CONNECTION LIMIT` 小于「mgmt 池 + analytics 多子系统各自建池」之和，
//!   空闲池连接占满配额后登录无法再建连。
//! - 修复：analytics 共用一池；提高角色限流与池 max，并设置 `application_name`。

/// 与 `mconfigs.tmpl/mgmt-bootstrap.toml` `[admin_database].max_connections` 对齐。
pub const MGMT_ADMIN_POOL_MAX: u32 = 8;
/// 与 `mconfigs.tmpl/analytics-bootstrap.toml` `[alerting.database].max_connections` 对齐；
/// analytics 内 alerting/capacity/anomaly/slo 必须共用这一池。
pub const ANALYTICS_ADMIN_POOL_MAX: u32 = 8;
/// 与 `init-dbs` `connection_limit_for("mazbri_admin")` 对齐。
pub const MAZBRI_ADMIN_ROLE_CONN_LIMIT: u32 = 20;

/// 登录路径对池的瞬时占用模型：查用户 + 审计/设置短查询可能并行。
pub const LOGIN_CONCURRENT_ACQUIRES: u32 = 3;

#[must_use]
pub const fn role_limit_covers_pools() -> bool {
    MGMT_ADMIN_POOL_MAX + ANALYTICS_ADMIN_POOL_MAX + 2 + 2 == MAZBRI_ADMIN_ROLE_CONN_LIMIT
}

#[must_use]
pub const fn login_fits_in_mgmt_pool() -> bool {
    LOGIN_CONCURRENT_ACQUIRES <= MGMT_ADMIN_POOL_MAX
}

const _: () = assert!(role_limit_covers_pools());
const _: () = assert!(login_fits_in_mgmt_pool());

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tokio::sync::Semaphore;

    #[test]
    fn admin_role_limit_matches_bootstrap_and_init_dbs_budget() {
        assert_eq!(
            MGMT_ADMIN_POOL_MAX + ANALYTICS_ADMIN_POOL_MAX + 2 + 2,
            MAZBRI_ADMIN_ROLE_CONN_LIMIT,
            "8+8+2手工会话+2余量 必须等于角色限流"
        );
        assert!(role_limit_covers_pools());
        assert!(login_fits_in_mgmt_pool());
    }

    /// 模拟「池 max=N、并发登录各占 1 条连接做短查询」：在预算内不应出现 acquire 超时。
    #[tokio::test]
    async fn concurrent_login_shaped_acquires_complete_within_pool_budget() {
        let pool = Arc::new(Semaphore::new(MGMT_ADMIN_POOL_MAX as usize));
        let acquire_timeout = Duration::from_millis(200);
        let login_hold = Duration::from_millis(20);
        let concurrent_logins = (MGMT_ADMIN_POOL_MAX * 2) as usize;

        let mut handles = Vec::with_capacity(concurrent_logins);
        for _ in 0..concurrent_logins {
            let pool = Arc::clone(&pool);
            handles.push(tokio::spawn(async move {
                let started = Instant::now();
                let permit = tokio::time::timeout(acquire_timeout, pool.acquire())
                    .await
                    .expect("login-shaped acquire must fit pool budget")
                    .expect("semaphore");
                tokio::time::sleep(login_hold).await;
                drop(permit);
                started.elapsed()
            }));
        }

        for handle in handles {
            let elapsed = handle.await.expect("join");
            assert!(
                elapsed < acquire_timeout,
                "concurrent login acquire exceeded budget: {elapsed:?}"
            );
        }
    }

    /// 反例：角色限流被空闲池占满时，新登录会在 `acquire_timeout` 内失败（本回归的根因模型）。
    #[tokio::test]
    async fn exhausted_role_budget_times_out_new_login_acquire() {
        // 已有空闲连接占满「角色」配额后，mgmt 池无法再扩容，只剩 0 条可借。
        let role_slots = Arc::new(Semaphore::new(0));
        let acquire_timeout = Duration::from_millis(50);
        let result = tokio::time::timeout(acquire_timeout, role_slots.acquire()).await;
        assert!(
            result.is_err(),
            "角色配额耗尽时新登录必须在 acquire_timeout 失败"
        );
    }
}
