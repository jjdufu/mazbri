




export function parseRequestIdLookup(raw: string | null | undefined): string | null {
  const trimmed = (raw ?? "").trim();
  if (!trimmed) return null;

  const hyphenated =
    /^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$/;
  if (hyphenated.test(trimmed)) {
    return trimmed.toLowerCase();
  }

  const simple = /^[0-9a-fA-F]{32}$/;
  if (simple.test(trimmed)) {
    const s = trimmed.toLowerCase();
    return `${s.slice(0, 8)}-${s.slice(8, 12)}-${s.slice(12, 16)}-${s.slice(16, 20)}-${s.slice(20)}`;
  }


  const embedded = trimmed.match(
    /[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}/g,
  );
  if (embedded?.length === 1) {
    return embedded[0]!.toLowerCase();
  }

  const embeddedSimple = trimmed.match(/\b[0-9a-fA-F]{32}\b/g);
  if (embeddedSimple?.length === 1) {
    const s = embeddedSimple[0]!.toLowerCase();
    return `${s.slice(0, 8)}-${s.slice(8, 12)}-${s.slice(12, 16)}-${s.slice(16, 20)}-${s.slice(20)}`;
  }

  return null;
}


export function requestIdFromFilters(
  filters: Array<{ field: string; op: string; value?: unknown }> | null | undefined,
): string | null {
  if (!filters?.length) return null;
  for (const f of filters) {
    if (f.field !== "request_id" || f.op !== "eq") continue;
    if (typeof f.value === "string") {
      const id = parseRequestIdLookup(f.value);
      if (id) return id;
    }
  }
  return null;
}
