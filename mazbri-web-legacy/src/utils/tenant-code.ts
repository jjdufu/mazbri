




export function normalizeTenantCode(
  value: unknown,
): string | undefined {
  if (value == null) return undefined;
  const raw = Array.isArray(value) ? value[0] : value;
  if (typeof raw !== "string" && typeof raw !== "number") return undefined;
  const code = String(raw).trim();
  if (!code) return undefined;
  const lower = code.toLowerCase();
  if (lower === "undefined" || lower === "null") return undefined;
  return code;
}


export function isValidTenantCode(value: unknown): value is string {
  return normalizeTenantCode(value) !== undefined;
}





export function requireTenantCode(
  value: unknown,
  context = "tenantCode",
): string {
  const code = normalizeTenantCode(value);
  if (!code) {
    throw new Error(`${context} is required`);
  }
  return code;
}
