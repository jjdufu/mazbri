import { hash } from "ohash";
import { logger } from "@/utils/sys";
export enum CacheInvalidationStrategy {
  CLEAR_ALL = "clear_all",
  CLEAR_CURRENT = "clear_current",
  CLEAR_PAGINATION = "clear_pagination",
  KEEP_ALL = "keep_all",
}
export interface ApiResponse<T = unknown> {
  records?: T[];
  data?: T[];
  total?: number;
  current?: number;
  size?: number;
  [key: string]: unknown;
}
export interface CacheItem<T> {
  data: T[];
  response: ApiResponse<T>;
  timestamp: number;
  params: string;
  tags: Set<string>;
  accessCount: number;
  lastAccessTime: number;
}
export class TableCache<T> {
  private cache = new Map<string, CacheItem<T>>();
  private cacheTime: number;
  private maxSize: number;
  private enableLog: boolean;
  constructor(cacheTime = 5 * 60 * 1000, maxSize = 50, enableLog = false) {
    this.cacheTime = cacheTime;
    this.maxSize = maxSize;
    this.enableLog = enableLog;
  }
  private log(message: string, ...args: unknown[]) {
    void message;
    void args;
  }
  private generateKey(params: unknown): string {
    return hash(params);
  }
  private generateTags(params: Record<string, unknown>): Set<string> {
    const tags = new Set<string>();
    const searchKeys = Object.keys(params).filter(
      (key) =>
        !["current", "size", "total"].includes(key) &&
        params[key] !== undefined &&
        params[key] !== "" &&
        params[key] !== null,
    );
    if (searchKeys.length > 0) {
      const searchTag = searchKeys
        .map((key) => `${key}:${String(params[key])}`)
        .join("|");
      tags.add(`search:${searchTag}`);
    } else {
      tags.add("search:default");
    }
    tags.add(`pagination:${params.size || 10}`);
    tags.add("pagination");
    return tags;
  }
  private evictLRU(): void {
    if (this.cache.size <= this.maxSize) return;
    let lruKey = "";
    let minAccessCount = Infinity;
    let oldestTime = Infinity;
    for (const [key, item] of this.cache.entries()) {
      if (
        item.accessCount < minAccessCount ||
        (item.accessCount === minAccessCount &&
          item.lastAccessTime < oldestTime)
      ) {
        lruKey = key;
        minAccessCount = item.accessCount;
        oldestTime = item.lastAccessTime;
      }
    }
    if (lruKey) {
      this.cache.delete(lruKey);
      this.log(`LRU evicted cache entry: ${lruKey}`);
    }
  }
  set(params: unknown, data: T[], response: ApiResponse<T>): void {
    const key = this.generateKey(params);
    const tags = this.generateTags(params as Record<string, unknown>);
    const now = Date.now();
    this.evictLRU();
    this.cache.set(key, {
      data,
      response,
      timestamp: now,
      params: key,
      tags,
      accessCount: 1,
      lastAccessTime: now,
    });
  }
  get(params: unknown): CacheItem<T> | null {
    const key = this.generateKey(params);
    const item = this.cache.get(key);
    if (!item) return null;
    if (Date.now() - item.timestamp > this.cacheTime) {
      this.cache.delete(key);
      return null;
    }
    item.accessCount++;
    item.lastAccessTime = Date.now();
    return item;
  }
  clearByTags(tags: string[]): number {
    let clearedCount = 0;
    for (const [key, item] of this.cache.entries()) {
      const hasMatchingTag = tags.some((tag) =>
        Array.from(item.tags).some((itemTag) => itemTag.includes(tag)),
      );
      if (hasMatchingTag) {
        this.cache.delete(key);
        clearedCount++;
      }
    }
    return clearedCount;
  }
  clearCurrentSearch(params: unknown): number {
    const key = this.generateKey(params);
    const deleted = this.cache.delete(key);
    return deleted ? 1 : 0;
  }
  clearPagination(): number {
    return this.clearByTags(["pagination"]);
  }
  clear(): void {
    this.cache.clear();
  }
  getStats(): {
    total: number;
    size: string;
    hitRate: string;
  } {
    const total = this.cache.size;
    let totalSize = 0;
    let totalAccess = 0;
    for (const item of this.cache.values()) {
      totalSize += JSON.stringify(item.data).length;
      totalAccess += item.accessCount;
    }
    const sizeInKB = (totalSize / 1024).toFixed(2);
    const avgHits = total > 0 ? (totalAccess / total).toFixed(1) : "0";
    return {
      total,
      size: `${sizeInKB}KB`,
      hitRate: `${avgHits} avg hits`,
    };
  }
  cleanupExpired(): number {
    let cleanedCount = 0;
    const now = Date.now();
    for (const [key, item] of this.cache.entries()) {
      if (now - item.timestamp > this.cacheTime) {
        this.cache.delete(key);
        cleanedCount++;
      }
    }
    return cleanedCount;
  }
}
