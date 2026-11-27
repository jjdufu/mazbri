import { computed } from "vue";
import { $t } from "@/locales";

export function useAuthorizationManagementLabels() {
  const importSectionTitle = computed(() =>
    $t("certificate.authorizationManagement.sections.import"),
  );
  const detailSectionTitle = computed(() =>
    $t("certificate.authorizationManagement.sections.detail"),
  );
  return {
    importSectionTitle,
    detailSectionTitle,
  };
}
