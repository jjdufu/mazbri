<template>
  <div class="resource-table-action-bar">

    <ListQueryBar
      v-if="props.showSearch"
      v-model:q="searchValue"
      v-model:filters="filtersValue"
      :catalog="props.filterCatalog"
      :enable-filters="Boolean(props.filterCatalog)"
      :panel-width="props.searchPanelWidth"
      :live-commit="props.liveCommit"
      @search="emit('search')"
    />


    <slot name="after-search" />

    <slot
      v-if="props.showImport"
      name="import"
      :click="handleImport"
      :disabled="props.importDisabled"
      :icon="props.importIcon"
      :label="props.importLabel"
    >
      <ElTooltip
        :disabled="!props.importDisabled || !props.importDisabledReason"
        :content="props.importDisabledReason"
      >
        <span>
          <ElButton
            v-if="!props.importRoles"
            :disabled="props.importDisabled"
            @click="handleImport"
          >
            <template #icon><ArtSvgIcon :icon="props.importIcon" /></template>
            {{ props.importLabel }}
          </ElButton>
          <ElButton
            v-else
            v-roles="props.importRoles"
            :disabled="props.importDisabled"
            @click="handleImport"
          >
            <template #icon><ArtSvgIcon :icon="props.importIcon" /></template>
            {{ props.importLabel }}
          </ElButton>
        </span>
      </ElTooltip>
    </slot>

    <TableExportDropdown
      v-if="props.showExport && !props.exportRoles"
      :disabled="props.exportDisabled"
      :disabled-reason="props.exportDisabledReason"
      :loading="props.exportLoading"
      :items="props.exportItems"
      :label="props.exportLabel"
      :button-type="props.exportButtonType"
      :icon="props.exportIcon"
      @command="handleExport"
    />
    <TableExportDropdown
      v-else-if="props.showExport"
      v-roles="props.exportRoles"
      :disabled="props.exportDisabled"
      :disabled-reason="props.exportDisabledReason"
      :loading="props.exportLoading"
      :items="props.exportItems"
      :label="props.exportLabel"
      :button-type="props.exportButtonType"
      :icon="props.exportIcon"
      @command="handleExport"
    />

    <slot
      v-if="props.showPrimary"
      name="primary"
      :click="handlePrimary"
      :disabled="props.primaryDisabled"
      :icon="props.primaryIcon"
      :label="props.primaryLabel"
      :type="props.primaryButtonType"
    >
      <ElTooltip
        :disabled="!props.primaryDisabled || !props.primaryDisabledReason"
        :content="props.primaryDisabledReason"
      >
        <span>
          <ElButton
            v-if="!props.primaryRoles"
            :disabled="props.primaryDisabled"
            :type="props.primaryButtonType"
            @click="handlePrimary"
          >
            <template #icon><ArtSvgIcon :icon="props.primaryIcon" /></template>
            {{ props.primaryLabel }}
          </ElButton>
          <ElButton
            v-else
            v-roles="props.primaryRoles"
            :disabled="props.primaryDisabled"
            :type="props.primaryButtonType"
            @click="handlePrimary"
          >
            <template #icon><ArtSvgIcon :icon="props.primaryIcon" /></template>
            {{ props.primaryLabel }}
          </ElButton>
        </span>
      </ElTooltip>
    </slot>


    <slot name="before-delete" />

    <ElTooltip
      v-if="props.showDelete"
      :disabled="!props.deleteDisabled || !props.deleteDisabledReason"
      :content="props.deleteDisabledReason"
    >
      <span>
        <TableDeleteDropdown
          v-if="!props.deleteRoles"
          :total-count="props.deleteTotalCount"
          :selected-count="props.deleteSelectedCount"
          :label="props.deleteLabel"
          :button-type="props.deleteButtonType"
          :icon="props.deleteIcon"
          :disabled="props.deleteDisabled"
          @command="handleDelete"
        />
        <TableDeleteDropdown
          v-else
          v-roles="props.deleteRoles"
          :total-count="props.deleteTotalCount"
          :selected-count="props.deleteSelectedCount"
          :label="props.deleteLabel"
          :button-type="props.deleteButtonType"
          :icon="props.deleteIcon"
          :disabled="props.deleteDisabled"
          @command="handleDelete"
        />
      </span>
    </ElTooltip>

    <slot name="after" />
  </div>
