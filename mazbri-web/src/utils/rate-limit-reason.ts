




export type RateLimitReason =
  | "tenant_qps"
  | "api_qps"
  | "client_ip_qps"
  | "bandwidth"
  | "deny_until"
  | "deny_all"
  | "snapshot_stale"
  | "channel_qps"
  | "queue_timeout"
  | "queue_full";

export type ReasonNavTarget =
  | "tenant_plan"
  | "tenant_api_override"
  | "tenant_abuse"
  | "tenant_emergency"
  | "upstream_channel";

export interface RateLimitReasonGuide {
  reason: RateLimitReason;

  labelKey: string;

  talkKey: string;
  target: ReasonNavTarget;
}

export const RATE_LIMIT_REASON_GUIDE: RateLimitReasonGuide[] = [
  {
    reason: "tenant_qps",
    labelKey: "rateLimit.reasons.tenant_qps.label",
    talkKey: "rateLimit.reasons.tenant_qps.talk",
    target: "tenant_plan",
  },
  {
    reason: "api_qps",
    labelKey: "rateLimit.reasons.api_qps.label",
    talkKey: "rateLimit.reasons.api_qps.talk",
    target: "tenant_api_override",
  },
  {
    reason: "client_ip_qps",
    labelKey: "rateLimit.reasons.client_ip_qps.label",
    talkKey: "rateLimit.reasons.client_ip_qps.talk",
    target: "tenant_abuse",
  },
  {
    reason: "bandwidth",
    labelKey: "rateLimit.reasons.bandwidth.label",
    talkKey: "rateLimit.reasons.bandwidth.talk",
    target: "tenant_abuse",
  },
  {
    reason: "deny_until",
    labelKey: "rateLimit.reasons.deny_until.label",
    talkKey: "rateLimit.reasons.deny_until.talk",
    target: "tenant_emergency",
  },
  {
    reason: "deny_all",
    labelKey: "rateLimit.reasons.deny_all.label",
    talkKey: "rateLimit.reasons.deny_all.talk",
    target: "tenant_emergency",
  },
  {
    reason: "snapshot_stale",
    labelKey: "rateLimit.reasons.snapshot_stale.label",
    talkKey: "rateLimit.reasons.snapshot_stale.talk",
    target: "tenant_emergency",
  },
  {
    reason: "channel_qps",
    labelKey: "rateLimit.reasons.channel_qps.label",
    talkKey: "rateLimit.reasons.channel_qps.talk",
    target: "upstream_channel",
  },
  {
    reason: "queue_timeout",
    labelKey: "rateLimit.reasons.queue_timeout.label",
    talkKey: "rateLimit.reasons.queue_timeout.talk",
    target: "upstream_channel",
  },
  {
    reason: "queue_full",
    labelKey: "rateLimit.reasons.queue_full.label",
    talkKey: "rateLimit.reasons.queue_full.talk",
    target: "upstream_channel",
  },
];

export function findReasonGuide(reason: string): RateLimitReasonGuide | undefined {
  return RATE_LIMIT_REASON_GUIDE.find((g) => g.reason === reason);
}


export function reasonNavLocation(
  target: ReasonNavTarget,
  tenantCode?: string,
): { path: string; query?: Record<string, string> } {
  switch (target) {
    case "tenant_plan":
    case "tenant_api_override":
    case "tenant_abuse":
    case "tenant_emergency":
      if (tenantCode) {
        return {
          path: `/ops/tenants/${tenantCode}`,
          query: { tab: "access-strategy" },
        };
      }
      return { path: "/ops/tenants" };
    case "upstream_channel":
      return { path: "/ops/upstream-supply" };
    default:
      return { path: "/ops/tenants" };
  }
}


export interface PlanRateTemplate {
  id: "trial" | "standard" | "premium";

  nameKey: string;
  max_concurrent: number;
  burst_size: number;

  client_ip_qps: number | null;
  client_ip_burst: number | null;
  max_bandwidth_mbps: number;

  clearApiLimits: boolean;
  enabled: boolean;
}

export const PLAN_RATE_TEMPLATES: PlanRateTemplate[] = [
  {
    id: "trial",
    nameKey: "rateLimit.templates.trial",
    max_concurrent: 10,
    burst_size: 10,
    client_ip_qps: null,
    client_ip_burst: null,
    max_bandwidth_mbps: 0,
    clearApiLimits: true,
    enabled: true,
  },
  {
    id: "standard",
    nameKey: "rateLimit.templates.standard",
    max_concurrent: 100,
    burst_size: 50,
    client_ip_qps: null,
    client_ip_burst: null,
    max_bandwidth_mbps: 0,
    clearApiLimits: true,
    enabled: true,
  },
  {
    id: "premium",
    nameKey: "rateLimit.templates.premium",
    max_concurrent: 1000,
    burst_size: 200,
    client_ip_qps: null,
    client_ip_burst: null,
    max_bandwidth_mbps: 0,
    clearApiLimits: true,
    enabled: true,
  },
];
