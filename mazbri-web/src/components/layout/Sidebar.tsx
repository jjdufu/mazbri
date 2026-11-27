import { useMemo } from "react";
import { Link, useLocation } from "react-router";
import { ChevronsLeft, ChevronsRight } from "lucide-react";
import { IconButton, Tooltip, cn } from "@/components/ui";
import { LanguageEnum } from "@/enums/appEnum";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import {
  getFilteredMenuGroups,
  resolveActiveMenuPath,
  resolveDefaultHomePath,
} from "./menuConfig";

export interface SidebarProps {
  collapsed: boolean;
  onToggleCollapse?: () => void;
}

export function Sidebar({ collapsed, onToggleCollapse }: SidebarProps) {
  const location = useLocation();
  const { t, locale } = useT();
  const role = useAuthStore((s) => s.info.role);

  const menuGroups = useMemo(() => getFilteredMenuGroups(role), [role]);
  const activePath = useMemo(
    () => resolveActiveMenuPath(location.pathname),
    [location.pathname],
  );
  const homePath = useMemo(() => resolveDefaultHomePath(role), [role]);

  const isZh = locale === LanguageEnum.ZH;
  const collapseLabel = collapsed
    ? isZh
      ? "展开导航栏 (⌘B)"
      : "Expand sidebar (⌘B)"
    : isZh
      ? "收起导航栏 (⌘B)"
      : "Collapse sidebar (⌘B)";

  return (
    <aside
      data-testid="app-sidebar"
      data-collapsed={collapsed ? "true" : "false"}
      className={cn(
        "h-full bg-app-frame flex flex-col justify-between shrink-0 transition-all overflow-y-auto select-none",
        collapsed ? "w-[60px] px-2 py-2.5" : "w-52 px-2.5 py-2.5",
      )}
    >
      <div className="space-y-3">
        {/* Sidebar Brand & Minimal Collapse Toggle (Zero border-r, Zero crosshair) */}
        <div
          className={cn(
            "flex items-center select-none",
            collapsed
              ? "flex-col gap-1.5 pt-0.5"
              : "h-9 justify-between pl-1.5 pr-0.5",
          )}
        >
          <Link
            to={homePath}
            className="flex items-center gap-2 min-w-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-xblue/45 rounded-md"
          >
            <span className="inline-flex h-7 w-7 items-center justify-center rounded-md bg-primary text-primary-foreground text-xs font-bold tracking-tight shadow-2xs shrink-0">
              M
            </span>
            {!collapsed && (
              <span className="text-[15px] font-semibold tracking-tight text-foreground truncate">
                Mazbri
              </span>
            )}
          </Link>

          {onToggleCollapse && (
            <Tooltip content={collapseLabel} side="right">
              <IconButton
                size="sm"
                onClick={onToggleCollapse}
                aria-label={collapseLabel}
                data-testid="sidebar-collapse-btn"
                className="text-muted-foreground/85 hover:text-foreground"
              >
                {collapsed ? (
                  <ChevronsRight className="h-4 w-4" />
                ) : (
                  <ChevronsLeft className="h-4 w-4" />
                )}
              </IconButton>
            </Tooltip>
          )}
        </div>

        <nav aria-label={t("menus.mazbri.title")} className="space-y-0.5">
          {menuGroups.map((group, groupIndex) => (
            <div
              key={group.key}
              className={cn(groupIndex > 0 && (collapsed ? "pt-2.5" : "pt-3"))}
            >
              {!collapsed && (
                <div className="px-2.5 pb-1 text-[11px] font-medium text-sidebar-muted tracking-wide truncate">
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
                            "group relative w-full flex items-center rounded-md py-1.5 text-[13px] transition-all duration-150 ease-[cubic-bezier(0.16,1,0.3,1)] cursor-pointer",
                            collapsed
                              ? "justify-center px-0"
                              : "gap-2.5 px-2.5",
                            isActive
                              ? "bg-background text-xblue font-semibold shadow-[0_1px_2px_rgba(15,23,42,0.07),0_0_0_1px_var(--border-subtle)] dark:bg-card before:absolute before:left-1 before:top-1.5 before:bottom-1.5 before:w-[2.5px] before:rounded-full before:bg-primary"
                              : "font-medium text-muted-foreground hover:text-foreground hover:bg-nav-hover-bg active:bg-active-bg active:scale-[0.98]",
                          )}
                        >
                          <Icon
                            className={cn(
                              "h-4 w-4 shrink-0 transition-all duration-150",
                              isActive
                                ? "text-xblue"
                                : "text-muted-foreground group-hover:text-xblue group-hover:scale-108",
                            )}
                          />
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
      </div>

      <div className="mt-3 pt-2">
        <div
          title="Mazbri v0.6.8"
          className={cn(
            "text-[11px] text-muted-foreground/65 select-none",
            collapsed
              ? "flex items-center justify-center font-mono-tech"
              : "px-2.5 flex items-center justify-between",
          )}
        >
          {collapsed ? (
            <span className="text-[10px] font-mono-tech tracking-tight">
              v0.6
            </span>
          ) : (
            <>
              <span>Mazbri</span>
              <span className="font-mono-tech">v0.6.8</span>
            </>
          )}
        </div>
      </div>
    </aside>
  );
}
