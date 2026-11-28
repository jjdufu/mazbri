import { useEffect, useState } from "react";
import { useLocation } from "react-router";
import {
  ChevronDown,
  Languages,
  Lock,
  LogOut,
  Moon,
  Search,
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
import { formatThemeTooltip, useThemeStore } from "@/stores/theme";
import { ROLE_ADMIN, ROLE_FINANCE, ROLE_VIEWER } from "@/utils/ops/roles";

export interface TopbarProps {
  onOpenCommandPalette?: () => void;
}

export function Topbar({ onOpenCommandPalette }: TopbarProps = {}) {
  const location = useLocation();
  const { t, locale, toggleLocale } = useT();
  const info = useAuthStore((s) => s.info);
  const isLoggingOut = useAuthStore((s) => s.isLoggingOut);
  const logOut = useAuthStore((s) => s.logOut);
  const theme = useThemeStore((s) => s.theme);
  const toggleTheme = useThemeStore((s) => s.toggleTheme);
  const openLockDialog = useLockScreenStore((s) => s.openLockDialog);
  const openSecurityDrawer = usePersonalSecurityStore((s) => s.openDrawer);

  const [userMenuOpen, setUserMenuOpen] = useState(false);
  const [logoutConfirmOpen, setLogoutConfirmOpen] = useState(false);

  useEffect(() => {
    setUserMenuOpen(false);
  }, [location.pathname]);

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
  const localeTooltip = isZh ? "English" : "中文";
  const themeTooltip = formatThemeTooltip(theme, isZh);

  const handleConfirmLogout = async () => {
    setLogoutConfirmOpen(false);
    await logOut();
  };

  return (
    <>
      <header
        data-testid="app-topbar"
        className="sticky top-0 z-30 h-11 bg-canvas-bg/92 backdrop-blur-md px-3.5 md:px-5 flex items-center justify-between gap-2 shrink-0"
      >
        <span className="sr-only">Mazbri</span>

        <div className="hidden lg:block w-28 shrink-0" aria-hidden="true" />

        {/* Top-Center: Soft Sunken Command Palette Trigger (xBlueNode Parity) */}
        <div className="flex-1 flex justify-center min-w-0">
          <button
            type="button"
            data-testid="global-cmdk-trigger"
            onClick={onOpenCommandPalette}
            aria-label={isZh ? "打开命令面板" : "Open command palette"}
            className="inline-flex h-8 w-full max-w-md items-center justify-between gap-2 rounded-md border border-border-subtle bg-control-bg px-3 text-xs text-muted-foreground hover:text-foreground hover:bg-control-hover hover:border-border-hover active:scale-[0.99] transition-all duration-150 cursor-pointer"
          >
            <span className="inline-flex items-center gap-2 min-w-0">
              <Search className="h-3.5 w-3.5 text-xblue shrink-0" />
              <span className="hidden sm:inline truncate">
                {isZh ? "搜索…" : "Search…"}
              </span>
            </span>
            <kbd className="rounded bg-background/90 px-1.5 py-0.5 text-[10px] font-mono-tech text-muted-foreground shadow-2xs shrink-0">
              ⌘K
            </kbd>
          </button>
        </div>

        {/* Top-Right: Unified Borderless Action Cluster (Lock + Locale + Auto-A Theme + User Pill) */}
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

          <Tooltip content={themeTooltip} side="bottom">
            <IconButton
              size="md"
              onClick={toggleTheme}
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
