import { useEffect } from "react";
import { AlertTriangle, ShieldCheck } from "lucide-react";
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

  if (!loaded) return null;

  return (
    <div
      className={cn(
        "flex w-full flex-nowrap items-start gap-2 rounded-md border px-3 py-2 text-xs leading-relaxed",
        mfaEnabled
          ? "border-border-subtle bg-muted/45 text-muted-foreground"
          : "border-warning-border bg-warning-bg text-warning-fg",
        className,
      )}
    >
      {mfaEnabled ? (
        <ShieldCheck className="mt-0.5 h-3.5 w-3.5 shrink-0 text-primary" />
      ) : (
        <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0 text-warning-fg" />
      )}
      <div className="min-w-0 flex-1">
        <span>
          {mfaEnabled
            ? t("mfaRequiredHint.enabled")
            : t("mfaRequiredHint.notEnabled")}
        </span>
        {!mfaEnabled && (
          <button
            type="button"
            onClick={() => openDrawer("mfa")}
            className="ml-1.5 font-medium text-primary underline-offset-2 hover:underline cursor-pointer"
          >
            {t("mfaRequiredHint.goToSetup")}
          </button>
        )}
      </div>
    </div>
  );
}
