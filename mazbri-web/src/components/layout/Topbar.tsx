import { Fragment, useMemo, useState } from "react";
import { Link, useLocation } from "react-router";
import {
  ChevronDown,
  ChevronRight,
  Lock,
  LogOut,
  Moon,
  ShieldCheck,
  Stamp,
  Sun,
} from "lucide-react";
import {
  Badge,
  Button,
  ConfirmDialog,
  IconButton,
  Popover,
  Tooltip,
} from "@/components/ui";
import { LanguageEnum } from "@/enums/appEnum";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import { useLockScreenStore } from "@/stores/lockscreen";
import { usePersonalSecurityStore } from "@/stores/personal-security";
import { useThemeStore } from "@/stores/theme";
import { useWatermarkStore } from "@/stores/watermark";
import { ROLE_ADMIN, ROLE_FINANCE, ROLE_VIEWER } from "@/utils/ops/roles";
import { resolveBreadcrumbs } from "./menuConfig";

export function Topbar() {
  const location = useLocation();
  const { t, locale, toggleLocale } = useT();
  const info = useAuthStore((s) => s.info);
  const isLoggingOut = useAuthStore((s) => s.isLoggingOut);
  const logOut = useAuthStore((s) => s.logOut);
  const isDark = useThemeStore((s) => s.isDark);
  const toggleTheme = useThemeStore((s) => s.toggleTheme);
  const watermarkVisible = useWatermarkStore((s) => s.visible);
  const toggleWatermark = useWatermarkStore((s) => s.toggleVisible);
  const openLockDialog = useLockScreenStore((s) => s.openLockDialog);
  const openSecurityDrawer = usePersonalSecurityStore((s) => s.openDrawer);

  const [userMenuOpen, setUserMenuOpen] = useState(false);
  const [logoutConfirmOpen, setLogoutConfirmOpen] = useState(false);

  const breadcrumbs = useMemo(
    () => resolveBreadcrumbs(location.pathname, t),
    [location.pathname, t],
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

  const themeTooltip = isDark
    ? locale === LanguageEnum.ZH
      ? "浅色模式"
      : "Light mode"
    : locale === LanguageEnum.ZH
      ? "暗色模式"
      : "Dark mode";

  const localeTooltip =
    locale === LanguageEnum.ZH ? "Switch to English" : "切换为中文";

  const handleConfirmLogout = async () => {
    setLogoutConfirmOpen(false);
    await logOut();
  };

  return (
    <>
      <header
        data-testid="app-topbar"
        className="h-12 bg-card border-b border-border px-4 flex items-center justify-between gap-4 shrink-0"
      >
        <nav
          aria-label="Breadcrumb"
          data-testid="topbar-breadcrumb"
          className="flex items-center gap-1.5 text-xs min-w-0"
        >
          {breadcrumbs.map((crumb, index) => {
            const isLast = index === breadcrumbs.length - 1;
            return (
              <Fragment key={`${crumb.label}-${index}`}>
                {index > 0 && (
                  <ChevronRight className="h-3.5 w-3.5 text-muted-foreground/60 shrink-0" />
                )}
                {isLast ? (
                  <span className="font-semibold text-foreground truncate">
                    {crumb.label}
                  </span>
                ) : crumb.path ? (
                  <Link
                    to={crumb.path}
                    className="text-muted-foreground hover:text-foreground transition-colors truncate"
                  >
                    {crumb.label}
                  </Link>
                ) : (
                  <span className="text-muted-foreground truncate">
                    {crumb.label}
                  </span>
                )}
              </Fragment>
            );
          })}
        </nav>

        <div className="flex items-center gap-1.5 shrink-0">
          <Tooltip content={t("watermark.title")} side="bottom">
            <IconButton
              size="md"
              active={watermarkVisible}
              onClick={toggleWatermark}
              aria-label={t("watermark.title")}
              data-testid="topbar-watermark-btn"
            >
              <Stamp className="h-4 w-4" />
            </IconButton>
          </Tooltip>

          <Tooltip content={t("topBar.user.lockScreen")} side="bottom">
            <IconButton
              size="md"
              onClick={openLockDialog}
              aria-label={t("topBar.user.lockScreen")}
              data-testid="topbar-lock-btn"
            >
              <Lock className="h-4 w-4" />
            </IconButton>
          </Tooltip>

          <Tooltip content={localeTooltip} side="bottom">
            <Button
              type="button"
              variant="ghost"
              size="md"
              onClick={toggleLocale}
              aria-label={localeTooltip}
              data-testid="topbar-locale-btn"
              className="px-2 font-mono-tech text-xs"
            >
              {locale === LanguageEnum.ZH ? "中/EN" : "EN/中"}
            </Button>
          </Tooltip>

          <Tooltip content={themeTooltip} side="bottom">
            <IconButton
              size="md"
              onClick={toggleTheme}
              aria-label={themeTooltip}
              data-testid="topbar-theme-btn"
            >
              {isDark ? (
                <Sun className="h-4 w-4" />
              ) : (
                <Moon className="h-4 w-4" />
              )}
            </IconButton>
          </Tooltip>

          <div className="h-4 w-px bg-border mx-0.5" aria-hidden="true" />

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
                onClick={() => setUserMenuOpen((prev) => !prev)}
                className="inline-flex h-8 items-center gap-2 rounded-md px-2 text-xs text-foreground hover:bg-hover-bg active:bg-active-bg transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-xblue/45"
              >
                <span className="inline-flex h-6 w-6 items-center justify-center rounded-full bg-xblue-soft text-xblue font-semibold text-[11px] shrink-0">
                  {avatarInitial}
                </span>
                <span className="font-medium max-w-[120px] truncate">
                  {username}
                </span>
                {roleLabel && (
                  <Badge
                    variant={info.role === ROLE_ADMIN ? "xblue" : "neutral"}
                    className="hidden sm:inline-flex"
                  >
                    {roleLabel}
                  </Badge>
                )}
                <ChevronDown className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
              </button>
            }
          >
            <div className="px-2.5 py-2 border-b border-border mb-1">
              <div className="text-xs font-semibold text-foreground truncate">
                {info.display_name || username}
              </div>
              <div className="text-[11px] text-muted-foreground truncate">
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
              className="flex w-full h-8 items-center gap-2 rounded-md px-2.5 text-xs text-foreground hover:bg-hover-bg transition-colors cursor-pointer text-left"
            >
              <ShieldCheck className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
              <span className="truncate">
                {t("accountSecurity.drawer.title")}
              </span>
            </button>

            <button
              type="button"
              data-testid="user-menu-lock-btn"
              onClick={() => {
                setUserMenuOpen(false);
                openLockDialog();
              }}
              className="flex w-full h-8 items-center gap-2 rounded-md px-2.5 text-xs text-foreground hover:bg-hover-bg transition-colors cursor-pointer text-left"
            >
              <Lock className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
              <span className="truncate">{t("topBar.user.lockScreen")}</span>
            </button>

            <div className="my-1 border-t border-border" />

            <button
              type="button"
              data-testid="user-menu-logout-btn"
              onClick={() => {
                setUserMenuOpen(false);
                setLogoutConfirmOpen(true);
              }}
              className="flex w-full h-8 items-center gap-2 rounded-md px-2.5 text-xs text-red-600 dark:text-red-400 hover:bg-red-500/10 transition-colors cursor-pointer text-left"
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
