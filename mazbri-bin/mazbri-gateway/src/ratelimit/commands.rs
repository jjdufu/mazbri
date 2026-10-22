use mazbri_rpc::ratelimit::ControlCommand;
use parking_lot::RwLock;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use uuid::Uuid;

/// Per-tenant control-plane state applied on the local gateway.
///
/// Same command type is last-write-wins by `effective_at_unix` (not stacked).
/// State is rebuilt from active commands on each poll; expiry restores defaults.
#[derive(Debug, Clone)]
pub struct TenantControlState {
    pub deny_until_unix: Option<i64>,
    deny_effective_at: i64,
    pub quota_multiplier: f64,
    quota_effective_at: i64,
    /// Set when a newer `refresh_policy` command was seen and not yet consumed.
    refresh_requested: bool,
    refresh_effective_at: i64,
}

impl Default for TenantControlState {
    fn default() -> Self {
        Self {
            deny_until_unix: None,
            deny_effective_at: 0,
            quota_multiplier: 1.0,
            quota_effective_at: 0,
            refresh_requested: false,
            refresh_effective_at: 0,
        }
    }
}

#[derive(Clone, Default)]
pub struct ControlCommandStore {
    inner: Arc<RwLock<HashMap<Uuid, TenantControlState>>>,
    /// Global flag: at least one tenant requested an immediate policy refresh.
    policy_refresh_pending: Arc<AtomicBool>,
}

impl ControlCommandStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Rebuild local state from currently active control commands (server-filtered).
    ///
    /// For each tenant + command type, the command with the highest
    /// `effective_at_unix` wins. Tenants with no active commands drop back to
    /// defaults (deny cleared, multiplier 1.0). Pending refresh flags for
    /// already-seen commands are preserved so a poll cannot lose a refresh
    /// that has not been consumed yet.
    pub fn update_from_commands(&self, commands: &[ControlCommand]) {
        let previous = self.inner.read().clone();
        let mut next: HashMap<Uuid, TenantControlState> = HashMap::new();

        // Ascending effective_at so later iterations overwrite older ones.
        let mut ordered = commands.to_vec();
        ordered.sort_by(|a, b| {
            a.effective_at_unix
                .cmp(&b.effective_at_unix)
                .then_with(|| a.created_at_unix.cmp(&b.created_at_unix))
        });

        let now_unix = chrono::Utc::now().timestamp();
        let mut any_refresh = false;

        for command in &ordered {
            let Ok(tenant_id) = Uuid::parse_str(&command.tenant_id) else {
                continue;
            };
            // Skip commands that are not yet effective or already expired.
            if command.effective_at_unix > now_unix {
                continue;
            }
            if let Some(expires_at) = command.expires_at_unix
                && expires_at <= now_unix
            {
                continue;
            }

            let state = next.entry(tenant_id).or_default();
            match command.command_type.as_str() {
                "deny_until" => apply_deny_until(state, command, now_unix),
                "quota_correction" => apply_quota_correction(state, command),
                "refresh_policy" => {
                    any_refresh |= apply_refresh_policy(state, command, previous.get(&tenant_id));
                }
                _ => {}
            }
        }

        // Preserve unconsumed refresh requests for tenants whose refresh_policy
        // command may have expired between polls before the gateway acted.
        for (tenant_id, prev) in &previous {
            if prev.refresh_requested {
                let entry = next.entry(*tenant_id).or_default();
                if !entry.refresh_requested {
                    entry.refresh_requested = true;
                    entry.refresh_effective_at =
                        entry.refresh_effective_at.max(prev.refresh_effective_at);
                    any_refresh = true;
                }
            }
        }

        *self.inner.write() = next;
        if any_refresh {
            self.policy_refresh_pending.store(true, Ordering::SeqCst);
        }
    }

    pub fn is_denied(&self, tenant_id: Uuid, now_unix: i64) -> Option<i64> {
        let mut guard = self.inner.write();
        let state = guard.get_mut(&tenant_id)?;
        let deny_until_unix = state.deny_until_unix?;
        if deny_until_unix > now_unix {
            Some(deny_until_unix)
        } else {
            // Local expiry cleanup (command may already be gone from server list).
            state.deny_until_unix = None;
            None
        }
    }

    pub fn quota_multiplier(&self, tenant_id: Uuid) -> f64 {
        self.inner
            .read()
            .get(&tenant_id)
            .map_or(1.0, |state| state.quota_multiplier)
            .clamp(0.1, 10.0)
    }

    /// Consume the global pending policy-refresh flag (full snapshot refresh is acceptable).
    pub fn take_policy_refresh_request(&self) -> bool {
        if !self.policy_refresh_pending.swap(false, Ordering::SeqCst) {
            return false;
        }
        let mut guard = self.inner.write();
        let mut any = false;
        for state in guard.values_mut() {
            if state.refresh_requested {
                state.refresh_requested = false;
                any = true;
            }
        }
        any
    }
}

fn apply_deny_until(state: &mut TenantControlState, command: &ControlCommand, now_unix: i64) {
    if command.effective_at_unix < state.deny_effective_at {
        return;
    }
    let payload = serde_json::from_str::<Value>(&command.command_payload).unwrap_or_default();
    let deny_until = payload
        .get("deny_until_unix")
        .and_then(Value::as_i64)
        .or_else(|| {
            payload
                .get("deny_for_secs")
                .and_then(Value::as_i64)
                .map(|secs| now_unix + secs.max(0))
        });
    state.deny_until_unix = deny_until.filter(|ts| *ts > now_unix);
    state.deny_effective_at = command.effective_at_unix;
}

fn apply_quota_correction(state: &mut TenantControlState, command: &ControlCommand) {
    if command.effective_at_unix < state.quota_effective_at {
        return;
    }
    let payload = serde_json::from_str::<Value>(&command.command_payload).unwrap_or_default();
    state.quota_multiplier = payload
        .get("quota_multiplier")
        .and_then(Value::as_f64)
        .or_else(|| payload.get("tenant_qps_multiplier").and_then(Value::as_f64))
        .unwrap_or(1.0)
        .clamp(0.1, 10.0);
    state.quota_effective_at = command.effective_at_unix;
}

/// Returns `true` when a new (not yet consumed) refresh was requested.
fn apply_refresh_policy(
    state: &mut TenantControlState,
    command: &ControlCommand,
    prev: Option<&TenantControlState>,
) -> bool {
    if command.effective_at_unix < state.refresh_effective_at {
        return false;
    }
    // New or equal-effective command still marks refresh once per newer effective_at.
    let already_consumed = prev.is_some_and(|p| {
        p.refresh_effective_at >= command.effective_at_unix && !p.refresh_requested
    });
    state.refresh_effective_at = command.effective_at_unix;
    if already_consumed {
        state.refresh_requested = false;
        return false;
    }
    let prev_pending = prev.is_some_and(|p| {
        p.refresh_requested && p.refresh_effective_at == command.effective_at_unix
    });
    let is_newer = prev.is_none_or(|p| command.effective_at_unix > p.refresh_effective_at);
    state.refresh_requested = prev_pending || is_newer;
    state.refresh_requested
}
