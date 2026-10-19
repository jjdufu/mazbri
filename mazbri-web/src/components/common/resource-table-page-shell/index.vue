<template>
  <ResourceTableShell
    v-model:columns="columnsModel"
    :title="props.title"
    :description="props.description"
    :meta="props.meta"
    :header-align="props.headerAlign"
    :stack-header-on-mobile="props.stackHeaderOnMobile"
    :loading="props.loading"
    :data="props.data"
    :empty-text="props.emptyText"
    :pagination="props.pagination"
    :pagination-options="props.paginationOptions"
    :toolbar-layout="props.toolbarLayout"
    :show-toolbar="props.showToolbar"
    :show-header="props.showHeader"
    :show-total="props.showTotal"
    :table-class="props.tableClass"
    :table-layout="props.tableLayout"
    :height="props.height"
    :empty-height="props.emptyHeight"
    :show-table-header="props.showTableHeader"
    :stripe="props.stripe"
    :border="props.border"
    :size="props.size"
    :fit="props.fit"
    @refresh="emit('refresh')"
    @selection-change="emit('selection-change', $event)"
    @pagination:size-change="emit('pagination:size-change', $event)"
    @pagination:current-change="emit('pagination:current-change', $event)"
  >
    <template #actions>
      <slot name="actions">
        <ResourceTableActionBar
          v-if="props.showActionBar"
          v-model="searchValue"
          v-model:filters="filtersValue"
          :filter-catalog="props.filterCatalog"
          :search-panel-width="props.searchPanelWidth"
          :live-commit="props.liveCommit"
          :show-search="props.showSearch"
          :show-import="props.showImport"
          :import-label="props.importLabel"
          :import-icon="props.importIcon"
          :import-disabled="props.importDisabled"
          :import-roles="props.importRoles"
          :show-export="props.showExport"
          :export-items="props.exportItems"
          :export-label="props.exportLabel"
          :export-disabled="props.exportDisabled"
          :export-loading="props.exportLoading"
          :export-button-type="props.exportButtonType"
          :export-icon="props.exportIcon"
          :export-roles="props.exportRoles"
          :show-primary="props.showPrimary"
          :primary-label="props.primaryLabel"
          :primary-icon="props.primaryIcon"
          :primary-button-type="props.primaryButtonType"
          :primary-disabled="props.primaryDisabled"
          :primary-roles="props.primaryRoles"
          :show-delete="props.showDelete"
          :delete-total-count="props.deleteTotalCount"
          :delete-selected-count="props.deleteSelectedCount"
          :delete-label="props.deleteLabel"
          :delete-icon="props.deleteIcon"
          :delete-button-type="props.deleteButtonType"
          :delete-roles="props.deleteRoles"
          @search="emit('search')"
          @import="emit('import')"
          @export="(command) => emit('export', command)"
          @primary="emit('primary')"
          @delete="(command) => emit('delete', command)"
        >
          <template v-if="$slots['after-search']" #after-search>
            <slot name="after-search" />
          </template>
          <template v-if="$slots.import" #import="slotProps">
            <slot name="import" v-bind="slotProps" />
          </template>
          <template v-if="$slots.primary" #primary="slotProps">
            <slot name="primary" v-bind="slotProps" />
          </template>
          <template v-if="$slots.after" #after>
            <slot name="after" />
          </template>
        </ResourceTableActionBar>
      </slot>
    </template>

    <template v-if="$slots.header" #header>
      <slot name="header" />
    </template>
    <template v-if="$slots.headerLeft" #headerLeft>
      <slot name="headerLeft" />
    </template>
    <template v-if="$slots.toolbar" #toolbar>
      <slot name="toolbar" />
    </template>
    <template v-if="$slots.toolbarLeft" #toolbarLeft>
      <slot name="toolbarLeft" />
    </template>
    <template v-if="$slots.empty" #empty>
      <slot name="empty" />
    </template>
    <template v-for="(_, name) in tableSlots" #[name]="slotProps">
      <slot :name="name" v-bind="slotProps" />
    </template>
  </ResourceTableShell>
</template>

<script setup lang="ts" generic="T extends DefaultRow">
import { computed, useSlots } from "vue";
import type { DefaultRow } from "element-plus/es/components/table/src/table/defaults";
import type { TableExportDropdownItem } from "@/components/table/TableExportDropdown.vue";
import ResourceTableActionBar from "@/components/common/resource-table-action-bar/index.vue";
import ResourceTableShell from "@/components/common/resource-table-shell/index.vue";
import type { ColumnOption, PaginationConfig } from "@/types/component";
import type { PaginationOptions } from "@/components/core/tables/art-table/types";
import type { ListFilter, ResourceFilterCatalog } from "@/types/list-query";

