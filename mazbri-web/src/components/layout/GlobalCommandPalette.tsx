import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useNavigate } from "react-router";
import {
  ArrowRight,
  Award,
  CheckSquare,
  FileCode2,
  Languages,
  Lock,
  Network,
  Plug,
  ReceiptText,
  Search,
  ShieldCheck,
  SunMoon,
  X,
  type LucideIcon,
} from "lucide-react";
import { cn } from "@/components/ui";
import { LanguageEnum } from "@/enums/appEnum";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import { useLockScreenStore } from "@/stores/lockscreen";
import { usePersonalSecurityStore } from "@/stores/personal-security";
import { useThemeStore } from "@/stores/theme";
import type { AdminRole } from "@/types/api";
import {
  ROLES_ADMIN,
  ROLES_ALL,
  ROLES_BIZ,
  canAccessRoles,
} from "@/utils/ops/roles";
import { getFilteredMenuGroups } from "./menuConfig";

export interface GlobalCommandPaletteProps {
  readonly open: boolean;
  readonly onOpenChange: (open: boolean) => void;
}

interface PaletteEntry {
  readonly id: string;
  readonly group: "nav" | "sub" | "action";
  readonly label: string;
  readonly hint?: string;
  readonly keywords: readonly string[];
  readonly icon: LucideIcon;
  readonly path?: string;
  readonly action?: "theme" | "locale" | "lock" | "security";
}

interface SubViewSpec {
  readonly id: string;
  readonly zh: string;
  readonly en: string;
  readonly path: string;
  readonly icon: LucideIcon;
  readonly roles: readonly AdminRole[];
  readonly keywords: readonly string[];
}

const SUB_VIEW_SPECS: readonly SubViewSpec[] = [
  {
    id: "sub-authorizations",
    zh: "产品与授权 · 授权关系列表",
    en: "Products & Auth · Authorizations",
    path: "/operations/product-authorization?tab=authorizations",
    icon: ShieldCheck,
    roles: ROLES_ALL,
    keywords: ["authorization", "grant", "tenant", "授权", "开通"],
  },
  {
    id: "sub-channels",
    zh: "上游供给 · 上游通道列表",
    en: "Upstream Supply · Channels",
    path: "/operations/upstream-supply?tab=channels",
    icon: Network,
    roles: ROLES_ALL,
    keywords: ["channel", "upstream", "route", "通道", "上游"],
  },
  {
    id: "sub-drafts",
    zh: "上游供给 · 连接器草稿",
    en: "Upstream Supply · Connector Drafts",
    path: "/operations/upstream-supply?tab=drafts",
    icon: FileCode2,
    roles: ROLES_ALL,
    keywords: ["draft", "connector", "草稿", "连接器"],
  },
  {
    id: "sub-workbench",
    zh: "连接器调试工作台",
    en: "Connector Workbench",
    path: "/operations/upstream-supply/workbench",
    icon: Plug,
    roles: ROLES_ALL,
    keywords: ["workbench", "connector", "debug", "http", "工作台", "调试"],
  },
  {
    id: "sub-bills-list",
    zh: "账单与对账 · 账单列表",
    en: "Bills & Reconciliation · Bills",
    path: "/operations/bills?tab=bills",
    icon: ReceiptText,
    roles: ROLES_BIZ,
    keywords: ["bill", "invoice", "finance", "账单", "财务"],
  },
  {
    id: "sub-reconcile",
    zh: "账单与对账 · 用量对账",
    en: "Bills & Reconciliation · Reconciliation",
    path: "/operations/bills?tab=reconcile",
    icon: CheckSquare,
    roles: ROLES_BIZ,
    keywords: ["reconcile", "discrepancy", "audit", "对账", "差异"],
  },
  {
    id: "sub-security-policy",
    zh: "平台设置 · 安全策略",
    en: "Platform Settings · Security Policy",
    path: "/control-panel/system?tab=security",
    icon: ShieldCheck,
    roles: ROLES_ADMIN,
    keywords: ["password", "mfa", "totp", "policy", "密码策略", "安全策略"],
  },
  {
    id: "sub-platform-license",
    zh: "平台设置 · 授权许可",
    en: "Platform Settings · License",
    path: "/control-panel/system?tab=license",
    icon: Award,
    roles: ROLES_ADMIN,
    keywords: ["license", "jws", "root", "许可证", "授权包"],
  },
];

function scorePaletteEntry(entry: PaletteEntry, rawQuery: string): number {
  const q = rawQuery.trim().toLowerCase();
  if (!q) return 1;
  const haystacks = [
    entry.label,
    entry.hint ?? "",
    entry.path ?? "",
    ...entry.keywords,
  ].map((s) => s.toLowerCase());
  for (const h of haystacks) {
    if (!h) continue;
    if (h === q) return 1;
    if (h.startsWith(q)) return 0.96;
    if (h.includes(q)) return 0.88;
  }
  const combined = haystacks.join(" ");
  let qi = 0;
  for (let i = 0; i < combined.length && qi < q.length; i += 1) {
    if (combined[i] === q[qi]) {
      qi += 1;
    }
  }
  return qi === q.length ? 0.52 : 0;
}

