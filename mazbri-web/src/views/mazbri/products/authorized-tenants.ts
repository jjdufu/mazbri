import type { ProductTenantAccess } from "@/types/api/product";
import type { Tenant, TenantStatus } from "@/types/api/tenant";
export interface AuthorizedTenantRow {
  tenant_id: string;
  tenant_code: string;
  name: string;
  status: TenantStatus;
  granted_at: string;
}
export function buildAuthorizedTenantRows(
  accessList: ProductTenantAccess[],
  tenants: Tenant[],
): AuthorizedTenantRow[] {
  const tenantByCode = new Map(
    tenants.map((tenant) => [tenant.tenant_code, tenant]),
  );
  return accessList.map((access) => {
    const tenant = tenantByCode.get(access.tenant_code);
    return {
      tenant_id: tenant?.tenant_id ?? access.tenant_id,
      tenant_code: access.tenant_code,
      name: tenant?.name ?? access.tenant_code,
      status: tenant?.status ?? "pending",
      granted_at: access.created_at,
    };
  });
}
