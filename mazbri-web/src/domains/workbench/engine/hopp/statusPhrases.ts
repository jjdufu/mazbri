

export const STATUS_PHRASES: Readonly<Record<number, string>> = Object.freeze({
  200: "OK",
  201: "Created",
  204: "No Content",
  400: "Bad Request",
  401: "Unauthorized",
  403: "Forbidden",
  404: "Not Found",
  409: "Conflict",
  415: "Unsupported Media Type",
  429: "Too Many Requests",
  500: "Internal Server Error",
  502: "Bad Gateway",
  503: "Service Unavailable",
});

export function statusPhrase(status?: number | null): string {
  if (status == null || !Number.isFinite(status)) return "";
  return STATUS_PHRASES[status] ?? "";
}

export function statusGroupClass(status?: number | null): string {
  if (status == null || !Number.isFinite(status)) return "is-other";
  if (status >= 100 && status < 200) return "is-1xx";
  if (status >= 200 && status < 300) return "is-2xx";
  if (status >= 300 && status < 400) return "is-3xx";
  if (status >= 400 && status < 500) return "is-4xx";
  if (status >= 500 && status < 600) return "is-5xx";
  return "is-other";
}
