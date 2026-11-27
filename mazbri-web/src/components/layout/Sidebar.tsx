import { useEffect, useMemo, useState } from "react";
import { Link, useLocation } from "react-router";
import { PanelLeftClose, PanelLeftOpen, Shield } from "lucide-react";
import { Tooltip, cn } from "@/components/ui";
import { LanguageEnum } from "@/enums/appEnum";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import { useWatermarkStore } from "@/stores/watermark";
import {
  getFilteredMenuGroups,
  resolveActiveMenuPath,
  resolveDefaultHomePath,
} from "./menuConfig";

const SIDEBAR_COLLAPSED_KEY = "mazbri-sidebar-collapsed";

function readInitialCollapsed(): boolean {
  if (typeof localStorage === "undefined") return false;
  try {
    return localStorage.getItem(SIDEBAR_COLLAPSED_KEY) === "1";
  } catch {
    return false;
  }
}

export function Sidebar() {
  const location = useLocation();
  const { t, locale } = useT();
  const role = useAuthStore((s) => s.info.role);
  const siteName = useWatermarkStore((s) => s.siteName);
  const [collapsed, setCollapsed] = useState<boolean>(readInitialCollapsed);

  useEffect(() => {
    if (typeof localStorage === "undefined") return;
    try {
      localStorage.setItem(SIDEBAR_COLLAPSED_KEY, collapsed ? "1" : "0");
    } catch {
      // Ignore storage errors in restricted contexts.
    }
  }, [collapsed]);

  const menuGroups = useMemo(() => getFilteredMenuGroups(role), [role]);
  const activePath = useMemo(
    () => resolveActiveMenuPath(location.pathname),
    [location.pathname],
  );
  const homePath = useMemo(() => resolveDefaultHomePath(role), [role]);
  const brandTitle = siteName.trim() || "Mazbri";
  const toggleLabel = collapsed
    ? locale === LanguageEnum.ZH
      ? "展开侧栏"
      : "Expand sidebar"
    : locale === LanguageEnum.ZH
      ? "收起侧栏"
      : "Collapse sidebar";

  return (
    <aside
      data-testid="app-sidebar"
      data-collapsed={collapsed ? "true" : "false"}
      className={cn(
        "bg-sidebar-bg text-slate-200 flex flex-col shrink-0 border-r border-white/10 select-none transition-[width] duration-200",
        collapsed ? "w-[60px]" : "w-[220px]",
      )}
    >
      <Link
        to={homePath}
        className={cn(
          "h-12 flex items-center border-b border-white/10 shrink-0 transition-colors hover:bg-sidebar-hover",
          collapsed ? "justify-center px-0" : "gap-2.5 px-3.5",
        )}
      >
        <span className="inline-flex h-7 w-7 items-center justify-center rounded-md bg-sidebar-active text-white shrink-0">
          <Shield className="h-4 w-4" />
        </span>
        {!collapsed && (
          <span className="text-sm font-semibold tracking-tight text-white truncate">
            {brandTitle}
          </span>
        )}
      </Link>

      <nav
        aria-label={t("menus.mazbri.title")}
        className="flex-1 overflow-y-auto py-2 px-2 space-y-1"
      >
        {menuGroups.map((group, groupIndex) => (
          <div
            key={group.key}
            className={cn(groupIndex > 0 && (collapsed ? "pt-2" : "pt-3"))}
          >
            {collapsed ? (
              groupIndex > 0 && (
                <div className="mx-1.5 mb-2 border-t border-white/10" />
              )
            ) : (
              <div className="px-2.5 pb-1 text-[11px] font-medium uppercase tracking-wider text-slate-400 truncate">
                {t(group.titleKey)}
              </div>
            )}

            <ul className="space-y-0.5">
              {group.items.map((item) => {
                const Icon = item.icon;
                const isActive = activePath === item.path;
                const label = t(item.titleKey);
                return (
                  <li key={item.key}>
                    <Tooltip
                      content={label}
                      side="right"
                      disabled={!collapsed}
                    >
                      <Link
                        to={item.path}
                        data-testid={`nav-item-${item.key}`}
                        aria-current={isActive ? "page" : undefined}
                        className={cn(
                          "flex h-8 items-center rounded-md text-xs transition-colors duration-150",
                          collapsed
                            ? "justify-center px-0"
                            : "gap-2.5 px-2.5",
                          isActive
                            ? "bg-sidebar-active text-white font-medium"
                            : "text-slate-300 hover:bg-sidebar-hover hover:text-white",
                        )}
                      >
                        <Icon className="h-4 w-4 shrink-0" />
                        {!collapsed && (
                          <span className="truncate">{label}</span>
                        )}
                      </Link>
                    </Tooltip>
                  </li>
                );
              })}
            </ul>
          </div>
        ))}
      </nav>

      <div className="border-t border-white/10 p-2 shrink-0">
        <Tooltip content={toggleLabel} side="right" disabled={!collapsed}>
          <button
            type="button"
            data-testid="sidebar-collapse-btn"
            aria-label={toggleLabel}
            onClick={() => setCollapsed((prev) => !prev)}
            className={cn(
              "flex h-8 w-full items-center rounded-md text-xs text-slate-300 hover:bg-sidebar-hover hover:text-white transition-colors cursor-pointer",
              collapsed ? "justify-center px-0" : "gap-2 px-2.5",
            )}
          >
            {collapsed ? (
              <PanelLeftOpen className="h-4 w-4 shrink-0" />
            ) : (
              <>
                <PanelLeftClose className="h-4 w-4 shrink-0" />
                <span className="truncate">{toggleLabel}</span>
              </>
            )}
          </button>
        </Tooltip>
      </div>
    </aside>
  );
}
