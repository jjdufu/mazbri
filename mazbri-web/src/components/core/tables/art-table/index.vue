<template>
  <div
    ref="tableRootRef"
    class="art-table"
    :class="{ 'is-empty': isEmpty, 'art-table--cell-copy': cellCopy }"
    :style="containerStyle"
    @click="onTableBodyClick"
  >
    <ElTable
      ref="elTableRef"
      v-loading="!!loading"
      v-bind="{
        ...$attrs,
        ...props,
        height,
        stripe,
        border,
        size,
        headerCellStyle,
      }"
    >
      <template v-for="col in visibleColumns" :key="col.prop || col.type">
        <ElTableColumn
          v-if="col.type === 'globalIndex'"
          v-bind="cleanColumnProps(col)"
        >
          <template #default="{ $index }">
            <span>{{ getGlobalIndex($index) }}</span>
          </template>
        </ElTableColumn>

        <ElTableColumn
          v-else-if="col.type === 'expand'"
          v-bind="cleanColumnProps(col)"
        >
          <template #default="{ row }">
            <component :is="col.formatter ? col.formatter(row) : null" />
          </template>
        </ElTableColumn>

        <ElTableColumn v-else v-bind="cleanColumnProps(col)">
          <template v-if="col.useHeaderSlot && col.prop" #header="headerScope">
            <slot
              :name="col.headerSlotName || `${col.prop}-header`"
              v-bind="{ ...headerScope, prop: col.prop, label: col.label }"
            >
              {{ col.label }}
            </slot>
          </template>
          <template v-if="col.useSlot && col.prop" #default="slotScope">
            <slot
              :name="col.slotName || col.prop"
              v-bind="{
                ...slotScope,
                prop: col.prop,
                value: col.prop ? slotScope.row[col.prop] : undefined,
              }"
            />
          </template>
        </ElTableColumn>
      </template>

      <template v-if="$slots.default" #default><slot /></template>

      <template #empty>
        <div v-if="loading"></div>
        <div v-else class="art-table-empty">{{ emptyText }}</div>
      </template>
    </ElTable>

    <div
      v-if="showPagination"
      ref="paginationRef"
      class="pagination custom-pagination pagination-container"
      :class="mergedPaginationOptions?.align"
    >
      <div class="pagination-bar">

        <ElPagination
          v-bind="mergedPaginationOptions"
          class="pagination-sizes"
          :total="pagination?.total ?? 0"
          :page-count="safePageCount"
          :disabled="loading"
          :page-size="pagination?.size"
          :current-page="pagination?.current"
          @size-change="handleSizeChange"
          @current-change="onCurrentChange"
        />

        <div class="pagination-nav" role="navigation" :aria-label="t('common.pagination.nav')">
          <button
            type="button"
            class="pagination-nav-btn"
            :disabled="!!loading || isFirstPage"
            :aria-label="t('common.pagination.prev')"
            :title="t('common.pagination.prev')"
            @click="goPrev"
          >
            <ElIcon><ArrowLeft /></ElIcon>
          </button>





          <div class="pagination-jump">
            <input
              ref="jumpInputRef"
              v-model="jumpInput"
              class="pagination-jump-input"
              type="text"
              inputmode="numeric"
              autocomplete="off"
              spellcheck="false"
              :disabled="!!loading"
              :aria-label="t('common.pagination.jumpTo')"
              @focus="onJumpFocus"
              @keydown.enter.prevent="commitJump"
              @blur="commitJump"
            />
            <span class="pagination-jump-sep" aria-hidden="true">/</span>
            <button
              type="button"
              class="pagination-jump-total"
              :disabled="!!loading || isLastPage || safePageCount <= 1"
              :title="t('common.pagination.goLast')"
              :aria-label="t('common.pagination.goLast')"
              @mousedown.prevent
              @click="jumpToLast"
            >
              {{ safePageCount }}
            </button>
          </div>

          <button
            type="button"
            class="pagination-nav-btn"
            :disabled="!!loading || isLastPage"
            :aria-label="t('common.pagination.next')"
            :title="t('common.pagination.next')"
            @click="goNext"
          >
            <ElIcon><ArrowRight /></ElIcon>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import {
  computed,
  nextTick,
  ref,
  watch,
} from "vue";
import { useI18n } from "vue-i18n";
import { storeToRefs } from "pinia";
import { ArrowLeft, ArrowRight } from "@element-plus/icons-vue";
import { ElMessage } from "element-plus";
import { useTableStore } from "@/store/modules/table";
import type { ColumnOption } from "@/types/component";
import { getColumnVisibility } from "@/hooks/core/useTableColumns";
import { resolveSafePageCount } from "@/utils/table/paginationLimits";
import { copyTextToClipboard } from "@/utils/clipboard";
import type { ArtTableProps, PaginationOptions } from "./types";
import {
  SIZES_LAYOUT,
  STRIP_LAYOUT_PARTS,
  createDefaultPaginationOptions,
} from "./constants";

