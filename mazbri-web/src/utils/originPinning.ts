const DEFAULT_PORTS: Record<string, number> = {
  http: 80,
  https: 443,
  ws: 80,
  wss: 443,
};

const SCHEME_RE = /^[a-zA-Z][a-zA-Z0-9+.-]*:\/\//;

export interface CanonicalOriginOptions {
  defaultScheme?: "http" | "https";
}

/**
 * 判断凭据值是否处于后端回显的掩码态（如 `••••••••` 或 `********`）。
 */
export function isMaskedSecretValue(value: string | null | undefined): boolean {
  const trimmed = (value ?? "").trim();
  if (!trimmed) return false;
  if (trimmed === "••••••••" || trimmed === "********") return true;
  return /^•+$/.test(trimmed) && trimmed.length >= 4;
}

/**
 * 提取规范化 Origin（`scheme://host:port`）：
 * - 小写 scheme 与小写 host（保留 IPv6 方括号）
 * - 显式端口或协议默认端口（http/ws=80，https/wss=443）
 * - 路径、查询参数与片段变更不影响规范化 Origin
 */
export function canonicalOrigin(
  rawUrl: string | null | undefined,
  options: CanonicalOriginOptions = {},
): string {
  const trimmed = (rawUrl ?? "").trim();
  if (!trimmed) return "";

  const candidate = SCHEME_RE.test(trimmed)
    ? trimmed
    : options.defaultScheme
      ? `${options.defaultScheme}://${trimmed.replace(/^\/+/, "")}`
      : "";

  if (candidate) {
    try {
      const parsed = new URL(candidate);
      const scheme = parsed.protocol.replace(/:$/, "").toLowerCase();
      const host = parsed.hostname.toLowerCase();
      if (scheme && host) {
        const explicitPort = parsed.port ? Number(parsed.port) : NaN;
        const port =
          Number.isInteger(explicitPort) && explicitPort > 0
            ? explicitPort
            : (DEFAULT_PORTS[scheme] ?? 0);
        return `${scheme}://${host}:${port}`;
      }
    } catch {
      // 含未展开模板占位符等非标准 URL 时回退 authority 段归一化比较
    }
  }

  const withoutScheme = SCHEME_RE.test(trimmed)
    ? trimmed
    : options.defaultScheme
      ? `${options.defaultScheme}://${trimmed.replace(/^\/+/, "")}`
      : trimmed;
  const schemeSep = withoutScheme.indexOf("://");
  if (schemeSep > 0) {
    const scheme = withoutScheme.slice(0, schemeSep).toLowerCase();
    const rest = withoutScheme.slice(schemeSep + 3);
    const authority = (rest.split(/[/?#]/)[0] ?? "").toLowerCase();
    if (!authority) return "";
    const hasExplicitPort = authority.startsWith("[")
      ? /\]:\d+$/.test(authority)
      : /:\d+$/.test(authority);
    if (hasExplicitPort) {
      return `${scheme}://${authority}`;
    }
    const defaultPort = DEFAULT_PORTS[scheme] ?? 0;
    return `${scheme}://${authority}:${defaultPort}`;
  }

  return (withoutScheme.split(/[/?#]/)[0] ?? "").toLowerCase();
}

/**
 * 对比当前地址与初始地址的规范化 Origin 是否发生变更。
 * - 若初始值非空且当前非空且两者规范化 Origin 不同，判定为变更；
 * - 若初始值非空且当前已被清空，或初始值为空但新增了与基准不同的外部地址，按 `treatEmptyAsChanged` 控制。
 */
export function hasOriginChanged(
  initialUrl: string | null | undefined,
  currentUrl: string | null | undefined,
  options: CanonicalOriginOptions & {
    fallbackInitialOrigin?: string;
  } = {},
): boolean {
  const initialOrigin = canonicalOrigin(initialUrl, options);
  const currentOrigin = canonicalOrigin(currentUrl, options);
  if (!initialOrigin && !currentOrigin) return false;
  if (initialOrigin && !currentOrigin) return false;
  if (!initialOrigin && currentOrigin) {
    if (options.fallbackInitialOrigin) {
      return currentOrigin !== options.fallbackInitialOrigin;
    }
    return true;
  }
  return initialOrigin !== currentOrigin;
}

/**
 * 清空列表中处于掩码态的键值项，返回被清空的键名集合（用于标记必填校验）。
 */
export function clearMaskedKvEntries(
  entries: Array<{ key: string; value: string }>,
): string[] {
  const clearedKeys: string[] = [];
  for (const item of entries) {
    if (isMaskedSecretValue(item.value)) {
      item.value = "";
      const key = item.key.trim();
      if (key && !clearedKeys.includes(key)) {
        clearedKeys.push(key);
      }
    }
  }
  return clearedKeys;
}
