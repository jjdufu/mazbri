import { ElMessage } from "element-plus";
import { $t } from "@/locales";
import {
  downloadCsv,
  generateCsv,
  parseCsvLine,
  parseCsvRecords,
} from "@/utils/csv";

export interface ColumnConfig {
  key: string;
  label: string;
  required: boolean;
  type?: "string" | "number" | "array";
}

export interface PreviewResult<T> {
  new: T[];
  duplicate: T[];
  invalid: {
    raw: string;
    reason: string;
  }[];
}

export interface CsvExportColumn<T> {
  key: keyof T;
  label: string;
}

export interface ExportResourceCsvOptions {
  successMessage?: string | false;
}

export interface CsvImportPreviewOptions<T> {
  content: string;
  columns: ColumnConfig[];
  validator?: (row: Record<string, string>) => {
    valid: boolean;
    reason?: string;
  };
  transformer: (row: Record<string, string>) => T;
  uniqueKey?: keyof T | string;
  existingData?: Record<string, unknown>[];
  isDuplicate?: (item: T) => boolean;
  missingColumnReason?: (column: ColumnConfig) => string;
  validationFailedReason?: string;
}

export interface ResourceCsvImportResult {
  imported: number;
  failed: number;
}

export interface RunResourceCsvImportOptions<T> {
  onError?: (error: unknown, item: T) => void;
}

export function buildResourceCsvContent<T extends Record<string, unknown>>(
  rows: T[],
  columns: CsvExportColumn<T>[],
) {
  return generateCsv(rows, columns);
}

export function exportResourceCsv<T extends Record<string, unknown>>(
  rows: T[],
  columns: CsvExportColumn<T>[],
  filename: string,
  options: ExportResourceCsvOptions = {},
) {
  if (rows.length === 0) {
    ElMessage.warning($t("feedback.import.noDataToExport"));
    return false;
  }

  downloadCsv(buildResourceCsvContent(rows, columns), filename);
  if (options.successMessage !== false) {
    ElMessage.success(
      options.successMessage ?? $t("common.feedback.exportSuccess"),
    );
  }
  return true;
}

export function hasResourceCsvImportRows(content: string) {
  return parseCsvRecords(content).length >= 2;
}

export function parseResourceCsvImportPreview<T>(
  options: CsvImportPreviewOptions<T>,
): PreviewResult<T> {
  const {
    content,
    columns,
    validator,
    transformer,
    uniqueKey,
    existingData = [],
    isDuplicate,
    missingColumnReason = (column) =>
      $t("feedback.import.csvMissingColumn", { column: column.key }),
    validationFailedReason = $t("feedback.import.validationFailed"),
  } = options;
  const result: PreviewResult<T> = {
    new: [],
    duplicate: [],
    invalid: [],
  };
  const lines = parseCsvRecords(content);

  if (lines.length < 2) {
    return result;
  }

  const headers = parseCsvLine(lines[0].toLowerCase()).map((value) =>
    value.trim(),
  );
  const requiredColumns = columns.filter((column) => column.required);

  for (const column of requiredColumns) {
    if (!headers.includes(column.key)) {
      result.invalid.push({
        raw: lines[0].trim(),
        reason: missingColumnReason(column),
      });
      return result;
    }
  }

  const columnIndices: Record<string, number> = {};
  for (const column of columns) {
    const index = headers.findIndex((header) => header === column.key);
    if (index >= 0) {
      columnIndices[column.key] = index;
    }
  }

  for (const line of lines.slice(1)) {
    if (!line.trim()) {
      continue;
    }

    const values = parseCsvLine(line);
    const row: Record<string, string> = {};
    for (const column of columns) {
      const index = columnIndices[column.key];
      row[column.key] = index === undefined ? "" : (values[index] ?? "");
    }

    const validation = validator?.(row);
    if (validation && !validation.valid) {
      result.invalid.push({
        raw: line.trim(),
        reason: validation.reason || validationFailedReason,
      });
      continue;
    }

    const item = transformer(row);
    const duplicate =
      isDuplicate?.(item) ??
      (uniqueKey !== undefined &&
        existingData.some(
          (existing) =>
            String(existing[String(uniqueKey)]) ===
            String((item as Record<string, unknown>)[String(uniqueKey)]),
        ));

    if (duplicate) {
      result.duplicate.push(item);
      continue;
    }

    result.new.push(item);
  }

  return result;
}

export async function runResourceCsvImport<T>(
  items: Record<string, unknown>[],
  importItem: (item: T) => Promise<void>,
  options: RunResourceCsvImportOptions<T> = {},
): Promise<ResourceCsvImportResult> {
  const result: ResourceCsvImportResult = {
    imported: 0,
    failed: 0,
  };

  for (const item of items) {
    const typedItem = item as T;
    try {
      await importItem(typedItem);
      result.imported++;
    } catch (error) {
      result.failed++;
      options.onError?.(error, typedItem);
    }
  }

  notifyResourceImportResult(result.imported, result.failed);
  return result;
}

export function notifyResourceImportResult(imported: number, failed: number) {
  if (failed === 0) {
    ElMessage.success(
      $t("feedback.import.importCountSuccess", { count: imported }),
    );
    return;
  }

  if (imported === 0) {
    ElMessage.error($t("feedback.import.importFailed"));
    return;
  }

  ElMessage.warning(
    $t("feedback.import.importPartialSuccess", { imported, failed }),
  );
}
