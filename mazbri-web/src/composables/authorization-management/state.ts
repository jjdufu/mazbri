import { computed, onMounted, reactive, ref, type ComputedRef, type Ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import type { LicenseStatus } from "@/types/api/authorizations";
import type { AuthorizationFormState } from "./actions";
import { useAuthorizationManagementLabels } from "./labels";
import { useAuthorizationManagementLoader } from "./loader";
import { useAuthorizationManagementSummary } from "./summary";
import type { TagType } from "@/utils/display-labels";

type SummaryRow = {
  label: string;
  value: string;
  kind?: "text" | "tag" | "code";
  tagType?: TagType;
};

export interface AuthorizationManagementContext {
  form: AuthorizationFormState;
  packageFile: Ref<File | null>;
  currentSummary: Ref<LicenseStatus | null>;
  hasSummary: ComputedRef<boolean>;
  statusEmptyDescription: ComputedRef<string>;
  summaryRows: ComputedRef<SummaryRow[]>;
  importSectionTitle: ComputedRef<string>;
  detailSectionTitle: ComputedRef<string>;
  importDisabled: ComputedRef<boolean>;
  loadStatus: (useLoading?: boolean) => Promise<void>;
  importing: Ref<boolean>;
  refreshing: Ref<boolean>;
}

export function useAuthorizationManagementState() {
  const route = useRoute();
  const router = useRouter();
  const form = reactive<AuthorizationFormState>({
    packageFile: null,
    passphrase: "",
  });
  const importing = ref(false);
  const packageFile = computed({
    get: () => form.packageFile,
    set: (file: File | null) => {
      form.packageFile = file;
    },
  });
  const { currentSummary, loadStatus, refreshing } = useAuthorizationManagementLoader();
  const { hasSummary, statusEmptyDescription, summaryRows } =
    useAuthorizationManagementSummary({ currentSummary });
  const { importSectionTitle, detailSectionTitle } = useAuthorizationManagementLabels();
  const importDisabled = computed(() => !form.packageFile || importing.value);

  onMounted(() => {
    if (route.query.type) {
      void router.replace({ path: route.path, query: {} });
    }
    void loadStatus();
  });

  return {
    form,
    packageFile,
    currentSummary,
    hasSummary,
    statusEmptyDescription,
    summaryRows,
    importSectionTitle,
    detailSectionTitle,
    importDisabled,
    loadStatus,
    importing,
    refreshing,
  } satisfies AuthorizationManagementContext;
}
