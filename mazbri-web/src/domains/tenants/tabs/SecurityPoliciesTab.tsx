import { CachePolicySection } from "./security/CachePolicySection";
import { RateLimitSection } from "./security/RateLimitSection";
import { TransportEncryptionSection } from "./security/TransportEncryptionSection";
import { WhitelistSection } from "./security/WhitelistSection";

export interface SecurityPoliciesTabProps {
  tenantCode: string;
}

export function SecurityPoliciesTab({ tenantCode }: SecurityPoliciesTabProps) {
  return (
    <div className="space-y-4 min-w-0" data-testid="tenant-security-tab">
      <WhitelistSection tenantCode={tenantCode} />

      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2 items-start">
        <CachePolicySection tenantCode={tenantCode} />
        <RateLimitSection tenantCode={tenantCode} />
      </div>

      <TransportEncryptionSection tenantCode={tenantCode} />
    </div>
  );
}