defineOptions({ name: "ResourceTablePageShell", inheritAttrs: false });

const searchValue = defineModel<string>({
  required: true,
});

const filtersValue = defineModel<ListFilter[]>("filters", {
  default: () => [],
});

const props = withDefaults(
  defineProps<{
    title?: string;
    description?: string;
    meta?: string;
    headerAlign?: "center" | "start";
    stackHeaderOnMobile?: boolean;
    columns: ColumnOption<T>[];
    loading?: boolean;
    data: T[];
    emptyText?: string;
    pagination?: PaginationConfig;
    paginationOptions?: PaginationOptions;
    toolbarLayout?: string;
    showToolbar?: boolean;
    showHeader?: boolean;

    showTotal?: boolean;
    tableClass?: string;
    fit?: boolean;
    tableLayout?: "fixed" | "auto";
    height?: string | number;
    emptyHeight?: string;
    showTableHeader?: boolean;
    stripe?: boolean;
    border?: boolean;
    size?: "large" | "default" | "small";
    showActionBar?: boolean;
    filterCatalog?: ResourceFilterCatalog;
    searchPanelWidth?: number | string;

    liveCommit?: boolean;
    showSearch?: boolean;
    showImport?: boolean;
    importLabel?: string;
    importIcon?: string;
    importDisabled?: boolean;
    importRoles?: string[];
    showExport?: boolean;
    exportItems?: TableExportDropdownItem[];
    exportLabel?: string;
    exportDisabled?: boolean;
    exportLoading?: boolean;
    exportButtonType?: "primary" | "success" | "warning" | "danger" | "info";
    exportIcon?: string;
    exportRoles?: string[];
    showPrimary?: boolean;
    primaryLabel?: string;
    primaryIcon?: string;
    primaryButtonType?: "primary" | "success" | "warning" | "danger" | "info";
    primaryDisabled?: boolean;
    primaryRoles?: string[];
    showDelete?: boolean;
    deleteTotalCount?: number;
    deleteSelectedCount?: number;
    deleteLabel?: string;
    deleteIcon?: string;
    deleteButtonType?: "primary" | "success" | "warning" | "danger" | "info";
    deleteRoles?: string[];
  }>(),
  {
    title: "",
    description: "",
    meta: "",
    headerAlign: "center",
    stackHeaderOnMobile: false,
    loading: false,
    emptyText: undefined,
    pagination: undefined,
    paginationOptions: undefined,
    toolbarLayout: "refresh,size,fullscreen,columns,settings",
    showToolbar: true,
    showHeader: true,
    showTotal: true,
    tableClass: "surface-panel-table",
    fit: true,
    tableLayout: undefined,
    height: undefined,
    emptyHeight: undefined,
    showTableHeader: true,
    stripe: undefined,
    border: undefined,
    size: undefined,
    showActionBar: true,
    filterCatalog: undefined,
    searchPanelWidth: 260,
    liveCommit: true,
    showSearch: true,
    showImport: false,
    importLabel: undefined,
    importIcon: "ri:upload-line",
    importDisabled: false,
    importRoles: undefined,
    showExport: true,
    exportItems: () => [],
    exportLabel: undefined,
    exportDisabled: false,
    exportLoading: false,
    exportButtonType: undefined,
    exportIcon: "ri:download-line",
    exportRoles: undefined,
    showPrimary: false,
    primaryLabel: undefined,
    primaryIcon: "ri:add-line",
    primaryButtonType: "primary",
    primaryDisabled: false,
    primaryRoles: undefined,
    showDelete: false,
    deleteTotalCount: 0,
    deleteSelectedCount: 0,
    deleteLabel: undefined,
    deleteIcon: "ri:delete-bin-line",
    deleteButtonType: "danger",
    deleteRoles: undefined,
  },
);

const emit = defineEmits<{
  (event: "update:columns", columns: ColumnOption<T>[]): void;
  (event: "refresh"): void;
  (event: "selection-change", rows: T[]): void;
  (event: "pagination:size-change", size: number): void;
  (event: "pagination:current-change", page: number): void;
  (event: "search"): void;
  (event: "import"): void;
  (event: "export", command: string): void;
  (event: "primary"): void;
  (event: "delete", command: "all" | "selected"): void;
}>();

const slots = useSlots();
const reservedSlotNames = new Set([
  "header",
  "headerLeft",
  "actions",
  "toolbar",
  "toolbarLeft",
  "empty",
  "import",
  "primary",
  "after",
  "after-search",
]);

const columnsModel = computed({
  get: () => props.columns,
  set: (value: ColumnOption<T>[]) => emit("update:columns", value),
});

const tableSlots = computed(() =>
  Object.fromEntries(
    Object.entries(slots).filter(([name]) => !reservedSlotNames.has(name)),
  ),
);
</script>
