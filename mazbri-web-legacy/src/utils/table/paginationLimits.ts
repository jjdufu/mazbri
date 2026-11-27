




export const MAX_TABLE_BROWSE_TOTAL = 20_000;

export function resolveSafePageCount(
  total: number | undefined,
  size: number | undefined,
  pageCount?: number,
): number {
  const safeSize = Math.max(1, size || 1);
  const safeTotal = Math.max(0, total ?? 0);
  const fromTotal =
    safeTotal > 0
      ? Math.max(
          1,
          Math.ceil(Math.min(safeTotal, MAX_TABLE_BROWSE_TOTAL) / safeSize),
        )
      : 1;



  if (
    typeof pageCount === "number" &&
    Number.isFinite(pageCount) &&
    pageCount > 0 &&
    safeTotal > 0
  ) {
    return Math.min(Math.floor(pageCount), fromTotal);
  }

  if (
    typeof pageCount === "number" &&
    Number.isFinite(pageCount) &&
    pageCount > 0 &&
    safeTotal <= 0
  ) {
    return Math.floor(pageCount);
  }

  return fromTotal;
}

export function clampPageToSafeRange(
  page: number,
  total: number | undefined,
  size: number | undefined,
  pageCount?: number,
): number {
  const maxPage = resolveSafePageCount(total, size, pageCount);
  if (!Number.isFinite(page) || page < 1) {
    return 1;
  }
  return Math.min(Math.floor(page), maxPage);
}
