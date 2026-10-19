const HTTPS_SCHEME = "https://";
const SENSITIVE_URL_KEYS = new Set(["endpoint_url", "upstream_url"]);
function stripScheme(value: string): string {
  return value.replace(/^[a-z][a-z0-9+.-]*:\/\//i, "");
}
export function stripUrlScheme(value: string): string {
  return stripScheme(value.trim());
}
export function normalizeUpstreamUrlInput(value: string): string {
  const trimmed = value.trim();
  if (!trimmed) return trimmed;
  return /^[a-z][a-z0-9+.-]*:\/\//i.test(trimmed)
    ? trimmed
    : `${HTTPS_SCHEME}${trimmed}`;
}
export function formatUrlDisplay(value: string | null | undefined): string {
  if (!value) return "-";
  const trimmed = value.trim();
  if (!trimmed) return "-";
  const withoutScheme = stripScheme(trimmed);
  return withoutScheme || "-";
}
export function sanitizeSensitiveValue<T>(value: T): T {
  if (Array.isArray(value)) {
    return value.map((item) => sanitizeSensitiveValue(item)) as T;
  }
  if (value && typeof value === "object") {
    const record = value as Record<string, unknown>;
    const next: Record<string, unknown> = {};
    for (const [key, item] of Object.entries(record)) {
      if (SENSITIVE_URL_KEYS.has(key) && typeof item === "string") {
        next[key] = formatUrlDisplay(item);
        continue;
      }
      next[key] = sanitizeSensitiveValue(item);
    }
    return next as T;
  }
  if (typeof value === "string" && /^[a-z][a-z0-9+.-]*:\/\//i.test(value)) {
    return formatUrlDisplay(value) as T;
  }
  return value;
}
