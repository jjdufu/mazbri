










const DATETIME_RE =
  /^(\d{4})-(\d{2})-(\d{2})(?:[ T](\d{2}):(\d{2})(?::(\d{2})(?:\.(\d{1,9}))?)?)?$/;


function hasUnsafeChars(s: string): boolean {
  if (/['";]/.test(s)) return true;
  for (let i = 0; i < s.length; i++) {
    if (s.charCodeAt(i) < 0x20) return true;
  }
  return false;
}

function pad2(n: number): string {
  return String(n).padStart(2, "0");
}

function pad3FromFraction(frac: string | undefined): string {
  if (!frac) return "000";
  const digits = frac.replace(/\D/g, "").slice(0, 3);
  return digits.padEnd(3, "0");
}





function formatNaive(
  y: number,
  mo: number,
  d: number,
  h = 0,
  mi = 0,
  s = 0,
  ms = 0,
): string {
  return `${y}-${pad2(mo)}-${pad2(d)} ${pad2(h)}:${pad2(mi)}:${pad2(s)}.${String(ms).padStart(3, "0")}`;
}

function parseUnixLiteral(s: string): string | null {
  if (!/^-?\d+$/.test(s)) return null;
  const n = Number(s);
  if (!Number.isFinite(n)) return null;

  const secs = Math.abs(n) > 1_000_000_000_000 ? Math.trunc(n / 1000) : Math.trunc(n);
  if (secs <= 0) return null;
  const date = new Date(secs * 1000);
  if (Number.isNaN(date.getTime())) return null;

  const y = date.getUTCFullYear();
  const mo = date.getUTCMonth() + 1;
  const d = date.getUTCDate();
  const h = date.getUTCHours();
  const mi = date.getUTCMinutes();
  const sec = date.getUTCSeconds();
  const ms = date.getUTCMilliseconds();
  return formatNaive(y, mo, d, h, mi, sec, ms);
}

function parseNaiveWallClock(s: string): string | null {
  const m = s.match(DATETIME_RE);
  if (!m) return null;
  const y = Number(m[1]);
  const mo = Number(m[2]);
  const d = Number(m[3]);
  const hasTime = m[4] !== undefined;
  const h = hasTime ? Number(m[4]) : 0;
  const mi = hasTime ? Number(m[5]) : 0;
  const sec = m[6] !== undefined ? Number(m[6]) : 0;
  const ms =
    m[7] !== undefined ? Number(pad3FromFraction(m[7])) : 0;
  if (
    !Number.isFinite(y) ||
    mo < 1 ||
    mo > 12 ||
    d < 1 ||
    d > 31 ||
    h > 23 ||
    mi > 59 ||
    sec > 59 ||
    ms > 999
  ) {
    return null;
  }

  const probe = Date.UTC(y, mo - 1, d, h, mi, sec, ms);
  const check = new Date(probe);
  if (
    check.getUTCFullYear() !== y ||
    check.getUTCMonth() + 1 !== mo ||
    check.getUTCDate() !== d
  ) {
    return null;
  }
  return formatNaive(y, mo, d, h, mi, sec, ms);
}

function parseIsoWithOffset(s: string): string | null {

  if (!/(?:Z|[+-]\d{2}:?\d{2})$/i.test(s)) return null;
  const date = new Date(s);
  if (Number.isNaN(date.getTime())) return null;
  return formatNaive(
    date.getUTCFullYear(),
    date.getUTCMonth() + 1,
    date.getUTCDate(),
    date.getUTCHours(),
    date.getUTCMinutes(),
    date.getUTCSeconds(),
    date.getUTCMilliseconds(),
  );
}




export function parseDatetimeInput(
  raw: string | number | null | undefined,
): string | null {
  if (raw === null || raw === undefined) return null;
  if (typeof raw === "number") {
    if (!Number.isFinite(raw) || raw <= 0) return null;
    return parseUnixLiteral(String(Math.trunc(raw)));
  }
  const s = String(raw).trim();
  if (!s || hasUnsafeChars(s)) return null;

  const unix = parseUnixLiteral(s);
  if (unix) return unix;

  const iso = parseIsoWithOffset(s);
  if (iso) return iso;


  const naive = parseNaiveWallClock(s.includes("T") ? s.replace("T", " ") : s);
  if (naive) return naive;

  return null;
}
