import React, { useRef, useState } from "react";
import {
  AlertCircle,
  AlertTriangle,
  CheckCircle2,
  FileSpreadsheet,
  UploadCloud,
  X,
} from "lucide-react";
import { Badge, Button, IconButton, Modal } from "@/components/ui";
import { ErrorCodes } from "@/constants/error-codes";
import { useT } from "@/locales";
import {
  hasResourceCsvImportRows,
  parseResourceCsvImportPreview,
  type ColumnConfig,
  type PreviewResult,
} from "@/utils/resource-csv";
import { logger } from "@/utils/sys/logger";
import {
  showErrorToast,
  showSuccessToast,
  showWarningToast,
} from "@/utils/ui/message-guard";

export interface CsvImportModalProps {
  open: boolean;
  onClose: () => void;
  title: string;
  columns: ColumnConfig[];
  formatHint?: string;
  uniqueKey?: string;
  existingData?: Record<string, unknown>[];
  validator?: (row: Record<string, string>) => {
    valid: boolean;
    reason?: string;
  };
  transformer: (row: Record<string, string>) => Record<string, unknown>;
  onImport: (items: Record<string, unknown>[]) => Promise<void>;
  onSuccess?: () => void;
  showSuccessMessage?: boolean;
  showErrorMessage?: boolean;
  testId?: string;
}

const MAX_FILE_SIZE = 100 * 1024;
const PREVIEW_LIMIT = 10;

const EMPTY_PREVIEW: PreviewResult<Record<string, unknown>> = {
  new: [],
  duplicate: [],
  invalid: [],
};

