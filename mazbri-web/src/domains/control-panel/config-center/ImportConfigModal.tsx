import { useEffect, useRef, useState } from "react";
import { FileCode2, Upload, X } from "lucide-react";
import { importConfigs } from "@/api/config-center";
import { FormModal, cn } from "@/components/ui";
import { useT } from "@/locales";
import { formatBytes } from "@/utils/format";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { logger } from "@/utils/sys/logger";
import {
  showSuccessToast,
  showWarningToast,
} from "@/utils/ui/message-guard";

const ALLOWED_EXT_RE = /\.(toml|zip)$/i;

export interface ImportConfigModalProps {
  open: boolean;
  authorizationDisabled: boolean;
  authorizationDisabledReason: string;
  onClose: () => void;
  onSuccess: () => void;
}

export function ImportConfigModal({
  open,
  authorizationDisabled,
  authorizationDisabledReason,
  onClose,
  onSuccess,
}: ImportConfigModalProps) {
  const { t } = useT();
  const inputRef = useRef<HTMLInputElement | null>(null);
  const [files, setFiles] = useState<File[]>([]);
  const [dragging, setDragging] = useState(false);
  const [importing, setImporting] = useState(false);

  useEffect(() => {
    if (!open) {
      setFiles([]);
      setDragging(false);
    }
  }, [open]);

  const appendFiles = (list: FileList | null | undefined) => {
    if (!list || list.length === 0) return;
    const valid: File[] = [];
    let hasInvalid = false;
    for (const file of Array.from(list)) {
      if (ALLOWED_EXT_RE.test(file.name)) {
        valid.push(file);
      } else {
        hasInvalid = true;
      }
    }
    if (hasInvalid) {
      showWarningToast(t("configCenter.importDialog.validation.fileType"));
    }
    if (valid.length > 0) {
      setFiles((prev) => {
        const existingNames = new Set(prev.map((f) => `${f.name}-${f.size}`));
        const deduped = valid.filter(
          (f) => !existingNames.has(`${f.name}-${f.size}`),
        );
        return [...prev, ...deduped];
      });
    }
  };

  const handleSubmit = async () => {
    if (authorizationDisabled) {
      showWarningToast(authorizationDisabledReason);
      return;
    }
    if (files.length === 0 || importing) return;
    setImporting(true);
    try {
      const result = await importConfigs(files);
      const imported = result.imported;
      const failed = result.skipped;
      const errors = result.errors ?? [];
      const parts: string[] = [];
      if (imported > 0) {
        parts.push(
          t("configCenter.importDialog.messages.importedCount", {
            count: imported,
          }),
        );
      }
      if (failed > 0) {
        parts.push(
          t("configCenter.importDialog.messages.failedCount", {
            count: failed,
          }),
        );
      }
      if (errors.length > 0) {
        logger.error("Import errors:", errors);
      }
      const summary = t("configCenter.importDialog.messages.importCompleted", {
        details: parts.join("，"),
      });
      if (failed > 0 && imported === 0) {
        showWarningToast(errors[0] ? `${summary}（${errors[0]}）` : summary);
        return;
      }
      if (failed > 0) {
        showWarningToast(errors[0] ? `${summary}（${errors[0]}）` : summary);
      } else {
        showSuccessToast(summary);
      }
      onClose();
      onSuccess();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to import configs:", error);
      }
    } finally {
      setImporting(false);
    }
  };

  return (
    <FormModal
      open={open}
      onClose={onClose}
      onSubmit={handleSubmit}
      title={t("configCenter.importDialog.title")}
      size="md"
      submitLabel={t("configCenter.importDialog.confirm")}
      cancelLabel={t("common.cancel")}
      submitting={importing}
      submitDisabled={authorizationDisabled || files.length === 0}
      testId="config-import-modal"
    >
      <input
        ref={inputRef}
        type="file"
        multiple
        accept=".toml,.zip"
        className="hidden"
        disabled={authorizationDisabled}
        onChange={(e) => {
          appendFiles(e.target.files);
          e.target.value = "";
        }}
      />

      <div
        onDragOver={(e) => {
          e.preventDefault();
          if (!authorizationDisabled) setDragging(true);
        }}
        onDragLeave={() => setDragging(false)}
        onDrop={(e) => {
          e.preventDefault();
          setDragging(false);
          if (!authorizationDisabled) appendFiles(e.dataTransfer.files);
        }}
        onClick={() => {
          if (!authorizationDisabled) inputRef.current?.click();
        }}
        className={cn(
          "flex flex-col items-center justify-center gap-2 rounded-lg border border-dashed px-4 py-7 text-center transition-colors",
          authorizationDisabled
            ? "opacity-50 pointer-events-none border-border bg-muted/20"
            : dragging
              ? "border-primary bg-primary-soft/50 cursor-pointer"
              : "border-border bg-muted/20 hover:border-primary/60 hover:bg-hover-bg/60 cursor-pointer",
        )}
      >
        <Upload className="h-5 w-5 text-muted-foreground" />
        <div className="text-xs font-medium text-foreground">
          {t("configCenter.importDialog.uploadText")}
          <span className="text-primary">
            {t("configCenter.importDialog.clickToUpload")}
          </span>
        </div>
        <div className="text-[11px] text-muted-foreground">
          {t("configCenter.importDialog.tip")}
        </div>
      </div>

      {files.length > 0 && (
        <ul className="max-h-44 overflow-y-auto rounded-lg border border-border divide-y divide-border text-xs">
          {files.map((file, idx) => (
            <li
              key={`${file.name}-${file.size}-${idx}`}
              className="flex items-center justify-between gap-2 px-3 py-2"
            >
              <span className="inline-flex items-center gap-2 min-w-0">
                <FileCode2 className="h-4 w-4 text-primary shrink-0" />
                <span className="truncate font-mono-tech text-foreground">
                  {file.name}
                </span>
                <span className="text-[11px] text-muted-foreground font-mono-tech shrink-0">
                  {formatBytes(file.size)}
                </span>
              </span>
              <button
                type="button"
                aria-label={t("common.filePicker.remove")}
                onClick={() =>
                  setFiles((prev) => prev.filter((_, i) => i !== idx))
                }
                className="inline-flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground hover:text-foreground hover:bg-hover-bg cursor-pointer shrink-0"
              >
                <X className="h-3.5 w-3.5" />
              </button>
            </li>
          ))}
        </ul>
      )}
    </FormModal>
  );
}
