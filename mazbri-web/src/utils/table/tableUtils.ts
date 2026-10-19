import type { ApiResponse } from "./tableCache";
import { tableConfig } from "./tableConfig";
import { clampPageToSafeRange } from "./paginationLimits";
import { logger } from "@/utils/sys";
import { $t } from "@/locales";
export interface BaseRequestParams extends Api.Common.PaginationParams {
  [key: string]: unknown;
}
export interface TableError {
  code: string;
  message: string;
  details?: unknown;
}
export const syncPaginationKeys = (
  paramsRecord: Record<string, unknown>,
  pageKey: string,
  sizeKey: string,
  current: number,
  size: number,
): void => {
  if (paramsRecord[pageKey] !== current) {
    paramsRecord[pageKey] = current;
  }
  if (paramsRecord[sizeKey] !== size) {
    paramsRecord[sizeKey] = size;
  }
};
export const buildTableRequestParams = <
  TParams extends Record<string, unknown>,
>(options: {
  searchParams: TParams;
  pageKey: string;
  sizeKey: string;
  current: number;
  size: number;
  params?: Partial<TParams>;
  excludeParams?: string[];
}): TParams => {
  let requestParams = Object.assign(
    {},
    options.searchParams,
    {
      [options.pageKey]: options.current,
      [options.sizeKey]: options.size,
    },
    options.params || {},
  ) as TParams;
  if (!options.excludeParams?.length) {
    return requestParams;
  }
  const filteredParams = { ...requestParams };
  options.excludeParams.forEach((key) => {
    delete (filteredParams as Record<string, unknown>)[key];
  });
  requestParams = filteredParams as TParams;
  return requestParams;
};
export const resetTableSearchParams = (
  searchParams: Record<string, unknown>,
  apiParams: Record<string, unknown>,
  pageKey: string,
  sizeKey: string,
): {
  current: number;
  size: number;
} => {
  const defaultPagination = {
    [pageKey]: 1,
    [sizeKey]: (searchParams[sizeKey] as number) || 10,
  };
  Object.keys(searchParams).forEach((key) => {
    delete searchParams[key];
  });
  Object.assign(searchParams, apiParams, defaultPagination);
  return {
    current: defaultPagination[pageKey] as number,
    size: defaultPagination[sizeKey] as number,
  };
};
export const applyPageSizeChange = (
  searchParams: Record<string, unknown>,
  pagination: Api.Common.PaginationParams,
  pageKey: string,
  sizeKey: string,
  newSize: number,
): void => {
  pagination.size = newSize;
  pagination.current = 1;
  searchParams[sizeKey] = newSize;
  searchParams[pageKey] = 1;
};
export const shouldSkipCurrentPageChange = (
  isCurrentChanging: boolean,
  currentPage: number,
  nextPage: number,
): boolean => {
  if (isCurrentChanging) {
    return true;
  }
  return currentPage === nextPage;
};
function extractRecords<T>(
  obj: Record<string, unknown>,
  fields: string[],
): T[] {
  for (const field of fields) {
    if (field in obj && Array.isArray(obj[field])) {
      return obj[field] as T[];
    }
  }
  return [];
}
function extractTotal(
  obj: Record<string, unknown>,
  records: unknown[],
  fields: string[],
): number {
  for (const field of fields) {
    if (field in obj && typeof obj[field] === "number") {
      return obj[field] as number;
    }
  }
  return records.length;
}
function extractPagination(
  obj: Record<string, unknown>,
  data?: Record<string, unknown>,
): Pick<ApiResponse<unknown>, "current" | "size"> | undefined {
  const result: Partial<Pick<ApiResponse<unknown>, "current" | "size">> = {};
  const sources = [obj, data ?? {}];
  const currentFields = tableConfig.currentFields;
  for (const src of sources) {
    for (const field of currentFields) {
      if (field in src && typeof src[field] === "number") {
        result.current = src[field] as number;
        break;
      }
    }
    if (result.current !== undefined) break;
  }
  const sizeFields = tableConfig.sizeFields;
  for (const src of sources) {
    for (const field of sizeFields) {
      if (field in src && typeof src[field] === "number") {
        result.size = src[field] as number;
        break;
      }
    }
    if (result.size !== undefined) break;
  }
  if (result.current === undefined && result.size === undefined)
    return undefined;
  return result;
}
export const defaultResponseAdapter = <T>(
  response: unknown,
): ApiResponse<T> => {
  const recordFields = tableConfig.recordFields;
  if (!response) {
    return { records: [], total: 0 };
  }
  if (Array.isArray(response)) {
    return { records: response, total: response.length };
  }
  if (typeof response !== "object") {
    logger.warn(
      "[tableUtils] Unrecognized response format. Supported formats include arrays, objects with " +
        recordFields.join("/") +
        " fields, and nested data objects. Current format:",
    );
    return { records: [], total: 0 };
  }
  const res = response as Record<string, unknown>;
  let records: T[] = [];
  let total = 0;
  let pagination: Pick<ApiResponse<unknown>, "current" | "size"> | undefined;
  records = extractRecords(res, recordFields);
  total = extractTotal(res, records, tableConfig.totalFields);
  pagination = extractPagination(res);
  if (records.length === 0 && "data" in res && typeof res.data === "object") {
    const data = res.data as Record<string, unknown>;
    records = extractRecords(data, ["list", "records", "items"]);
    total = extractTotal(data, records, tableConfig.totalFields);
    pagination = extractPagination(res, data);
    if (Array.isArray(res.data)) {
      records = res.data as T[];
      total = records.length;
    }
  }
  if (!recordFields.some((field) => field in res) && records.length === 0) {
    logger.warn("[tableUtils] Unrecognized response format");
    logger.warn("[tableUtils] Supported fields: " + recordFields.join(", "));
    logger.warn("[tableUtils] Add more fields in utils/table/tableConfig");
  }
  const result: ApiResponse<T> = { records, total };
  if (pagination) {
    Object.assign(result, pagination);
  }
  return result;
};
export const extractTableData = <T>(response: ApiResponse<T>): T[] => {
  const data = response.records || response.data || [];
  return Array.isArray(data) ? data : [];
};
export const updatePaginationFromResponse = <T>(
  pagination: Api.Common.PaginationParams,
  response: ApiResponse<T>,
): void => {
  pagination.total = response.total ?? pagination.total ?? 0;
  if (response.current !== undefined) {
    pagination.current = response.current;
  }
  pagination.current = clampPageToSafeRange(
    pagination.current,
    pagination.total,
    pagination.size,
  );
};
export const createSmartDebounce = <TArgs extends unknown[], TResult>(
  fn: (...args: TArgs) => Promise<TResult>,
  delay: number,
): ((...args: TArgs) => Promise<TResult>) & {
  cancel: () => void;
  flush: () => Promise<TResult>;
} => {
  let timeoutId: ReturnType<typeof setTimeout> | null = null;
  let lastArgs: TArgs | null = null;
  let lastResolve: ((value: TResult) => void) | null = null;
  let lastReject: ((reason: unknown) => void) | null = null;
  const debouncedFn = (...args: TArgs): Promise<TResult> => {
    return new Promise((resolve, reject) => {
      if (timeoutId) clearTimeout(timeoutId);
      lastArgs = args;
      lastResolve = resolve;
      lastReject = reject;
      timeoutId = setTimeout(async () => {
        try {
          const result = await fn(...args);
          resolve(result);
        } catch (error) {
          reject(error);
        } finally {
          timeoutId = null;
          lastArgs = null;
          lastResolve = null;
          lastReject = null;
        }
      }, delay);
    });
  };
  debouncedFn.cancel = (): void => {
    if (timeoutId) clearTimeout(timeoutId);
    timeoutId = null;
    lastArgs = null;
    lastResolve = null;
    lastReject = null;
  };
  debouncedFn.flush = async (): Promise<TResult> => {
    if (timeoutId && lastArgs && lastResolve && lastReject) {
      clearTimeout(timeoutId);
      timeoutId = null;
      const args = lastArgs;
      const resolve = lastResolve;
      const reject = lastReject;
      lastArgs = null;
      lastResolve = null;
      lastReject = null;
      try {
        const result = await fn(...args);
        resolve(result);
        return result;
      } catch (error) {
        reject(error);
        throw error;
      }
    }
    return Promise.resolve(undefined as TResult);
  };
  return debouncedFn as ((...args: TArgs) => Promise<TResult>) & {
    cancel: () => void;
    flush: () => Promise<TResult>;
  };
};
export const createErrorHandler = (
  onError?: (error: TableError) => void,
  enableLog: boolean = false,
) => {
  const log = {
    error: (message: string, ..._args: unknown[]) => {
      if (enableLog) logger.error(`[useTable] ${message}`);
    },
  };
  return (err: unknown, context: string): TableError => {
    const tableError: TableError = {
      code: "UNKNOWN_ERROR",
      message: $t("common.feedback.unknownError"),
      details: err,
    };
    if (err instanceof Error) {
      tableError.message = err.message;
      tableError.code = err.name;
    } else if (typeof err === "string") {
      tableError.message = err;
    }
    log.error(`${context}:`, err);
    onError?.(tableError);
    return tableError;
  };
};
