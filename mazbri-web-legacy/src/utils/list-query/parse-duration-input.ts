



export function parseDurationMsInput(raw: string | number | null | undefined): number | null {
  if (raw === null || raw === undefined) return null;
  if (typeof raw === "number") {
    return Number.isFinite(raw) && raw >= 0 ? Math.floor(raw) : null;
  }
  const s = String(raw).trim().toLowerCase();
  if (!s) return null;
  const match = s.match(/^(\d+(?:\.\d+)?)\s*(ms|s|m)?$/);
  if (!match) {
    const n = Number(s);
    return Number.isFinite(n) && n >= 0 ? Math.floor(n) : null;
  }
  const n = Number(match[1]);
  if (!Number.isFinite(n) || n < 0) return null;
  const unit = match[2] ?? "ms";
  if (unit === "ms") return Math.floor(n);
  if (unit === "s") return Math.floor(n * 1000);
  if (unit === "m") return Math.floor(n * 60_000);
  return null;
}