</template>

<script setup lang="ts">
import { type TableExportDropdownItem } from "@/components/table/TableExportDropdown.vue";
import ListQueryBar from "@/components/common/list-query-bar/index.vue";
import TableDeleteDropdown from "@/components/table/TableDeleteDropdown.vue";
import TableExportDropdown from "@/components/table/TableExportDropdown.vue";
import type { ListFilter, ResourceFilterCatalog } from "@/types/list-query";

defineOptions({ name: "ResourceTableActionBar" });

const searchValue = defineModel<string>({
  required: true,
});


const filtersValue = defineModel<ListFilter[]>("filters", {
  default: () => [],
});

const props = withDefaults(
  defineProps<{

    filterCatalog?: ResourceFilterCatalog;
    searchPanelWidth?: number | string;

    liveCommit?: boolean;
    showSearch?: boolean;
    showImport?: boolean;
    importLabel?: string;
    importIcon?: string;
    importDisabled?: boolean;
    importDisabledReason?: string;
    showExport?: boolean;
    exportItems?: TableExportDropdownItem[];
    exportLabel?: string;
    exportDisabled?: boolean;
    exportDisabledReason?: string;
    exportLoading?: boolean;
    exportButtonType?: "primary" | "success" | "warning" | "danger" | "info";
    exportIcon?: string;
    importRoles?: string[];
    exportRoles?: string[];
    showPrimary?: boolean;
    primaryLabel?: string;
    primaryIcon?: string;
    primaryButtonType?: "primary" | "success" | "warning" | "danger" | "info";
    primaryDisabled?: boolean;
    primaryDisabledReason?: string;
    primaryRoles?: string[];
    showDelete?: boolean;
    deleteTotalCount?: number;
    deleteSelectedCount?: number;
    deleteLabel?: string;
    deleteIcon?: string;
    deleteButtonType?: "primary" | "success" | "warning" | "danger" | "info";
    deleteDisabled?: boolean;
    deleteDisabledReason?: string;
    deleteRoles?: string[];
  }>(),
  {
    filterCatalog: undefined,
    searchPanelWidth: 260,
    liveCommit: true,
    showSearch: true,
    showImport: false,
    importLabel: undefined,
    importIcon: "ri:upload-line",
    importDisabled: false,
    importDisabledReason: undefined,
    showExport: true,
    exportItems: () => [],
    exportLabel: undefined,
    exportDisabled: false,
    exportDisabledReason: undefined,
    exportLoading: false,
    exportButtonType: undefined,
    exportIcon: "ri:download-line",
    importRoles: undefined,
    exportRoles: undefined,
    showPrimary: false,
    primaryLabel: undefined,
    primaryIcon: "ri:add-line",
    primaryButtonType: "primary",
    primaryDisabled: false,
    primaryDisabledReason: undefined,
    primaryRoles: undefined,
    showDelete: false,
    deleteTotalCount: 0,
    deleteSelectedCount: 0,
    deleteLabel: undefined,
    deleteIcon: "ri:delete-bin-line",
    deleteButtonType: "danger",
    deleteDisabled: false,
    deleteDisabledReason: undefined,
    deleteRoles: undefined,
  },
);

const emit = defineEmits<{
  (event: "search"): void;
  (event: "import"): void;
  (event: "export", command: string): void;
  (event: "primary"): void;
  (event: "delete", command: "all" | "selected"): void;
}>();

const handleImport = () => emit("import");
const handlePrimary = () => emit("primary");
const handleExport = (command: string) => emit("export", command);
const handleDelete = (command: "all" | "selected") => emit("delete", command);
</script>

<style scoped lang="scss">
.resource-table-action-bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: flex-end;
  gap: 10px;
  min-width: 0;
}
</style>
