import { computed, reactive, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage, ElMessageBox } from "element-plus";
import { $t } from "@/locales";
import { fetchCanonicalServiceNames } from "@/api/service-names";
import type { ConfigListQuery, ConfigSummary } from "@/types/api/config-center";
import type { ColumnOption } from "@/types/component";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-dialog-service";
import { logger } from "@/utils/sys";
import { formatDateTime as formatDate } from "@/utils/format";
import {
  FALLBACK_CORE_CONFIG_NAMES,
  isCoreProtectedConfigName,
} from "../helpers";

function buildColumns(
  prev?: ColumnOption<ConfigSummary>[],
): ColumnOption<ConfigSummary>[] {
  const prevByProp = new Map(prev?.map((col) => [col.prop, col]) ?? []);
  const defs: ColumnOption<ConfigSummary>[] = [
    {
      prop: "service_name",
      label: $t("configCenter.columns.serviceName"),
      minWidth: 220,
      useSlot: true,
    },
    {
      prop: "updated_by",
      label: $t("configCenter.columns.updatedBy"),
      minWidth: 140,
      useSlot: true,
    },
    {
      prop: "updated_at",
      label: $t("configCenter.columns.updatedAt"),
      minWidth: 180,
      useSlot: true,
    },
    {
      prop: "operation",
      label: $t("configCenter.columns.operation"),
      width: 240,
      fixed: "right",
      useSlot: true,
    },
  ];
  return defs.map((def) => {
    const existing = prevByProp.get(def.prop);
    return existing ? { ...existing, label: def.label } : def;
  });
}

export function useConfigCenterPage() {
  const { locale } = useI18n();
  const columns = ref<ColumnOption<ConfigSummary>[]>(buildColumns());
  watch(locale, () => {
    columns.value = buildColumns(columns.value);
  });

  const loading = ref(false);
  const tableData = ref<ConfigSummary[]>([]);
  const currentConfig = ref<ConfigSummary | null>(null);
  const coreProtectedSet = ref<Set<string>>(
    new Set(FALLBACK_CORE_CONFIG_NAMES),
  );
  const searchForm = reactive<Partial<ConfigListQuery>>({
    q: undefined,
  });
  const pagination = reactive({
    current: 1,
    size: 20,
    total: 0,
  });

  const existingServiceNames = computed(() =>
    tableData.value.map((item) => item.service_name),
  );

  const loadCoreProtectedNames = async () => {
    try {
      const data = await fetchCanonicalServiceNames();
      coreProtectedSet.value = new Set([
        ...FALLBACK_CORE_CONFIG_NAMES,
        data.global,
        ...data.services,
      ]);
    } catch (error) {
      logger.error("Failed to load canonical service names:", error);
    }
  };

  const isCoreProtectedService = (serviceName: string): boolean =>
    isCoreProtectedConfigName(serviceName, coreProtectedSet.value);

  const handleDeleteRow = async (
    row: ConfigSummary,
    deleteApi: (row: ConfigSummary) => Promise<void>,
    onDeleteSuccess: () => Promise<void> | void,
  ) => {
    if (isCoreProtectedService(row.service_name)) {
      return;
    }
    try {
      await ElMessageBox.confirm(
        $t("configCenter.messages.deleteServiceConfigConfirm", {
          serviceName: row.service_name,
        }),
        $t("common.tips"),
        {
          confirmButtonText: $t("common.confirm"),
          cancelButtonText: $t("common.cancel"),
          type: "warning",
        },
      );
      await deleteApi(row);
      ElMessage.success($t("common.feedback.deleteSuccess"));
      await onDeleteSuccess();
    } catch (error: unknown) {
      if (error !== "cancel" && !isMfaStepUpCancelled(error)) {
        logger.error("Failed to delete config:", error);
      }
    }
  };

  return {
    columns,
    loading,
    tableData,
    currentConfig,
    searchForm,
    pagination,
    existingServiceNames,
    formatDate,
    loadCoreProtectedNames,
    isCoreProtectedService,
    handleDeleteRow,
  };
}
