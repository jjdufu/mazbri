import { CachePolicySection } from "./security/CachePolicySection";
import { RateLimitSection } from "./security/RateLimitSection";

export interface RateLimitAndCacheTabProps {
  tenantCode: string;
}

export function RateLimitAndCacheTab({
  tenantCode,
}: RateLimitAndCacheTabProps) {
  return (
    <div
      className="grid grid-cols-1 gap-4 xl:grid-cols-2 items-start min-w-0"
      data-testid="tenant-policies-tab"
    >
      <RateLimitSection tenantCode={tenantCode} />
      <CachePolicySection tenantCode={tenantCode} />
    </div>
  );
}
