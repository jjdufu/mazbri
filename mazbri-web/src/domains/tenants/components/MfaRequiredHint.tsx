import { useEffect } from "react";
import { AlertTriangle } from "lucide-react";
import { cn } from "@/components/ui";
import { useT } from "@/locales";
import { useMfaStepUpStore } from "@/stores/mfa-step-up";
import { usePersonalSecurityStore } from "@/stores/personal-security";

export interface MfaRequiredHintProps {
  className?: string;
}

export function MfaRequiredHint({ className }: MfaRequiredHintProps) {
  const { t } = useT();
  const loaded = useMfaStepUpStore((s) => s.loaded);
  const mfaEnabled = useMfaStepUpStore((s) => s.mfaEnabled);
  const refresh = useMfaStepUpStore((s) => s.refresh);
  const openDrawer = usePersonalSecurityStore((s) => s.openDrawer);

  useEffect(() => {
    if (!loaded) {
      void refresh().catch(() => undefined);
    }
  }, [loaded, refresh]);

  if (!loaded || mfaEnabled) return null;

  return (
    <div
      className={cn(
        "flex w-full flex-nowrap items-start gap-2 rounded-md border border-warning-border bg-warning-bg px-3 py-2 text-xs leading-relaxed text-warning-fg",
        className,
      )}
    >
      <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0 text-warning-fg" />
      <div className="min-w-0 flex-1">
        <span>{t("mfaRequiredHint.notEnabled")}</span>
        <button
          type="button"
          onClick={() => openDrawer("mfa")}
          className="ml-1.5 font-medium text-primary underline-offset-2 hover:underline cursor-pointer"
        >
          {t("mfaRequiredHint.goToSetup")}
        </button>
      </div>
    </div>
  );
}
