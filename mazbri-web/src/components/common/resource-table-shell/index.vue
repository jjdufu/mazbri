<!--
  [不可更改] 列表页主标题统一由顶部导航栏面包屑（ArtBreadcrumb）承担，总条数统一由底部分页栏承担。
  严禁在表格卡片头部（ResourceTableShell / SurfaceCardHeader）渲染与面包屑冲突的大标题（props.title）
  或重复的「共 X 条」标签，以保障页面信息密度与样式统一。今后开发新页面必须遵守此规则。
-->
<template>
  <div ref="shellRootRef" class="resource-table-shell-root">
    <Teleport
      v-if="shouldTeleportActions && isShellVisible && tabHeaderActionsEl"
      :to="tabHeaderActionsEl"
    >
      <div class="resource-table-shell__tab-actions">
        <slot name="actions" />
      </div>
    </Teleport>

    <div
      v-else-if="
        props.showHeader &&
        !$slots.header &&
        ($slots.headerLeft ||
          props.description ||
          props.meta ||
          $slots.actions)
      "
      class="resource-table-shell__top-bar"
    >
      <SurfaceCardHeader
        :title="''"
        :description="props.description"
        :meta="props.meta"
        :total="null"
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
    </div>

    <ArtTableCard>
      <template
        v-if="props.showHeader && $slots.header"
        #header
      >
        <slot name="header" />
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
  </div>
</template>

<script setup lang="ts" generic="T extends DefaultRow">
import {
  computed,
  inject,
  onBeforeUnmount,
  onMounted,
  ref,
  useSlots,
  type Ref,
} from "vue";
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
    border: false,
    size: "small",
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
const shellRootRef = ref<HTMLElement | null>(null);
const isShellVisible = ref(true);
const tabHeaderActionsEl = inject<Ref<HTMLElement | null> | null>(
  "resourceTabHeaderActionsEl",
  null,
);

const shouldTeleportActions = computed(() =>
  Boolean(
    props.showHeader &&
      tabHeaderActionsEl?.value &&
      slots.actions &&
      !slots.header &&
      !slots.headerLeft &&
      !props.description &&
      !props.meta,
  ),
);

let visibilityObserver: ResizeObserver | null = null;
const updateVisibility = () => {
  const el = shellRootRef.value;
  isShellVisible.value = Boolean(el && el.getClientRects().length > 0);
};

onMounted(() => {
  updateVisibility();
  if (typeof ResizeObserver !== "undefined" && shellRootRef.value) {
    visibilityObserver = new ResizeObserver(updateVisibility);
    visibilityObserver.observe(shellRootRef.value);
  }
});

onBeforeUnmount(() => {
  visibilityObserver?.disconnect();
});

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

const tableSlots = computed(() =>
  Object.fromEntries(
    Object.entries(slots).filter(([name]) => !reservedSlotNames.has(name)),
  ),
);
</script>

<style scoped>
.resource-table-shell-root {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
  min-width: 0;
}

.resource-table-shell__top-bar {
  flex-shrink: 0;
  margin-bottom: 4px;
}

.resource-table-shell__tab-actions {
  display: inline-flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}
</style>
