import React, { useMemo, useState } from "react";
import { Link, useLocation } from "react-router";
import {
  Check,
  ChevronDown,
  ChevronRight,
  Languages,
  Lock,
  LogOut,
  Moon,
  ShieldCheck,
  Sun,
} from "lucide-react";
import {
  AutoThemeIcon,
  Badge,
  ConfirmDialog,
  IconButton,
  Popover,
  Tooltip,
  cn,
} from "@/components/ui";
import { LanguageEnum } from "@/enums/appEnum";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import { useLockScreenStore } from "@/stores/lockscreen";
import { usePersonalSecurityStore } from "@/stores/personal-security";
import { useThemeStore, type ThemeMode } from "@/stores/theme";
import { ROLE_ADMIN, ROLE_FINANCE, ROLE_VIEWER } from "@/utils/ops/roles";
import { resolveBreadcrumbs, resolveMenuPageHeader } from "./menuConfig";

export interface TopbarProps {
  collapsed?: boolean;
  onToggleCollapse?: () => void;
}

export function Topbar(_props: TopbarProps = {}) {
  const location = useLocation();
  const { t, locale, toggleLocale } = useT();
  const info = useAuthStore((s) => s.info);
  const isLoggingOut = useAuthStore((s) => s.isLoggingOut);
  const logOut = useAuthStore((s) => s.logOut);
  const theme = useThemeStore((s) => s.theme);
  const setTheme = useThemeStore((s) => s.setTheme);
  const openLockDialog = useLockScreenStore((s) => s.openLockDialog);
  const openSecurityDrawer = usePersonalSecurityStore((s) => s.openDrawer);

  const [themeMenuOpen, setThemeMenuOpen] = useState(false);
  const [userMenuOpen, setUserMenuOpen] = useState(false);
  const [logoutConfirmOpen, setLogoutConfirmOpen] = useState(false);

  const breadcrumbs = useMemo(
    () => resolveBreadcrumbs(location.pathname, t),
    [location.pathname, t],
  );
  const pageHeader = useMemo(
    () => resolveMenuPageHeader(location.pathname),
    [location.pathname],
  );

  const username = info.username || info.userName || "User";
  const avatarInitial = username.slice(0, 1).toUpperCase();
  const roleLabel =
    info.role === ROLE_ADMIN
      ? t("common.roleLabels.admin")
      : info.role === ROLE_FINANCE
        ? t("common.roleLabels.finance")
        : info.role === ROLE_VIEWER
          ? t("common.roleLabels.viewer")
          : "";

  const isZh = locale === LanguageEnum.ZH;
  const localeTooltip = isZh ? "切换为 English" : "Switch to 中文";

  const themeTooltip =
    theme === "auto"
      ? isZh
        ? "主题：跟随系统"
        : "Theme: System"
      : theme === "dark"
        ? isZh
          ? "主题：深色"
          : "Theme: Dark"
        : isZh
          ? "主题：浅色"
          : "Theme: Light";

  const themeOptions: {
    mode: ThemeMode;
    label: string;
    icon: React.ComponentType<{ className?: string }>;
  }[] = [
    {
      mode: "auto",
      label: isZh ? "跟随系统" : "System",
      icon: AutoThemeIcon,
    },
    {
      mode: "light",
      label: isZh ? "浅色" : "Light",
      icon: Sun,
    },
    {
      mode: "dark",
      label: isZh ? "深色" : "Dark",
      icon: Moon,
    },
  ];

  const handleConfirmLogout = async () => {
    setLogoutConfirmOpen(false);
    await logOut();
  };

  return (
    <>
      <header
        data-testid="app-topbar"
        className="sticky top-0 z-30 h-12 bg-canvas-bg/92 backdrop-blur-md px-5 md:px-6 flex items-center justify-between gap-3 shrink-0"
      >
        {/* Left: Clean Borderless Breadcrumb & Primary Page Title */}
        <nav
          aria-label="Breadcrumb"
          className="flex items-center gap-1.5 min-w-0 truncate"
        >
          <span className="sr-only">Mazbri</span>
          {breadcrumbs.map((crumb, idx) => {
            const isLast = idx === breadcrumbs.length - 1;
            return (
              <React.Fragment key={`${crumb.label}-${idx}`}>
                {idx > 0 && (
                  <ChevronRight
                    aria-hidden="true"
                    className="h-3.5 w-3.5 text-muted-foreground/60 shrink-0"
                  />
                )}
                {isLast ? (
                  pageHeader ? (
                    <h1 className="text-[15px] font-semibold tracking-tight text-foreground truncate">
                      {crumb.label}
                    </h1>
                  ) : (
                    <span className="text-xs font-semibold text-foreground truncate">
                      {crumb.label}
                    </span>
                  )
                ) : crumb.path ? (
                  <Link
                    to={crumb.path}
                    className="text-xs text-muted-foreground hover:text-foreground transition-colors truncate"
                  >
                    {crumb.label}
                  </Link>
                ) : (
                  <span className="text-xs text-muted-foreground truncate hidden sm:inline">
                    {crumb.label}
                  </span>
                )}
              </React.Fragment>
            );
          })}
        </nav>

        {/* Right: Unified Borderless Action Cluster (Lock + Locale + Auto-A Theme + User Pill) */}
        <div className="flex items-center gap-1 shrink-0">
          <Tooltip content={t("topBar.user.lockScreen")} side="bottom">
            <IconButton
              size="md"
              onClick={openLockDialog}
              aria-label={t("topBar.user.lockScreen")}
              data-testid="topbar-lock-btn"
            >
              <Lock className="h-4 w-4 shrink-0" />
            </IconButton>
          </Tooltip>

          <Tooltip content={localeTooltip} side="bottom">
            <IconButton
              size="md"
              onClick={toggleLocale}
              aria-label={localeTooltip}
              data-testid="topbar-locale-btn"
            >
              <Languages className="h-4 w-4 shrink-0" />
            </IconButton>
          </Tooltip>

          <Popover
            open={themeMenuOpen}
            onOpenChange={setThemeMenuOpen}
            align="right"
            contentClassName="w-36 p-1"
            contentTestId="topbar-theme-menu-popover"
            trigger={
              <Tooltip content={themeTooltip} side="bottom" disabled={themeMenuOpen}>
                <IconButton
                  size="md"
                  active={themeMenuOpen}
                  onClick={() => setThemeMenuOpen((prev) => !prev)}
                  aria-label={themeTooltip}
                  data-testid="topbar-theme-btn"
                >
                  {theme === "auto" ? (
                    <AutoThemeIcon className="h-4 w-4 shrink-0" />
                  ) : theme === "dark" ? (
                    <Moon className="h-4 w-4 shrink-0" />
                  ) : (
                    <Sun className="h-4 w-4 shrink-0" />
                  )}
                </IconButton>
              </Tooltip>
            }
          >
            <div className="flex flex-col gap-0.5">
              {themeOptions.map((opt) => {
                const Icon = opt.icon;
                const active = theme === opt.mode;
                return (
                  <button
                    key={opt.mode}
                    type="button"
                    data-testid={`theme-option-${opt.mode}`}
                    onClick={() => {
                      setTheme(opt.mode);
                      setThemeMenuOpen(false);
                    }}
                    className={cn(
                      "flex w-full h-8 items-center justify-between gap-2 rounded-md px-2.5 text-xs transition-colors cursor-pointer text-left",
                      active
                        ? "bg-xblue-soft text-xblue font-medium"
                        : "text-foreground hover:bg-nav-hover-bg",
                    )}
                  >
                    <span className="inline-flex items-center gap-2 min-w-0">
                      <Icon className="h-3.5 w-3.5 shrink-0" />
                      <span className="truncate">{opt.label}</span>
                    </span>
                    {active && <Check className="h-3.5 w-3.5 shrink-0" />}
                  </button>
                );
              })}
            </div>
          </Popover>

          <Popover
            open={userMenuOpen}
            onOpenChange={setUserMenuOpen}
            align="right"
            contentClassName="w-52 p-1"
            contentTestId="topbar-user-menu-popover"
            trigger={
              <button
                type="button"
                data-testid="topbar-user-menu-btn"
                aria-expanded={userMenuOpen}
                aria-haspopup="menu"
                onClick={() => setUserMenuOpen((prev) => !prev)}
                className={cn(
                  "inline-flex h-8 items-center gap-1.5 rounded-md pl-1.5 pr-2 text-xs transition-all duration-150 ease-[cubic-bezier(0.16,1,0.3,1)] cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-xblue/45 ml-0.5 active:scale-[0.95]",
                  userMenuOpen
                    ? "bg-xblue-soft text-xblue"
                    : "text-foreground/82 hover:bg-hover-bg hover:text-foreground active:bg-active-bg",
                )}
              >
                <span className="inline-flex h-6 w-6 items-center justify-center rounded-full bg-xblue-soft text-xblue font-semibold text-[11px] shrink-0">
                  {avatarInitial}
                </span>
                <span className="hidden sm:inline font-medium max-w-[120px] truncate">
                  {username}
                </span>
                <ChevronDown
                  className={cn(
                    "h-3.5 w-3.5 text-muted-foreground shrink-0 transition-transform",
                    userMenuOpen && "rotate-180 text-xblue",
                  )}
                />
              </button>
            }
          >
            <div className="px-2.5 py-2 border-b border-border-subtle mb-1">
              <div className="flex items-center justify-between gap-2">
                <span className="text-xs font-semibold text-foreground truncate">
                  {info.display_name || username}
                </span>
                {roleLabel && (
                  <Badge
                    variant={info.role === ROLE_ADMIN ? "xblue" : "neutral"}
                  >
                    {roleLabel}
                  </Badge>
                )}
              </div>
              <div className="text-[11px] text-muted-foreground truncate mt-0.5">
                {username}
              </div>
            </div>

            <button
              type="button"
              data-testid="user-menu-security-btn"
              onClick={() => {
                setUserMenuOpen(false);
                openSecurityDrawer("mfa");
              }}
              className="flex w-full h-8 items-center gap-2 rounded-md px-2.5 text-xs text-foreground hover:bg-nav-hover-bg transition-colors cursor-pointer text-left"
            >
              <ShieldCheck className="h-3.5 w-3.5 text-xblue shrink-0" />
              <span className="truncate">
                {t("accountSecurity.drawer.title")}
              </span>
            </button>

            <div className="my-1 border-t border-border-subtle" />

            <button
              type="button"
              data-testid="user-menu-logout-btn"
              onClick={() => {
                setUserMenuOpen(false);
                setLogoutConfirmOpen(true);
              }}
              className="flex w-full h-8 items-center gap-2 rounded-md px-2.5 text-xs text-danger-fg hover:bg-[var(--status-err-bg)] transition-colors cursor-pointer text-left"
            >
              <LogOut className="h-3.5 w-3.5 shrink-0" />
              <span className="truncate">{t("topBar.user.logout")}</span>
            </button>
          </Popover>
        </div>
      </header>

      <ConfirmDialog
        open={logoutConfirmOpen}
        onClose={() => setLogoutConfirmOpen(false)}
        onConfirm={handleConfirmLogout}
        title={t("topBar.user.logout")}
        description={t("common.logOutTips")}
        confirmLabel={t("topBar.user.logout")}
        cancelLabel={t("common.cancel")}
        danger
        loading={isLoggingOut}
      />
    </>
  );
}
