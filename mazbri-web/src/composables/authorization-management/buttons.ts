import { computed } from "vue";
import { $t } from "@/locales";
import type { AuthorizationManagementContext } from "./state";

export interface AuthorizationActionButton {
  key: string;
  label: string;
  type: "primary" | "danger" | "warning";
  loading: boolean;
  disabled: boolean;
  onClick: () => void | Promise<void>;
}

export function useAuthorizationManagementButtons(options: {
  state: AuthorizationManagementContext;
  actions: {
    handleExportRequest: () => void | Promise<void>;
    handleImportLicense: () => void | Promise<void>;
  };
}) {
  return computed<AuthorizationActionButton[]>(() => [
    {
      key: "export-request",
      label: $t("certificate.authorizationManagement.license.exportRequest"),
      type: "primary",
      loading: options.state.refreshing.value,
      disabled: false,
      onClick: options.actions.handleExportRequest,
    },
    {
      key: "import-license",
      label: $t("certificate.authorizationManagement.license.importLicense"),
      type: "warning",
      loading: options.state.importing.value,
      disabled: options.state.importDisabled.value,
      onClick: options.actions.handleImportLicense,
    },
  ]);
}