export function CsvImportModal({
  open,
  onClose,
  title,
  columns,
  formatHint,
  uniqueKey,
  existingData = [],
  validator,
  transformer,
  onImport,
  onSuccess,
  showSuccessMessage = true,
  showErrorMessage = true,
  testId,
}: CsvImportModalProps) {
  const { t } = useT();
  const inputRef = useRef<HTMLInputElement | null>(null);
  const [file, setFile] = useState<File | null>(null);
  const [dragging, setDragging] = useState(false);
  const [step, setStep] = useState<1 | 2>(1);
  const [importing, setImporting] = useState(false);
  const [preview, setPreview] =
    useState<PreviewResult<Record<string, unknown>>>(EMPTY_PREVIEW);

  const resetAll = () => {
    setFile(null);
    setDragging(false);
    setStep(1);
    setPreview(EMPTY_PREVIEW);
    if (inputRef.current) {
      inputRef.current.value = "";
    }
  };

  const handleClose = (forceClose = false) => {
    if (importing) return;
    if (step === 2 && !forceClose) {
      setStep(1);
      setPreview(EMPTY_PREVIEW);
      return;
    }
    resetAll();
    onClose();
  };

  const handleSelectFile = (nextFile: File | null) => {
    if (!nextFile) {
      setFile(null);
      return;
    }
    if (nextFile.size > MAX_FILE_SIZE) {
      showErrorToast(t("feedback.validation.fileSizeLimit100kb"));
      return;
    }
    setFile(nextFile);
  };

  const handlePreview = () => {
    if (!file) return;
    const reader = new FileReader();
    reader.onload = (ev) => {
      try {
        const content = String(ev.target?.result ?? "");
        if (!hasResourceCsvImportRows(content)) {
          showErrorToast(t("feedback.validation.csvNeedHeaderAndRow"));
          setPreview(EMPTY_PREVIEW);
          return;
        }
        const result = parseResourceCsvImportPreview({
          content,
          columns,
          validator,
          transformer,
          uniqueKey,
          existingData,
        });
        const firstLine = content.trim().split(/\r?\n/)[0]?.trim();
        const missingColumn =
          result.invalid.length === 1 &&
          result.new.length === 0 &&
          result.duplicate.length === 0 &&
          result.invalid[0]?.raw.trim() === firstLine;
        if (missingColumn) {
          showErrorToast(result.invalid[0].reason);
          setPreview(EMPTY_PREVIEW);
          return;
        }
        setPreview(result);
        setStep(2);
        if (
          result.new.length === 0 &&
          result.invalid.length === 0 &&
          result.duplicate.length === 0
        ) {
          showWarningToast(t("feedback.validation.csvNoValidData"));
        }
      } catch (error) {
        logger.error("CSV parse error", {
          service: "mazbri-mgmt-web",
          event: "import_dialog_csv_parse_failed",
          severity: "error",
          error_code: ErrorCodes.INTERNAL_ERROR,
          error,
        });
        showErrorToast(t("feedback.validation.csvParseFailed"));
        setPreview(EMPTY_PREVIEW);
      }
    };
    reader.readAsText(file);
  };

  const handleConfirm = async () => {
    if (preview.new.length === 0 || importing) return;
    setImporting(true);
    try {
      await onImport(preview.new);
      if (showSuccessMessage) {
        showSuccessToast(
          t("feedback.import.importCountSuccess", {
            count: preview.new.length,
          }),
        );
      }
      onSuccess?.();
      resetAll();
      onClose();
    } catch (error) {
      logger.error("Import failed", {
        service: "mazbri-mgmt-web",
        event: "import_dialog_import_failed",
        severity: "error",
        error_code: ErrorCodes.INTERNAL_ERROR,
        error,
      });
      if (showErrorMessage) {
        showErrorToast(t("feedback.import.importFailed"));
      }
    } finally {
      setImporting(false);
    }
  };

  return (
    <Modal
      open={open}
      onClose={() => handleClose(true)}
      title={title}
      size="lg"
      closeOnBackdrop={false}
      testId={testId ?? "csv-import-modal"}
      footer={
        <>
          <Button
            type="button"
            variant="outline"
            disabled={importing}
            onClick={() => handleClose(false)}
          >
            {step === 1 ? t("common.cancel") : t("common.importDialog.back")}
          </Button>
          {step === 1 ? (
            <Button
              type="button"
              variant="primary"
              disabled={!file}
              onClick={handlePreview}
              data-testid="csv-import-next-btn"
            >
              {t("common.importDialog.nextStep")}
            </Button>
          ) : (
            <Button
              type="button"
              variant="primary"
              loading={importing}
              disabled={preview.new.length === 0}
              onClick={() => void handleConfirm()}
              data-testid="csv-import-confirm-btn"
            >
              {t("common.importDialog.confirmImport")}
            </Button>
          )}
        </>
      }
    >
      {step === 1 ? (
        <div className="space-y-3">
          <input
            ref={inputRef}
            type="file"
            accept=".csv,text/csv"
            className="hidden"
            onChange={(e) => handleSelectFile(e.target.files?.[0] ?? null)}
          />
          <div
            role="button"
            tabIndex={0}
            onClick={() => inputRef.current?.click()}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                inputRef.current?.click();
              }
            }}
            onDragOver={(e) => {
              e.preventDefault();
              setDragging(true);
            }}
            onDragLeave={() => setDragging(false)}
            onDrop={(e) => {
              e.preventDefault();
              setDragging(false);
              handleSelectFile(e.dataTransfer.files?.[0] ?? null);
            }}
            className={`flex flex-col items-center justify-center gap-2 rounded-lg border border-dashed px-4 py-8 text-center transition-colors cursor-pointer ${
              dragging
                ? "border-primary bg-primary-soft/40"
                : "border-border bg-muted/25 hover:border-border-hover hover:bg-hover-bg/60"
            }`}
          >
            {file ? (
              <div className="flex items-center gap-2.5 text-foreground">
                <FileSpreadsheet className="h-6 w-6 text-primary shrink-0" />
                <div className="text-left min-w-0">
                  <div className="text-xs font-medium truncate max-w-[360px]">
                    {file.name}
                  </div>
                  <div className="text-[11px] text-muted-foreground">
                    {(file.size / 1024).toFixed(1)} KB
                  </div>
                </div>
                <IconButton
                  size="sm"
                  aria-label={t("common.actions.delete")}
                  onClick={(e) => {
                    e.stopPropagation();
                    handleSelectFile(null);
                    if (inputRef.current) inputRef.current.value = "";
                  }}
                >
                  <X className="h-3.5 w-3.5" />
                </IconButton>
              </div>
            ) : (
              <>
                <UploadCloud className="h-7 w-7 text-muted-foreground" />
                <div className="text-xs font-medium text-foreground">
                  {t("common.importDialog.dragToUpload", { type: "CSV" })}
                </div>
                <div className="text-[11px] text-muted-foreground">
                  {t("common.importDialog.fileTip", {
                    ext: "csv",
                    size: "100KB",
                  })}
                </div>
              </>
            )}
          </div>
          {formatHint && (
            <p className="whitespace-pre-line rounded-md border border-border-subtle bg-muted/35 px-3 py-2 text-[11px] leading-relaxed text-muted-foreground">
              {formatHint}
            </p>
          )}
        </div>
      ) : (
        <div className="space-y-3">
          {preview.new.length > 0 && (
            <div className="flex items-start gap-2 rounded-md border border-success-fg/30 bg-success-fg/10 px-3 py-2 text-xs text-success-fg">
              <CheckCircle2 className="h-4 w-4 shrink-0 mt-0.5" />
              <div>
                <div className="font-medium">
                  {t("common.importDialog.newItems", {
                    count: preview.new.length,
                  })}
                </div>
                <div className="text-[11px] opacity-90">
                  {t("common.importDialog.willBeAdded")}
                </div>
              </div>
            </div>
          )}

          {preview.duplicate.length > 0 && (
            <div className="flex items-start gap-2 rounded-md border border-warning-border bg-warning-bg px-3 py-2 text-xs text-warning-fg">
              <AlertTriangle className="h-4 w-4 shrink-0 mt-0.5" />
              <div>
                <div className="font-medium">
                  {t("common.importDialog.duplicateItems", {
                    count: preview.duplicate.length,
                  })}
                </div>
                <div className="text-[11px] opacity-90">
                  {t("common.importDialog.alreadyExists")}
                </div>
              </div>
            </div>
          )}

          {preview.invalid.length > 0 && (
            <div className="flex items-start gap-2 rounded-md border border-danger-fg/35 bg-danger-fg/10 px-3 py-2 text-xs text-danger-fg">
              <AlertCircle className="h-4 w-4 shrink-0 mt-0.5" />
              <div className="min-w-0 space-y-1">
                <div className="font-medium">
                  {t("common.importDialog.invalidItems", {
                    count: preview.invalid.length,
                  })}
                </div>
                {preview.invalid.slice(0, 5).map((item, idx) => (
                  <div
                    key={idx}
                    className="font-mono-tech text-[11px] break-all"
                  >
                    {item.raw} — {item.reason}
                  </div>
                ))}
                {preview.invalid.length > 5 && (
                  <div className="text-[11px]">
                    {t("common.importDialog.moreItems", {
                      count: preview.invalid.length - 5,
                    })}
                  </div>
                )}
              </div>
            </div>
          )}

          {preview.new.length > 0 && (
            <div className="space-y-2">
              <div className="text-xs font-medium text-foreground">
                {t("common.importDialog.previewTitle", {
                  count: PREVIEW_LIMIT,
                })}
              </div>
              <div className="max-h-64 overflow-auto rounded-lg border border-border">
                <table className="w-full border-collapse text-xs">
                  <thead>
                    <tr className="border-b border-border bg-muted/45 text-muted-foreground">
                      {columns.map((col) => (
                        <th
                          key={col.key}
                          className="h-8 px-2.5 text-left font-medium whitespace-nowrap"
                        >
                          {col.label}
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-border-subtle">
                    {preview.new.slice(0, PREVIEW_LIMIT).map((row, rIdx) => (
                      <tr key={rIdx} className="hover:bg-hover-bg/60">
                        {columns.map((col) => {
                          const cellVal = row[col.key];
                          if (col.type === "array" && Array.isArray(cellVal)) {
                            const arr = cellVal as string[];
                            return (
                              <td key={col.key} className="px-2.5 py-1.5">
                                <div className="flex flex-wrap gap-1">
                                  {arr.slice(0, 3).map((tag, tIdx) => (
                                    <Badge key={tIdx} variant="neutral">
                                      {tag}
                                    </Badge>
                                  ))}
                                  {arr.length > 3 && (
                                    <Badge variant="outline">
                                      +{arr.length - 3}
                                    </Badge>
                                  )}
                                </div>
                              </td>
                            );
                          }
                          return (
                            <td
                              key={col.key}
                              className="px-2.5 py-1.5 text-foreground max-w-[220px] truncate"
                              title={
                                cellVal != null ? String(cellVal) : undefined
                              }
                            >
                              {cellVal != null && cellVal !== ""
                                ? String(cellVal)
                                : "—"}
                            </td>
                          );
                        })}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          )}
        </div>
      )}
    </Modal>
  );
}
