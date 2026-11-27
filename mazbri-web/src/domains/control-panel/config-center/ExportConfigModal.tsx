import { useEffect, useState } from "react";
import { saveAs } from "file-saver";
import { Loader2 } from "lucide-react";
import { exportConfigs, fetchServiceList } from "@/api/config-center";
import { FormField, FormModal } from "@/components/ui";
import { useT } from "@/locales";
import { formatDisplayDateYmd } from "@/utils/format";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { logger } from "@/utils/sys/logger";
import {
  showSuccessToast,
  showWarningToast,
} from "@/utils/ui/message-guard";

export interface ExportConfigModalProps {
  open: boolean;
  authorizationDisabled: boolean;
  authorizationDisabledReason: string;
  onClose: () => void;
  onSuccess: () => void;
}

export function ExportConfigModal({
  open,
  authorizationDisabled,
  authorizationDisabledReason,
  onClose,
  onSuccess,
}: ExportConfigModalProps) {
  const { t } = useT();
  const [services, setServices] = useState<string[]>([]);
  const [selected, setSelected] = useState<string[]>([]);
  const [loadingServices, setLoadingServices] = useState(false);
  const [exporting, setExporting] = useState(false);

  useEffect(() => {
    if (!open) {
      setSelected([]);
      return;
    }
    let active = true;
    setLoadingServices(true);
    fetchServiceList()
      .then((res) => {
        if (!active) return;
        setServices(
          Array.isArray(res.services)
            ? res.services.filter((s): s is string => typeof s === "string")
            : [],
        );
      })
      .catch((error) => {
        logger.error("Failed to load services:", error);
      })
      .finally(() => {
        if (active) setLoadingServices(false);
      });
    return () => {
      active = false;
    };
  }, [open]);

  const toggleService = (name: string) => {
    setSelected((prev) =>
      prev.includes(name) ? prev.filter((s) => s !== name) : [...prev, name],
    );
  };

  const handleSubmit = async () => {
    if (authorizationDisabled) {
      showWarningToast(authorizationDisabledReason);
      return;
    }
    if (exporting) return;
    setExporting(true);
    try {
      const blob = await exportConfigs({
        service_names: selected.length > 0 ? selected : undefined,
      });
      const timestamp = formatDisplayDateYmd();
      const suffix =
        selected.length > 0 ? `-${selected.slice(0, 2).join("-")}` : "-all";
      const filename = `configs${suffix}-${timestamp}.zip`;
      saveAs(blob, filename);
      showSuccessToast(t("configCenter.exportDialog.messages.exportSuccess"));
      onClose();
      onSuccess();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to export configs:", error);
      }
    } finally {
      setExporting(false);
    }
  };

  return (
    <FormModal
      open={open}
      onClose={onClose}
      onSubmit={handleSubmit}
      title={t("configCenter.exportDialog.title")}
      size="sm"
      submitLabel={t("configCenter.exportDialog.confirm")}
      cancelLabel={t("common.cancel")}
      submitting={exporting}
      submitDisabled={authorizationDisabled || services.length === 0}
      testId="config-export-modal"
    >
      <FormField
        label={t("configCenter.exportDialog.fields.services")}
        hint={t("configCenter.exportDialog.placeholders.services")}
      >
        {loadingServices ? (
          <div className="flex items-center justify-center py-6 text-xs text-muted-foreground gap-2">
            <Loader2 className="h-4 w-4 animate-spin text-primary" />
            <span>{t("layout.loading")}</span>
          </div>
        ) : (
          <div className="max-h-56 overflow-y-auto rounded-lg border border-border divide-y divide-border">
            {services.map((svc) => (
              <label
                key={svc}
                className="flex items-center gap-2.5 px-3 py-2 text-xs text-foreground hover:bg-hover-bg transition-colors cursor-pointer"
              >
                <input
                  type="checkbox"
                  checked={selected.includes(svc)}
                  disabled={authorizationDisabled}
                  onChange={() => toggleService(svc)}
                />
                <span className="font-mono-tech">{svc}</span>
              </label>
            ))}
          </div>
        )}
      </FormField>
    </FormModal>
  );
}
