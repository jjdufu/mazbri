import { MAX_TABLE_BROWSE_TOTAL } from "./paginationLimits";

/** One page from a list API that already returns honest pagination.total. */
export type PageFetchResult<T> = {
  items: T[];
  total: number;
};

export type FetchAllPagesOptions = {
  /** Per-request page size; must stay within backend MAX_PAGE_SIZE. Default 100. */
  pageSize?: number;
  /** Hard ceiling so import/option loads cannot runaway. Default MAX_TABLE_BROWSE_TOTAL. */
  maxTotal?: number;
};

/**
 * Walk list pages with a capped page_size. Replaces the forbidden
 * `page_size=1000` / `10000` "load everything in one shot" pattern.
 */
export async function fetchAllPages<T>(
  fetchPage: (page: number, pageSize: number) => Promise<PageFetchResult<T>>,
  options?: FetchAllPagesOptions,
): Promise<T[]> {
  const pageSize = Math.max(1, options?.pageSize ?? 100);
  const maxTotal = Math.max(0, options?.maxTotal ?? MAX_TABLE_BROWSE_TOTAL);
  const first = await fetchPage(1, pageSize);
  const total = Math.min(Math.max(0, first.total), maxTotal);
  if (total === 0) {
    return [];
  }
  const items = [...first.items];
  const totalPages = Math.max(1, Math.ceil(total / pageSize));
  for (let page = 2; page <= totalPages; page += 1) {
    const res = await fetchPage(page, pageSize);
    items.push(...res.items);
    if (items.length >= total) {
      break;
    }
  }
  return items.slice(0, total);
}