import { useTableComposable } from "./useTableComposable";
defineOptions({ name: "ArtTable" });
const props = withDefaults(defineProps<ArtTableProps>(), {
  columns: () => [],
  fit: true,
  showHeader: true,
  stripe: undefined,
  border: undefined,
  size: undefined,
  emptyHeight: "100%",
  showTableHeader: true,

  cellCopy: true,
});
const { t } = useI18n();
const emit = defineEmits<{
  (e: "pagination:size-change", val: number): void;
  (e: "pagination:current-change", val: number): void;
}>();


const CELL_COPY_SKIP_SELECTOR = [
  "button",
  "a",
  "input",
  "textarea",
  "select",
  "label",
  ".el-button",
  ".el-checkbox",
  ".el-radio",
  ".el-switch",
  ".el-link",
  ".el-dropdown",
  ".el-select",
  ".el-input",
  ".el-pagination",
  ".el-table__expand-icon",
  ".table-action-row",
  "[contenteditable='true']",
].join(", ");

let lastCellCopyAt = 0;
let lastCellCopyText = "";

const tableRootRef = ref<HTMLElement | null>(null);

const extractCellCopyText = (td: Element): string => {
  // 优先查找带有 data-copy-text 属性的元素
  const copyTarget = td.querySelector("[data-copy-text]");
  if (copyTarget) {
    return copyTarget.getAttribute("data-copy-text") ?? "";
  }
  // 回退到提取整个 cell 的文本内容
  const cell = td.querySelector(".cell");
  const raw = (cell?.textContent ?? td.textContent ?? "")
    .replace(/\u00a0/g, " ")
    .replace(/[ \t]+\n/g, "\n")
    .replace(/\n{2,}/g, "\n")
    .trim();
  return raw;
};


const findCopyableCell = (target: EventTarget | null): HTMLElement | null => {
  if (!(target instanceof Element)) return null;
  if (
    target.closest(".pagination, .el-table__header-wrapper, .el-loading-mask")
  ) {
    return null;
  }
  if (target.closest(CELL_COPY_SKIP_SELECTOR)) return null;
  const td = target.closest(".el-table__body td");
  if (!td || !(td instanceof HTMLElement)) return null;
  if (td.querySelector(".el-checkbox, .el-radio, .el-table__expand-icon")) {
    return null;
  }
  if (td.querySelector(".el-button, .el-dropdown, .table-action-row, .el-link")) {
    return null;
  }
  return td;
};



const onTableBodyClick = async (event: MouseEvent) => {
  if (!props.cellCopy) return;
  const td = findCopyableCell(event.target);
  if (!td) return;

  const text = extractCellCopyText(td);
  if (!text) return;

  const now = Date.now();
  if (text === lastCellCopyText && now - lastCellCopyAt < 500) {
    return;
  }

  const ok = await copyTextToClipboard(text);
  if (!ok) {
    ElMessage.error(t("common.feedback.copyFailed"));
    return;
  }
  lastCellCopyAt = now;
  lastCellCopyText = text;
  ElMessage.success({
    message: t("common.feedback.copiedToClipboard"),
    duration: 1200,
  });
};
const tableStore = useTableStore();
const { isBorder, isZebra, tableSize, isFullScreen, isHeaderBackground } =
  storeToRefs(tableStore);
const {
  elTableRef,
  paginationRef,
  containerHeight,
  cleanColumnProps,
  handleSizeChange,
  handleCurrentChange,
  scrollToTop,
  getGlobalIndex,
} = useTableComposable(props, emit);

const defaultPaginationOptions = computed(() =>
  createDefaultPaginationOptions(),
);


const normalizeLayout = (raw: string | undefined): string => {
  if (!raw?.trim()) {
    return SIZES_LAYOUT;
  }
  const parts = raw
    .split(",")
    .map((part) => part.trim())
    .filter((part) => part && !STRIP_LAYOUT_PARTS.has(part));
  return parts.includes("sizes") ? "sizes" : SIZES_LAYOUT;
};

