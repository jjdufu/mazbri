<template>
  <ArtTableCard>
    <template v-if="props.showHeader" #header>
      <slot name="header">
        <SurfaceCardHeader
          :title="props.title"
          :description="props.description"
          :meta="props.meta"
          :total="headerTotal"
          :align="props.headerAlign"
          :stack-on-mobile="props.stackHeaderOnMobile"
        >
          <template v-if="$slots.headerLeft" #left>
            <slot name="headerLeft" />
          </template>
          <template v-if="$slots.actions" #actions>
            <slot name="actions" />
          </template>
        </SurfaceCardHeader>
      </slot>
    </template>

    <slot name="toolbar">
      <ArtTableHeader
        v-if="props.showToolbar"
        v-model:columns="columnsModel"
        :loading="props.loading"
        :layout="props.toolbarLayout"
        @refresh="emit('refresh')"
      >
        <template v-if="$slots.toolbarLeft" #left>
          <slot name="toolbarLeft" />
        </template>
      </ArtTableHeader>
    </slot>

    <ArtTable
      :class="props.tableClass"
      :loading="props.loading"
      :data="props.data"
      :columns="props.columns"
      :empty-text="props.emptyText"
      :pagination="props.pagination"
      :pagination-options="props.paginationOptions"
      :fit="props.fit"
      :table-layout="props.tableLayout"
      :height="props.height"
      :empty-height="props.emptyHeight"
      :show-table-header="props.showTableHeader"
      :stripe="props.stripe"
      :border="props.border"
      :size="props.size"
      @selection-change="emit('selection-change', $event)"
      @pagination:size-change="emit('pagination:size-change', $event)"
      @pagination:current-change="emit('pagination:current-change', $event)"
    >
      <template v-if="$slots.empty" #empty>
        <slot name="empty" />
      </template>
      <template v-for="(_, name) in tableSlots" #[name]="slotProps">
        <slot :name="name" v-bind="slotProps" />
      </template>
    </ArtTable>
  </ArtTableCard>
</template>

<script setup lang="ts" generic="T extends DefaultRow">
import { computed, useSlots } from "vue";
import ArtTableCard from "@/components/core/cards/art-table-card/index.vue";
import ArtTable from "@/components/core/tables/art-table/index.vue";
import ArtTableHeader from "@/components/core/tables/art-table-header/index.vue";
import SurfaceCardHeader from "@/components/common/surface-card-header/index.vue";
import type { ColumnOption, PaginationConfig } from "@/types/component";
import type { PaginationOptions } from "@/components/core/tables/art-table/types";
import type { DefaultRow } from "element-plus/es/components/table/src/table/defaults";

defineOptions({ name: "ResourceTableShell", inheritAttrs: false });

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
  },
);

const emit = defineEmits<{
  (event: "update:columns", columns: ColumnOption<T>[]): void;
  (event: "refresh"): void;
  (event: "selection-change", rows: T[]): void;
  (event: "pagination:size-change", size: number): void;
  (event: "pagination:current-change", page: number): void;
}>();

const slots = useSlots();
const reservedSlotNames = new Set([
  "header",
  "headerLeft",
  "actions",
  "toolbar",
  "toolbarLeft",
]);

const columnsModel = computed({
  get: () => props.columns,
  set: (value: ColumnOption<T>[]) => emit("update:columns", value),
});


const headerTotal = computed(() => {
  if (!props.showTotal || !props.pagination) {
    return null;
  }
  const total = props.pagination.total;
  if (total == null || !Number.isFinite(total)) {
    return 0;
  }
  return Math.max(0, Math.floor(total));
});

const tableSlots = computed(() =>
  Object.fromEntries(
    Object.entries(slots).filter(([name]) => !reservedSlotNames.has(name)),
  ),
);
</script>
