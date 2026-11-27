import { useMemo, useState } from "react";
import { Link } from "react-router";
import {
  Check,
  ChevronDown,
  ChevronsLeft,
  ChevronsRight,
  Languages,
  Lock,
  LogOut,
  Monitor,
  Moon,
  ShieldCheck,
  Sun,
} from "lucide-react";
import {
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
import { resolveDefaultHomePath } from "./menuConfig";

export interface TopbarProps {
  collapsed: boolean;
  onToggleCollapse: () => void;
}

export function Topbar({ collapsed, onToggleCollapse }: TopbarProps) {
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

  const homePath = useMemo(
    () => resolveDefaultHomePath(info.role),
    [info.role],
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

  const collapseLabel = collapsed
    ? isZh
      ? "展开导航栏 (⌘B)"
      : "Expand sidebar (⌘B)"
    : isZh
      ? "收起导航栏 (⌘B)"
      : "Collapse sidebar (⌘B)";

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
    icon: typeof Monitor;
  }[] = [
    {
      mode: "auto",
      label: isZh ? "跟随系统" : "System",
      icon: Monitor,
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
        className="sticky top-0 z-30 h-12 border-b border-border bg-card/95 backdrop-blur-xs flex items-center justify-between gap-2 shrink-0"
      >
        <div className="h-full flex items-center shrink-0">
          <Link
            to={homePath}
            className={cn(
              "h-full flex items-center border-r border-border transition-all shrink-0",
              collapsed ? "w-[60px] justify-center px-0" : "w-52 justify-start px-3.5",
            )}
          >
            <div className="flex items-center gap-2 min-w-0 select-none">
              <span className="inline-flex h-7 w-7 items-center justify-center rounded-md bg-xblue text-white text-xs font-bold tracking-tight shrink-0">
                M
              </span>
              {!collapsed && (
                <span className="text-[15px] font-semibold tracking-tight text-foreground truncate">
                  Mazbri
                </span>
              )}
            </div>
          </Link>

          <div className="pl-2 flex items-center">
            <Tooltip content={collapseLabel} side="bottom">
              <IconButton
                size="sm"
                onClick={onToggleCollapse}
                aria-label={collapseLabel}
                data-testid="sidebar-collapse-btn"
              >
                {collapsed ? (
                  <ChevronsRight className="h-4 w-4" />
                ) : (
                  <ChevronsLeft className="h-4 w-4" />
                )}
              </IconButton>
            </Tooltip>
          </div>
        </div>

        <div className="pr-4 flex items-center gap-1.5 shrink-0">
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
            <IconButton
              size="md"
              onClick={toggleLocale}
              aria-label={localeTooltip}
              data-testid="topbar-locale-btn"
            >
              <Languages className="h-4 w-4 text-xblue" />
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
                  onClick={() => setThemeMenuOpen((prev) => !prev)}
                  aria-label={themeTooltip}
                  data-testid="topbar-theme-btn"
                >
                  {theme === "auto" ? (
                    <Monitor className="h-4 w-4" />
                  ) : theme === "dark" ? (
                    <Moon className="h-4 w-4" />
                  ) : (
                    <Sun className="h-4 w-4" />
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
                        : "text-foreground hover:bg-hover-bg",
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
                className="inline-flex h-8 items-center gap-2 rounded-md border border-border bg-card pl-1.5 pr-2 text-xs text-foreground hover:bg-hover-bg hover:border-border-hover active:bg-active-bg transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-xblue/45"
              >
                <span className="inline-flex h-5 w-5 items-center justify-center rounded-full bg-xblue-soft text-xblue font-semibold text-[11px] shrink-0">
                  {avatarInitial}
                </span>
                <span className="font-medium max-w-[120px] truncate">
                  {username}
                </span>
                <ChevronDown className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
              </button>
            }
          >
            <div className="px-2.5 py-2 border-b border-border mb-1">
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
              className="flex w-full h-8 items-center gap-2 rounded-md px-2.5 text-xs text-foreground hover:bg-hover-bg transition-colors cursor-pointer text-left"
            >
              <ShieldCheck className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
              <span className="truncate">
                {t("accountSecurity.drawer.title")}
              </span>
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