const mergedPaginationOptions = computed<PaginationOptions>(() => {
  const merged = {
    ...defaultPaginationOptions.value,
    ...props.paginationOptions,
  };
  merged.layout = normalizeLayout(merged.layout);
  delete merged.pagerCount;
  return merged;
});
const safePageCount = computed(() =>
  resolveSafePageCount(
    props.pagination?.total,
    props.pagination?.size,
    props.pagination?.pageCount,
  ),
);
const currentPage = computed(() => props.pagination?.current ?? 1);
const isFirstPage = computed(() => currentPage.value <= 1);
const isLastPage = computed(() => currentPage.value >= safePageCount.value);

const jumpInputRef = ref<HTMLInputElement | null>(null);
const jumpInput = ref(String(props.pagination?.current ?? 1));
const jumpFocused = ref(false);
const syncJumpFromProps = () => {
  if (!jumpFocused.value) {
    jumpInput.value = String(props.pagination?.current ?? 1);
  }
};
watch(
  () =>
    [props.pagination?.current, props.pagination?.size, safePageCount.value] as const,
  () => {
    syncJumpFromProps();
  },
);
const onJumpFocus = () => {
  jumpFocused.value = true;
  nextTick(() => {
    jumpInputRef.value?.select?.();
  });
};
const clampJumpPage = (raw: string): number | null => {
  const digits = String(raw ?? "").replace(/[^\d]/g, "");
  if (!digits) {
    return null;
  }
  const parsed = Number.parseInt(digits, 10);
  if (!Number.isFinite(parsed) || parsed < 1) {
    return null;
  }
  const maxPage = Math.max(1, safePageCount.value);
  return Math.min(parsed, maxPage);
};
const commitJump = () => {
  jumpFocused.value = false;
  const next = clampJumpPage(jumpInput.value);
  const current = currentPage.value;
  if (next === null) {
    jumpInput.value = String(current);
    return;
  }
  jumpInput.value = String(next);
  if (next !== current) {
    handleCurrentChange(next);
  }
};
const goPrev = () => {
  if (props.loading || isFirstPage.value) {
    return;
  }
  handleCurrentChange(currentPage.value - 1);
};
const goNext = () => {
  if (props.loading || isLastPage.value) {
    return;
  }
  handleCurrentChange(currentPage.value + 1);
};
const jumpToLast = () => {
  jumpFocused.value = false;
  const last = Math.max(1, safePageCount.value);
  jumpInput.value = String(last);
  if (currentPage.value !== last) {
    handleCurrentChange(last);
  }
};
const onCurrentChange = (page: number) => {
  if (!jumpFocused.value) {
    jumpInput.value = String(page);
  }
  handleCurrentChange(page);
};

const orderVisibleColumns = (cols: ColumnOption[]): ColumnOption[] => {
  const selection: ColumnOption[] = [];
  const operations: ColumnOption[] = [];
  const middle: ColumnOption[] = [];
  for (const col of cols) {
    const prop = String(col.prop ?? col.slotName ?? col.type ?? "");
    if (col.type === "selection" || prop === "selection") {
      selection.push(col);
    } else if (prop === "operations" || col.slotName === "operations") {
      operations.push({ ...col, fixed: col.fixed === "left" ? "left" : "right" });
    } else {
      middle.push(col);
    }
  }
  return [...selection, ...middle, ...operations];
};

const visibleColumns = computed<ColumnOption[]>(() =>
  orderVisibleColumns(props.columns.filter((col) => getColumnVisibility(col))),
);
const emptyText = computed(
  () => props.emptyText ?? t("common.feedback.noData"),
);
const border = computed(() => props.border ?? isBorder.value);
const stripe = computed(() => props.stripe ?? isZebra.value);
const size = computed(() => props.size ?? tableSize.value);
const isEmpty = computed(() => props.data?.length === 0);
const containerStyle = computed(() => {
  if (isEmpty.value && props.emptyHeight === "auto" && !props.loading) {
    return {
      height: "auto",
      minHeight: "unset",
    };
  }
  return containerHeight.value;
});
const height = computed(() => {
  if (isFullScreen.value) return "100%";
  if (isEmpty.value && !props.loading) return props.emptyHeight;
  if (props.height) return props.height;
  return undefined;
});
const headerCellStyle = computed(() => ({
  background: isHeaderBackground.value
    ? "var(--el-fill-color-lighter)"
    : "var(--default-box-color)",
  ...(props.headerCellStyle || {}),
}));
const showPagination = computed(() => {
  if (!props.pagination) return false;
  const total = props.pagination.total ?? 0;
  return total > 0 || !isEmpty.value;
});
defineExpose({
  scrollToTop,
  elTableRef,
});
</script>

<style lang="scss" scoped>
@use "./style";
</style>
