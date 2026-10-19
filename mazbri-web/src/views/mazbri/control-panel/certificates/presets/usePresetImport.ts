import type {
  ColumnConfig,
  PreviewResult,
} from "@/composables/useResourceCsvImportExport";
import { parseResourceCsvImportPreview } from "@/composables/useResourceCsvImportExport";
import { $t } from "@/locales";
import type {
  CreatePresetRequest,
  PresetDnsName,
} from "@/types/api/certificate";
export const PRESET_IMPORT_COLUMNS: ColumnConfig[] = [
  { key: "service_name", label: "service_name", required: true },
  { key: "dns_names", label: "dns_names", required: true, type: "array" },
  { key: "description", label: "description", required: false },
];
export function parseDnsNamesText(text: string): string[] {
  return text
    .split(/[\n,]/)
    .map((value) => value.trim())
    .filter((value) => value.length > 0);
}
export function stringifyDnsNamesText(dnsNames: string[]): string {
  return dnsNames.join("\n");
}
export function sanitizePresetList(data: unknown): PresetDnsName[] {
  if (!Array.isArray(data)) {
    return [];
  }
  return data.filter(isValidPreset);
}
export function validatePresetImportRow(row: Record<string, string>) {
  const serviceName = row.service_name?.trim() ?? "";
  const dnsNames = parseDnsNamesText(row.dns_names ?? "");
  if (!serviceName) {
    return {
      valid: false,
      reason: $t("certificate.presets.importValidation.missingServiceName"),
    };
  }
  if (dnsNames.length === 0) {
    return {
      valid: false,
      reason: $t("certificate.presets.importValidation.missingDnsNames"),
    };
  }
  return { valid: true };
}
export function transformPresetImportRow(
  row: Record<string, string>,
): CreatePresetRequest {
  return {
    service_name: row.service_name.trim(),
    dns_names: parseDnsNamesText(row.dns_names ?? ""),
    description: row.description?.trim() ?? "",
  };
}
export function parsePresetImportPreview(
  content: string,
  existingPresets: Pick<PresetDnsName, "service_name">[],
): PreviewResult<CreatePresetRequest> {
  const existingServices = new Set(
    existingPresets.map((preset) => preset.service_name),
  );
  return parseResourceCsvImportPreview({
    content,
    columns: PRESET_IMPORT_COLUMNS,
    validator: validatePresetImportRow,
    transformer: transformPresetImportRow,
    isDuplicate: (item) => existingServices.has(item.service_name),
    missingColumnReason: (column) =>
      $t("certificate.presets.importValidation.missingColumn", {
        key: column.key,
      }),
    validationFailedReason: $t("certificate.presets.importValidation.failed"),
  });
}
function isValidPreset(value: unknown): value is PresetDnsName {
  if (!value || typeof value !== "object") {
    return false;
  }
  const preset = value as Partial<PresetDnsName>;
  return Boolean(
    preset.id &&
      preset.service_name?.trim() &&
      Array.isArray(preset.dns_names) &&
      preset.dns_names.length > 0,
  );
}
