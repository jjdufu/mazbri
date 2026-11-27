import { useMemo } from "react";
import { Link, useLocation } from "react-router";
import { Tooltip, cn } from "@/components/ui";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import { getFilteredMenuGroups, resolveActiveMenuPath } from "./menuConfig";

export interface SidebarProps {
  collapsed: boolean;
}

export function Sidebar({ collapsed }: SidebarProps) {
  const location = useLocation();
  const { t } = useT();
  const role = useAuthStore((s) => s.info.role);

  const menuGroups = useMemo(() => getFilteredMenuGroups(role), [role]);
  const activePath = useMemo(
    () => resolveActiveMenuPath(location.pathname),
    [location.pathname],
  );

  return (
    <aside
      data-testid="app-sidebar"
      data-collapsed={collapsed ? "true" : "false"}
      className={cn(
        "border-r border-border bg-card/35 flex flex-col justify-between shrink-0 transition-all overflow-y-auto select-none",
        collapsed ? "w-[60px] px-2 py-3" : "w-52 px-2.5 py-3",
      )}
    >
      <nav aria-label={t("menus.mazbri.title")} className="space-y-1">
        {menuGroups.map((group, groupIndex) => (
          <div
            key={group.key}
            className={cn(groupIndex > 0 && (collapsed ? "pt-2" : "pt-3.5"))}
          >
            {collapsed ? (
              groupIndex > 0 && (
                <div className="mx-1.5 mb-2 border-t border-border" />
              )
            ) : (
              <div className="px-2.5 pb-1.5 text-[11px] font-medium text-sidebar-muted truncate">
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
                          "relative w-full flex items-center rounded-md py-1.5 text-[13px] transition-colors cursor-pointer",
                          collapsed
                            ? "justify-center px-0"
                            : "gap-2.5 px-2.5",
                          isActive
                            ? "bg-xblue-soft text-xblue font-semibold before:absolute before:left-0 before:top-1.5 before:bottom-1.5 before:w-[3px] before:rounded-r before:bg-xblue"
                            : "font-medium text-muted-foreground hover:text-foreground hover:bg-hover-bg active:bg-active-bg",
                        )}
                      >
                        <Icon
                          className={cn(
                            "h-4 w-4 shrink-0",
                            isActive && "text-xblue",
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

      {!collapsed && (
        <div className="mt-3 pt-2 border-t border-border/60 text-[11px] text-muted-foreground/70 select-none px-2.5 flex items-center justify-between">
          <span>Mazbri</span>
          <span className="font-mono-tech">v0.6.8</span>
        </div>
      )}
    </aside>
  );
}