export function GlobalCommandPalette({
  open,
  onOpenChange,
}: GlobalCommandPaletteProps) {
  const navigate = useNavigate();
  const { t, locale, toggleLocale } = useT();
  const isZh = locale === LanguageEnum.ZH;
  const role = useAuthStore((s) => s.info.role);
  const toggleTheme = useThemeStore((s) => s.toggleTheme);
  const openLockDialog = useLockScreenStore((s) => s.openLockDialog);
  const openSecurityDrawer = usePersonalSecurityStore((s) => s.openDrawer);

  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const listRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;
    setQuery("");
    setActiveIndex(0);
    const id = window.requestAnimationFrame(() => {
      inputRef.current?.focus();
    });
    return () => window.cancelAnimationFrame(id);
  }, [open]);

  const allEntries = useMemo<readonly PaletteEntry[]>(() => {
    const groups = getFilteredMenuGroups(role);
    const navEntries: PaletteEntry[] = [];
    for (const group of groups) {
      const groupTitle = t(group.titleKey);
      for (const item of group.items) {
        const label = t(item.titleKey);
        navEntries.push({
          id: `nav-${item.key}`,
          group: "nav",
          label,
          hint: groupTitle,
          keywords: [item.key, item.path, groupTitle, label],
          icon: item.icon,
          path: item.path,
        });
      }
    }

    const subEntries: PaletteEntry[] = SUB_VIEW_SPECS.filter((spec) =>
      canAccessRoles(role, spec.roles),
    ).map((spec) => ({
      id: spec.id,
      group: "sub",
      label: isZh ? spec.zh : spec.en,
      hint: spec.path,
      keywords: [spec.zh, spec.en, ...spec.keywords],
      icon: spec.icon,
      path: spec.path,
    }));

    const actionEntries: PaletteEntry[] = [
      {
        id: "act-theme",
        group: "action",
        label: isZh ? "切换亮暗主题" : "Toggle Theme",
        keywords: ["theme", "dark", "light", "auto", "主题", "暗色", "亮色"],
        icon: SunMoon,
        action: "theme",
      },
      {
        id: "act-locale",
        group: "action",
        label: isZh ? "切换中英语言" : "Switch Language",
        keywords: ["locale", "language", "english", "chinese", "语言", "中英"],
        icon: Languages,
        action: "locale",
      },
      {
        id: "act-security",
        group: "action",
        label: t("accountSecurity.drawer.title"),
        keywords: ["security", "password", "mfa", "totp", "账号安全", "密码"],
        icon: ShieldCheck,
        action: "security",
      },
      {
        id: "act-lock",
        group: "action",
        label: t("topBar.user.lockScreen"),
        keywords: ["lock", "screen", "锁屏", "锁定"],
        icon: Lock,
        action: "lock",
      },
    ];

    return [...navEntries, ...subEntries, ...actionEntries];
  }, [isZh, role, t]);

  const filtered = useMemo<readonly PaletteEntry[]>(() => {
    const scored = allEntries
      .map((entry) => ({ entry, score: scorePaletteEntry(entry, query) }))
      .filter((item) => item.score > 0);
    if (query.trim()) {
      scored.sort((a, b) => b.score - a.score);
    }
    return scored.map((item) => item.entry);
  }, [allEntries, query]);

  useEffect(() => {
    setActiveIndex(0);
  }, [query]);

  useEffect(() => {
    if (!open || !listRef.current) return;
    const activeEl = listRef.current.querySelector<HTMLElement>(
      `[data-cmdk-idx="${activeIndex}"]`,
    );
    activeEl?.scrollIntoView({ block: "nearest" });
  }, [activeIndex, open]);

  if (!open || typeof document === "undefined") {
    return null;
  }

  const executeEntry = (entry: PaletteEntry) => {
    onOpenChange(false);
    if (entry.action === "theme") {
      toggleTheme();
      return;
    }
    if (entry.action === "locale") {
      toggleLocale();
      return;
    }
    if (entry.action === "lock") {
      openLockDialog();
      return;
    }
    if (entry.action === "security") {
      openSecurityDrawer("mfa");
      return;
    }
    if (entry.path) {
      void navigate(entry.path);
    }
  };

  const navGroup = filtered.filter((e) => e.group === "nav");
  const subGroup = filtered.filter((e) => e.group === "sub");
  const actGroup = filtered.filter((e) => e.group === "action");

  let runningIndex = -1;
  const renderSection = (heading: string, items: readonly PaletteEntry[]) => {
    if (items.length === 0) return null;
    return (
      <div className="space-y-0.5">
        <div className="px-2.5 pt-2 pb-1 text-[11px] font-semibold text-muted-foreground select-none">
          {heading}
        </div>
        {items.map((entry) => {
          runningIndex += 1;
          const itemIdx = runningIndex;
          const selected = itemIdx === activeIndex;
          const Icon = entry.icon;
          return (
            <button
              key={entry.id}
              type="button"
              data-cmdk-idx={itemIdx}
              data-testid={`cmdk-item-${entry.id}`}
              onMouseEnter={() => setActiveIndex(itemIdx)}
              onClick={() => executeEntry(entry)}
              className={cn(
                "flex w-full items-center justify-between gap-2 rounded-md px-2.5 py-2 text-[13px] transition-colors cursor-pointer select-none text-left",
                selected
                  ? "bg-xblue-soft text-xblue font-medium"
                  : "text-foreground hover:bg-hover-bg",
              )}
            >
              <span className="flex items-center gap-2 min-w-0 truncate">
                <Icon className="h-3.5 w-3.5 shrink-0 text-xblue" />
                <span className="truncate">{entry.label}</span>
                {entry.hint && (
                  <span className="font-mono-tech text-[11px] text-muted-foreground truncate">
                    {entry.hint}
                  </span>
                )}
              </span>
              <ArrowRight className="h-3.5 w-3.5 opacity-60 shrink-0" />
            </button>
          );
        })}
      </div>
    );
  };

  return createPortal(
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/45 pt-[13vh] px-4 backdrop-blur-xs"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) {
          onOpenChange(false);
        }
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={isZh ? "全局命令面板" : "Command Palette"}
        data-testid="global-cmdk-dialog"
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.preventDefault();
            onOpenChange(false);
            return;
          }
          if (filtered.length === 0) return;
          if (e.key === "ArrowDown") {
            e.preventDefault();
            setActiveIndex((prev) => (prev + 1) % filtered.length);
          } else if (e.key === "ArrowUp") {
            e.preventDefault();
            setActiveIndex(
              (prev) => (prev - 1 + filtered.length) % filtered.length,
            );
          } else if (e.key === "Enter") {
            e.preventDefault();
            const picked = filtered[activeIndex] ?? filtered[0];
            if (picked) executeEntry(picked);
          }
        }}
        className="w-full max-w-xl overflow-hidden rounded-lg border border-border bg-card text-card-foreground shadow-2xl"
      >
        <div className="flex items-center gap-2.5 border-b border-border px-3.5 py-2.5">
          <Search className="h-4 w-4 text-xblue shrink-0" />
          <input
            ref={inputRef}
            type="text"
            data-testid="global-cmdk-input"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={isZh ? "搜索…" : "Search…"}
            className="flex-1 bg-transparent text-[13px] text-foreground placeholder:text-muted-foreground focus:outline-none"
          />
          <button
            type="button"
            onClick={() => onOpenChange(false)}
            aria-label={t("common.actions.close")}
            className="rounded-md p-1 text-muted-foreground hover:bg-hover-bg hover:text-foreground cursor-pointer"
          >
            <X className="h-3.5 w-3.5" />
          </button>
        </div>

        <div
          ref={listRef}
          data-testid="global-cmdk-list"
          className="max-h-96 overflow-y-auto p-1.5 pb-2.5 space-y-1"
        >
          {filtered.length === 0 ? (
            <div className="py-8 text-center text-xs text-muted-foreground">
              {isZh ? "无匹配项" : "No matching items"}
            </div>
          ) : (
            <>
              {renderSection(
                isZh ? "导航页面" : "Navigation",
                navGroup,
              )}
              {renderSection(
                isZh ? "子视图与工作台" : "Views & Workbench",
                subGroup,
              )}
              {renderSection(
                isZh ? "快捷动作" : "Quick Actions",
                actGroup,
              )}
            </>
          )}
        </div>

        <div className="flex items-center justify-between border-t border-border bg-muted/20 px-3 py-1.5 text-[11px] text-muted-foreground select-none">
          <div className="flex items-center gap-3">
            <span>
              <kbd className="rounded border border-border bg-card px-1 py-0.5 font-mono-tech text-[10px]">
                ↑↓
              </kbd>{" "}
              {isZh ? "选择" : "Navigate"}
            </span>
            <span>
              <kbd className="rounded border border-border bg-card px-1 py-0.5 font-mono-tech text-[10px]">
                ↵
              </kbd>{" "}
              {isZh ? "打开" : "Open"}
            </span>
          </div>
          <span>
            <kbd className="rounded border border-border bg-card px-1 py-0.5 font-mono-tech text-[10px]">
              Esc
            </kbd>{" "}
            {isZh ? "关闭" : "Close"}
          </span>
        </div>
      </div>
    </div>,
    document.body,
  );
}
