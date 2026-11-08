<!--
  [不可更改] 全站列表搜索框统一规范：
  1. 高度严格对齐 Element Plus default 按钮（32px），禁止使用 36px/38px 导致与同行按钮高低错位；
  2. 默认常驻展开（alwaysExpanded: true）且默认宽度统一为 240px，内置前缀搜索图标，
     避免折叠态（36px）与展开态（含外部「收起搜索」按钮达 363px）切换时挤压同行操作按钮换行。
-->
<template>
  <div
    ref="rootRef"
    class="list-query-bar"
    :class="{ 'is-expanded': expanded }"
    :style="panelStyle"
  >

    <ElTooltip
      v-if="!expanded"
      placement="top"
      :show-after="hasActiveQuery ? 200 : 400"
      popper-class="list-query-bar__hover-popper"
    >
      <template #content>
        <div v-if="hasActiveQuery" class="list-query-bar__hover">
          <div v-if="qModel.trim()" class="list-query-bar__hover-line">
            {{ t("listQueryBar.hover.activeQ") }}:
            <strong>{{ qModel.trim() }}</strong>
          </div>
          <div
            v-if="filtersModel.length"
            class="list-query-bar__hover-title"
          >
            {{
              t("listQueryBar.hover.activeFilters", {
                n: filtersModel.length,
              })
            }}
          </div>
          <ul v-if="filtersModel.length" class="list-query-bar__hover-list">
            <li
              v-for="(f, index) in filtersModel"
              :key="`hover-${f.field}-${f.op}-${index}`"
              class="list-query-bar__hover-item"
            >
              {{ chipText(f) }}
            </li>
          </ul>
        </div>
        <span v-else>{{ tooltipText }}</span>
      </template>
      <button
        type="button"
        class="list-query-bar__trigger"
        :class="{ 'is-active': hasActiveQuery }"
        :aria-label="tooltipText"
        @click="handleExpand"
      >
        <ArtSvgIcon :icon="collapseIcon" />
        <span
          v-if="activeConditionCount > 0"
          class="list-query-bar__badge"
        >{{ activeConditionCount }}</span>
      </button>
    </ElTooltip>


    <template v-else>
      <button
        v-if="!alwaysExpanded"
        type="button"
        class="list-query-bar__trigger is-expanded-btn"
        :aria-label="collapseText"
        @click="handleCollapse"
      >
        <ArtSvgIcon :icon="collapseIcon" />
        <span class="list-query-bar__trigger-text">{{ collapseText }}</span>
        <span
          v-if="activeConditionCount > 0"
          class="list-query-bar__badge list-query-bar__badge--inline"
        >{{ activeConditionCount }}</span>
      </button>

      <div
        class="list-query-bar__panel"
        :class="{
          'has-chips': hasCommittedChips,
          'is-idle-chips': isIdleChips,
          'is-drafting': isDrafting,
        }"
        :style="shellPanelStyle"
        @mousedown.stop
      >
        <div
          ref="shellRef"
          class="list-query-bar__field"
          :class="{
            'has-chips': hasCommittedChips,
            'is-idle-chips': isIdleChips,
            'is-drafting': isDrafting,
          }"
          @mousedown="onFieldMouseDown"
        >
          <ArtSvgIcon
            v-if="alwaysExpanded"
            :icon="collapseIcon"
            class="list-query-bar__prefix-icon"
          />

          <div
            v-if="primaryChipItem"
            class="list-query-bar__chips"
          >
            <ListFilterChip
              :key="primaryChipItem.id"
              class="list-query-bar__inline-chip"
              :text="primaryChipItem.text"
              :title="primaryChipItem.title"
              :remove-label="t('listFilters.remove')"
              @remove="primaryChipItem.onRemove()"
            />
            <ElPopover
              v-if="overflowChipCount > 0"
              placement="bottom-start"
              trigger="hover"
              width="auto"
              :show-after="100"
              popper-class="list-query-bar__overflow-popper"
              @before-enter="measureOverflowContentWidth"
              @after-enter="measureOverflowContentWidth"
            >
              <template #reference>
                <span
                  class="list-query-bar__more-badge"
                  @mousedown.stop
                >
                  +{{ overflowChipCount }}
                </span>
              </template>
              <div
                class="list-query-bar__overflow-content"
                :style="overflowContentStyle"
                @mousedown.stop
              >
                <div class="list-query-bar__overflow-header">
                  <span class="list-query-bar__overflow-title">
                    {{
                      t("listQueryBar.hover.activeFilters", {
                        n: committedChipItems.length,
                      })
                    }}
                  </span>
                  <ElButton
                    link
                    type="primary"
                    size="small"
                    @click="clearAllQuery"
                  >
                    {{ t("listQueryBar.clearAll") }}
                  </ElButton>
                </div>
                <div
                  ref="overflowChipsRef"
                  class="list-query-bar__overflow-chips"
                >
                  <ListFilterChip
                    v-for="item in committedChipItems"
                    :key="`overflow-${item.id}`"
                    class="list-query-bar__overflow-chip"
                    :text="item.text"
                    :title="item.title"
                    :remove-label="t('listFilters.remove')"
                    @remove="item.onRemove()"
                  />
                </div>
              </div>
            </ElPopover>
          </div>

          <ElInput
            ref="inputRef"
            v-model="draftText"
            class="list-query-bar__input"
            :class="{ 'has-draft': hasDraftText }"
            size="default"
            clearable
            :placeholder="resolvedPlaceholder"
            :aria-label="tooltipText"
            @focus="onInputFocus"
            @keydown="onInputKeydown"
            @compositionstart="composing = true"
            @compositionend="onCompositionEnd"
            @input="onDraftInput"
            @clear="onDraftClear"
          />
        </div>

        <p
          v-if="parseError"
          class="list-query-bar__error"
          role="alert"
        >
          {{ parseError }}
        </p>
      </div>

      <ElButton
        v-if="hasActiveQuery"
        class="list-query-bar__clear"
        link
        type="primary"
        size="small"
        @click="clearAllQuery"
      >
        {{ t("listQueryBar.clearAll") }}
      </ElButton>
    </template>


    <Teleport to="body">
      <ul
        v-if="expanded && suggestOpen && suggestItems.length"
        class="list-query-bar__suggest"
        role="listbox"
        :style="suggestStyle"
      >
        <li
          v-for="(item, idx) in suggestItems"
          :key="item.id"
          class="list-query-bar__suggest-item"
          :class="{
            'is-active': idx === suggestIndex,
            'is-hint': item.kind === 'hint',
          }"
          role="option"
          :aria-selected="idx === suggestIndex"
          @mousedown.prevent="applySuggest(item)"
          @mouseenter="suggestIndex = idx"
        >
          <span class="list-query-bar__suggest-icon" aria-hidden="true">
            <ArtSvgIcon :icon="suggestIcon(item.kind)" />
          </span>
          <span class="list-query-bar__suggest-main">
            <span class="list-query-bar__suggest-label">{{ item.label }}</span>
          </span>
          <span
            v-if="item.detail"
            class="list-query-bar__suggest-detail"
          >
            {{ item.detail }}
          </span>
        </li>
      </ul>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
} from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import ListFilterChip from "@/components/common/list-filter-chip/index.vue";
import type {
  FilterFieldDef,
  FilterOp,
  ListFilter,
  ResourceFilterCatalog,
} from "@/types/list-query";
import {
  catalogFilterableFields,
  maxFilters,
} from "@/utils/list-query/catalog";
import { buildAliasMaps, matchFieldRefAtStart } from "@/utils/list-query/field-aliases";
import { mergeByFieldOp } from "@/utils/list-query/merge-filters";
import {
  looksLikeStrongClause,
  parseClause,
} from "@/utils/list-query/parse-clause";
import { formatFilterChipText } from "@/utils/list-query/serialize";
import {
  buildSuggestItems,
  type SuggestItem,
  type SuggestKind,
} from "@/utils/list-query/suggest";









defineOptions({ name: "ListQueryBar" });

const props = withDefaults(
  defineProps<{




    catalog?: ResourceFilterCatalog;
    enableFilters?: boolean;
    placeholder?: string;
    tooltip?: string;




    panelWidth?: number | string;
    collapseIcon?: string;
    collapseText?: string;
    alwaysExpanded?: boolean;
    liveCommit?: boolean;
  }>(),
  {
    catalog: undefined,
    enableFilters: undefined,
    placeholder: undefined,
    tooltip: undefined,

    panelWidth: 240,
    collapseIcon: "ri:search-line",
    collapseText: undefined,
    alwaysExpanded: true,
    liveCommit: true,
  },
);

const qModel = defineModel<string>("q", { default: "" });
const filtersModel = defineModel<ListFilter[]>("filters", {
  default: () => [],
});

const emit = defineEmits<{
  search: [];
  change: [payload: { q: string; filters: ListFilter[] }];
}>();

const { t } = useI18n();

const expanded = ref(props.alwaysExpanded);
const suggestOpen = ref(false);
const draftText = ref("");
const parseError = ref<string | null>(null);
const suggestIndex = ref(-1);
const composing = ref(false);
const rootRef = ref<HTMLElement | null>(null);
const shellRef = ref<HTMLElement | null>(null);
const inputRef = ref<{ focus?: () => void } | null>(null);
const suggestStyle = ref<Record<string, string>>({});

let liveCommitTimer: ReturnType<typeof setTimeout> | null = null;
const LIVE_COMMIT_MS = 380;

const isFullMode = computed(() => {
  if (!props.catalog) return false;
  if (props.enableFilters === false) return false;
  return true;
});

const hasActiveQuery = computed(
  () => Boolean(qModel.value.trim()) || filtersModel.value.length > 0,
);


const activeConditionCount = computed(() => {
  let n = filtersModel.value.length;
  if (qModel.value.trim()) n += 1;
  return n;
});

const tooltipText = computed(
  () => props.tooltip ?? t("listQueryBar.tooltip"),
);

const collapseText = computed(
  () => props.collapseText ?? t("common.searchPopover.collapse"),
);

const normalizedWidth = computed(() =>
  typeof props.panelWidth === "number"
    ? `${props.panelWidth}px`
    : props.panelWidth,
);


const designWidthPx = computed(() => {
  if (typeof props.panelWidth === "number") return props.panelWidth;
  const n = parseFloat(String(props.panelWidth));
  return Number.isFinite(n) ? n : 240;
});






const panelStyle = computed(() => ({
  "--list-query-bar-width": normalizedWidth.value,
}));


const hasDraftText = computed(() => draftText.value.length > 0);


const hasCommittedChips = computed(
  () => filtersModel.value.length > 0 || Boolean(qModel.value.trim()),
);


const isIdleChips = computed(
  () =>
    !props.alwaysExpanded &&
    hasCommittedChips.value &&
    !hasDraftText.value,
);


const isDrafting = computed(
  () => hasCommittedChips.value && hasDraftText.value,
);





const idleShellWidthPx = ref<number | null>(null);


const shellPanelStyle = computed(() => {
  if (isIdleChips.value && idleShellWidthPx.value != null) {
    return { width: `${idleShellWidthPx.value}px` };
  }
  return {};
});









const measureIdleShellWidth = async () => {
  await nextTick();
  if (!expanded.value || !isIdleChips.value) {
    idleShellWidthPx.value = null;
    return;
  }
  const field = shellRef.value;
  if (!field) {
    idleShellWidthPx.value = null;
    return;
  }

  const chips = Array.from(
    field.querySelectorAll(".list-filter-chip"),
  ) as HTMLElement[];
  if (!chips.length) {
    idleShellWidthPx.value = null;
    return;
  }

  const gap = 6;
  const caretW = 12;
  const cap = Math.min(
    designWidthPx.value,
    Math.max(80, window.innerWidth - 48),
  );


  idleShellWidthPx.value = Math.ceil(cap);
  await nextTick();

  const cs = getComputedStyle(field);
  const padX =
    (parseFloat(cs.paddingLeft) || 0) + (parseFloat(cs.paddingRight) || 0);
  const borderX =
    (parseFloat(cs.borderLeftWidth) || 0) +
    (parseFloat(cs.borderRightWidth) || 0);
  const chrome = padX + borderX;

  let maxChip = 0;
  let sumChips = 0;
  chips.forEach((c, i) => {
    const w = c.getBoundingClientRect().width;
    maxChip = Math.max(maxChip, w);
    sumChips += w + (i > 0 ? gap : 0);
  });

  const singleRow = sumChips + gap + caretW + chrome;

  let target: number;
  if (singleRow <= cap) {

    target = singleRow;
  } else {

    const fieldRect = field.getBoundingClientRect();
    let contentRight = fieldRect.left;
    chips.forEach((c) => {
      contentRight = Math.max(contentRight, c.getBoundingClientRect().right);
    });
    const inputEl = field.querySelector(
      ".list-query-bar__input",
    ) as HTMLElement | null;
    if (inputEl) {
      contentRight = Math.max(
        contentRight,
        inputEl.getBoundingClientRect().right,
      );
    }
    const padR = parseFloat(cs.paddingRight) || 0;
    const borderR = parseFloat(cs.borderRightWidth) || 0;
    target = contentRight - fieldRect.left + padR + borderR;
    if (!Number.isFinite(target) || target < 48) {
      target = maxChip + gap + caretW + chrome;
    }
  }

  idleShellWidthPx.value = Math.max(48, Math.min(cap, Math.ceil(target)));
};

watch(
  [expanded, filtersModel, qModel, hasDraftText, designWidthPx],
  () => {
    void measureIdleShellWidth();
  },
  { deep: true, flush: "post" },
);


const resolvedPlaceholder = computed(() =>
  hasCommittedChips.value ? "" : (props.placeholder ?? ""),
);


const qChipText = computed(() => qModel.value.trim());

const qChipTitle = computed(() => {
  const q = qModel.value.trim();
  if (!q) return "";
  return `${t("listQueryBar.hover.activeQ")}: ${q}`;
});

const fieldOptions = computed(() =>
  props.catalog ? catalogFilterableFields(props.catalog) : [],
);

const suggestItems = computed<SuggestItem[]>(() => {
  if (!expanded.value || !suggestOpen.value) return [];

  if (!isFullMode.value || !props.catalog) return [];
  return buildSuggestItems({
    catalog: props.catalog,
    t,
    draftText: draftText.value,
    opLabel: (op) => {
      const key = `listFilters.ops.${op}`;
      const label = t(key);
      return label === key ? op : label;
    },
    opDetail: (op) => {
      const key = `listQueryBar.opDetail.${op}`;
      const label = t(key);
      return label === key ? undefined : label;
    },
  });
});

let suggestRafId: number | null = null;
let suggestResizeObserver: ResizeObserver | null = null;
let lastSuggestTop = -1;
let lastSuggestLeft = -1;
let lastSuggestWidth = -1;

const updateSuggestPosition = () => {
  const el = shellRef.value;
  if (!el) return;
  const rect = el.getBoundingClientRect();

  const width = Math.round(Math.max(rect.width, 200));
  const maxLeft = Math.max(8, window.innerWidth - width - 8);
  const left = Math.round(Math.min(Math.max(8, rect.left), maxLeft));
  const top = Math.round(rect.bottom + 4);

  if (
    top === lastSuggestTop &&
    left === lastSuggestLeft &&
    width === lastSuggestWidth
  ) {
    return;
  }
  lastSuggestTop = top;
  lastSuggestLeft = left;
  lastSuggestWidth = width;

  suggestStyle.value = {
    position: "fixed",
    top: `${top}px`,
    left: `${left}px`,
    width: `${width}px`,
    zIndex: "4000",
  };
};

const stopSuggestPositionLoop = () => {
  if (suggestRafId != null) {
    cancelAnimationFrame(suggestRafId);
    suggestRafId = null;
  }
};

const startSuggestPositionLoop = () => {
  stopSuggestPositionLoop();
  const tick = () => {
    if (!expanded.value || !suggestOpen.value || !suggestItems.value.length) {
      suggestRafId = null;
      return;
    }
    updateSuggestPosition();
    suggestRafId = requestAnimationFrame(tick);
  };
  updateSuggestPosition();
  suggestRafId = requestAnimationFrame(tick);
};

watch(suggestItems, (items) => {
  if (!items.length) {
    suggestIndex.value = -1;
    stopSuggestPositionLoop();
    return;
  }
  if (suggestIndex.value < 0 || suggestIndex.value >= items.length) {
    suggestIndex.value = 0;
  }
  nextTick(() => startSuggestPositionLoop());
});

watch([expanded, suggestOpen, filtersModel, qModel], () => {
  if (expanded.value && suggestOpen.value && suggestItems.value.length) {
    nextTick(() => startSuggestPositionLoop());
  } else {
    stopSuggestPositionLoop();
  }
});

const onViewportChange = () => {
  if (expanded.value && suggestOpen.value) {
    updateSuggestPosition();
  }
  void measureIdleShellWidth();
};

const suggestIcon = (kind: SuggestKind): string => {
  switch (kind) {
    case "field":
      return "ri:file-list-3-line";
    case "op":
      return "ri:code-box-line";
    case "enum":
      return "ri:list-check";
    case "hint":
      return "ri:information-line";
    default:
      return "ri:filter-3-line";
  }
};

const fieldLabel = (f: FilterFieldDef) => {
  const label = t(f.label_key);
  return label === f.label_key ? f.field : label;
};

const opLabel = (op: FilterOp) => {
  const key = `listFilters.ops.${op}`;
  const label = t(key);
  return label === key ? op : label;
};

const chipText = (f: ListFilter) => {
  const def = fieldOptions.value.find((x) => x.field === f.field);
  const field = def ? fieldLabel(def) : f.field;
  return formatFilterChipText({
    fieldLabel: field,
    opLabel: opLabel(f.op),
    value: f.value,
    unit: def?.unit,
  });
};

interface CommittedChipItem {
  id: string;
  text: string;
  title: string;
  onRemove: () => void;
}

const committedChipItems = computed<CommittedChipItem[]>(() => {
  const items: CommittedChipItem[] = filtersModel.value.map((f, index) => {
    const text = chipText(f);
    return {
      id: `chip-${f.field}-${f.op}-${index}`,
      text,
      title: text,
      onRemove: () => removeFilterAt(index),
    };
  });
  if (qModel.value.trim()) {
    items.push({
      id: "chip-q",
      text: qChipText.value,
      title: qChipTitle.value,
      onRemove: clearQ,
    });
  }
  return items;
});

const primaryChipItem = computed<CommittedChipItem | null>(
  () => committedChipItems.value[0] ?? null,
);

const overflowChipCount = computed(() =>
  Math.max(0, committedChipItems.value.length - 1),
);

const overflowChipsRef = ref<HTMLElement | null>(null);
const overflowContentWidthPx = ref<number | null>(null);

const overflowContentStyle = computed(() =>
  overflowContentWidthPx.value != null
    ? { width: `${overflowContentWidthPx.value}px` }
    : {},
);

const measureOverflowContentWidth = async () => {
  overflowContentWidthPx.value = null;
  await nextTick();
  const chipsEl = overflowChipsRef.value;
  if (!chipsEl) return;
  const chipEls = Array.from(
    chipsEl.querySelectorAll(".list-filter-chip"),
  ) as HTMLElement[];
  if (!chipEls.length) return;
  const baseLeft = chipsEl.getBoundingClientRect().left;
  let maxRight = baseLeft;
  for (const chip of chipEls) {
    maxRight = Math.max(maxRight, chip.getBoundingClientRect().right);
  }
  const exactWidth = Math.ceil(maxRight - baseLeft);
  if (Number.isFinite(exactWidth) && exactWidth > 0) {
    overflowContentWidthPx.value = Math.max(140, Math.min(296, exactWidth));
  }
};

watch(
  committedChipItems,
  () => {
    void measureOverflowContentWidth();
  },
  { deep: true, flush: "post" },
);

const emitSearch = () => {
  emit("change", { q: qModel.value, filters: filtersModel.value });
  emit("search");
};

const setFilters = (next: ListFilter[], search = true) => {
  filtersModel.value = next;
  if (search) emitSearch();
};

const clearLiveCommitTimer = () => {
  if (liveCommitTimer != null) {
    clearTimeout(liveCommitTimer);
    liveCommitTimer = null;
  }
};





const commitParsedFilter = (filter: ListFilter): boolean => {
  if (!props.catalog) return false;
  const next = mergeByFieldOp(filtersModel.value, filter);
  if (next.length > maxFilters(props.catalog)) {
    ElMessage.warning(
      t("listFilters.tooMany", { max: maxFilters(props.catalog) }),
    );
    return false;
  }
  setFilters(next, true);
  void afterFilterCommit();
  return true;
};









const tryLiveCommit = (): boolean => {
  if (!props.liveCommit) return false;
  if (composing.value) return false;
  const text = draftText.value.trim();
  if (!text) return false;

  if (isFullMode.value && props.catalog) {
    const maps = buildAliasMaps(props.catalog, t);
    const fieldLocked = matchFieldRefAtStart(text, maps) !== null;

    if (looksLikeStrongClause(text, props.catalog, t, maps)) {
      const result = parseClause(text, props.catalog, t, maps);
      if (!result.ok) return false;
      clearLiveCommitTimer();
      return commitParsedFilter(result.filter);
    }


    if (fieldLocked) {
      return false;
    }
  }


  if (qModel.value.trim() === text) {
    if (draftText.value.trim()) {
      void afterFilterCommit();
    }
    return true;
  }
  clearLiveCommitTimer();
  qModel.value = text;
  emitSearch();
  void afterFilterCommit();
  return true;
};


const tryLiveCommitCompleteClause = (): boolean => tryLiveCommit();

const scheduleLiveCommit = () => {
  clearLiveCommitTimer();
  if (!props.liveCommit) return;
  liveCommitTimer = setTimeout(() => {
    liveCommitTimer = null;
    tryLiveCommit();
  }, LIVE_COMMIT_MS);
};

const handleExpand = async () => {
  expanded.value = true;
  suggestOpen.value = true;
  parseError.value = null;
  await nextTick();
  inputRef.value?.focus?.();
  updateSuggestPosition();
};


const handleCollapse = () => {
  clearLiveCommitTimer();
  if (!props.alwaysExpanded) {
    expanded.value = false;
  }
  suggestOpen.value = false;
  suggestIndex.value = -1;
};

const isEventInsideQueryUi = (target: EventTarget | null): boolean => {
  if (!(target instanceof Element)) return false;
  if (rootRef.value?.contains(target)) return true;

  if (target.closest(".list-query-bar__suggest")) return true;
  if (target.closest(".list-query-bar__overflow-popper")) return true;
  return false;
};

const onDocumentPointerDown = (event: Event) => {
  if (!expanded.value) return;
  if (isEventInsideQueryUi(event.target)) return;

  suggestOpen.value = false;
  suggestIndex.value = -1;
};

const onInputFocus = () => {
  suggestOpen.value = true;
  nextTick(() => updateSuggestPosition());
};


const onFieldMouseDown = (event: MouseEvent) => {
  const target = event.target;
  if (!(target instanceof Element)) return;
  if (target.closest(".list-filter-chip__remove")) return;
  if (target.closest(".list-query-bar__more-badge")) return;
  if (target.closest(".el-input")) return;
  event.preventDefault();
  inputRef.value?.focus?.();
};

onMounted(() => {
  document.addEventListener("mousedown", onDocumentPointerDown, true);
  document.addEventListener("touchstart", onDocumentPointerDown, true);
  window.addEventListener("resize", onViewportChange);
  window.addEventListener("scroll", onViewportChange, true);
  if (typeof ResizeObserver !== "undefined") {
    suggestResizeObserver = new ResizeObserver(() => {
      if (expanded.value && suggestOpen.value) {
        updateSuggestPosition();
      }
    });
    if (shellRef.value) suggestResizeObserver.observe(shellRef.value);
    if (rootRef.value?.parentElement) {
      suggestResizeObserver.observe(rootRef.value.parentElement);
    }
  }
});

onBeforeUnmount(() => {
  clearLiveCommitTimer();
  stopSuggestPositionLoop();
  suggestResizeObserver?.disconnect();
  suggestResizeObserver = null;
  document.removeEventListener("mousedown", onDocumentPointerDown, true);
  document.removeEventListener("touchstart", onDocumentPointerDown, true);
  window.removeEventListener("resize", onViewportChange);
  window.removeEventListener("scroll", onViewportChange, true);
});

const onDraftInput = () => {
  parseError.value = null;
  suggestOpen.value = true;
  if (suggestItems.value.length) {
    suggestIndex.value = 0;
  }
  nextTick(() => updateSuggestPosition());
  scheduleLiveCommit();
};

const onDraftClear = () => {
  clearLiveCommitTimer();
  draftText.value = "";
  parseError.value = null;
  suggestOpen.value = true;
  nextTick(() => updateSuggestPosition());
};

const onCompositionEnd = () => {
  composing.value = false;

  scheduleLiveCommit();
};

const onInputKeydown = (event: Event | KeyboardEvent) => {
  if (composing.value) return;
  if (!(event instanceof KeyboardEvent)) return;

  const items = suggestItems.value;
  if (event.key === "ArrowDown") {
    if (!items.length) {
      suggestOpen.value = true;
      return;
    }
    event.preventDefault();
    suggestOpen.value = true;
    suggestIndex.value =
      suggestIndex.value < 0
        ? 0
        : (suggestIndex.value + 1) % items.length;
    return;
  }
  if (event.key === "ArrowUp") {
    if (!items.length) return;
    event.preventDefault();
    suggestIndex.value =
      suggestIndex.value <= 0
        ? items.length - 1
        : suggestIndex.value - 1;
    return;
  }
  if (event.key === "Escape") {
    event.preventDefault();

    suggestOpen.value = false;
    suggestIndex.value = -1;
    return;
  }
  if (event.key === "Backspace") {
    if (draftText.value === "" && filtersModel.value.length > 0) {
      event.preventDefault();
      removeFilterAt(filtersModel.value.length - 1);
    } else if (
      draftText.value === "" &&
      filtersModel.value.length === 0 &&
      qModel.value.trim()
    ) {
      event.preventDefault();
      clearQ();
    }
    return;
  }
  if (event.key === "Enter") {
    event.preventDefault();
    clearLiveCommitTimer();
    const selected =
      suggestOpen.value && suggestIndex.value >= 0
        ? items[suggestIndex.value]
        : undefined;



    if (selected && selected.apply.type === "set_draft") {
      applySuggest(selected);
      return;
    }

    commitDraft();
  }
};

const formatParseError = (code: string, params?: Record<string, string | number>) => {
  const key = `listQueryBar.parse.${code}`;
  const msg = t(key, params ?? {});
  return msg === key ? code : msg;
};





const afterFilterCommit = async () => {
  clearLiveCommitTimer();
  draftText.value = "";
  parseError.value = null;
  suggestOpen.value = true;
  suggestIndex.value = 0;
  expanded.value = true;
  await nextTick();
  inputRef.value?.focus?.();
  updateSuggestPosition();
};

const commitDraft = () => {
  const text = draftText.value.trim();
  if (!text) {

    emitSearch();
    return;
  }

  if (isFullMode.value && props.catalog) {
    if (looksLikeStrongClause(text, props.catalog, t)) {
      const result = parseClause(text, props.catalog, t);
      if (result.ok) {
        commitParsedFilter(result.filter);
        return;
      }
      parseError.value = formatParseError(
        result.code,
        result.messageParams,
      );
      suggestOpen.value = true;
      suggestIndex.value = 0;
      return;
    }
  }


  qModel.value = text;
  emitSearch();
  void afterFilterCommit();
};

const applySuggest = (item: SuggestItem) => {
  const apply = item.apply;
  if (apply.type === "noop") return;
  if (apply.type === "set_draft") {
    clearLiveCommitTimer();
    draftText.value = apply.draft;
    parseError.value = null;

    if (tryLiveCommitCompleteClause()) {
      return;
    }
    suggestOpen.value = true;
    suggestIndex.value = 0;
    nextTick(() => {
      inputRef.value?.focus?.();
      updateSuggestPosition();
    });
  }
};

const removeFilterAt = (index: number) => {
  if (index < 0 || index >= filtersModel.value.length) return;
  const next = filtersModel.value.slice();
  next.splice(index, 1);
  setFilters(next, true);
  nextTick(() => updateSuggestPosition());
};

const clearQ = () => {
  if (!qModel.value.trim()) return;
  qModel.value = "";
  emitSearch();
  nextTick(() => updateSuggestPosition());
};

const clearAllQuery = () => {
  clearLiveCommitTimer();
  qModel.value = "";
  filtersModel.value = [];
  draftText.value = "";
  parseError.value = null;
  suggestOpen.value = true;
  suggestIndex.value = -1;

  expanded.value = true;
  emitSearch();
  nextTick(() => {
    inputRef.value?.focus?.();
    updateSuggestPosition();
  });
};
</script>

<style scoped lang="scss">
.list-query-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  height: 32px;

  &.is-expanded {
    flex: 0 0 auto;
    align-items: center;
    max-width: 100%;
  }
}

.list-query-bar__trigger {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  width: 32px;
  height: 32px;
  padding: 0;
  border: none;

  border-radius: var(--el-border-radius-base);
  background: transparent;
  color: var(--art-gray-700);
  cursor: pointer;
  flex-shrink: 0;
  transition:
    background-color 0.2s ease,
    color 0.2s ease;

  &:hover,
  &.is-active {
    background: color-mix(in srgb, var(--theme-color) 8%, transparent);
    color: var(--art-gray-900);
  }

  &.is-expanded-btn {
    width: auto;
    height: 32px;
    padding: 0 10px;
  }
}

.list-query-bar__trigger-text {
  white-space: nowrap;
  font-size: 13px;
  line-height: 1;
}

.list-query-bar__badge {
  position: absolute;
  top: 2px;
  right: 2px;
  min-width: 14px;
  height: 14px;
  padding: 0 3px;
  border-radius: var(--el-border-radius-base);
  background: var(--el-color-primary);
  color: #fff;
  font-size: 10px;
  line-height: 14px;
  text-align: center;

  &--inline {
    position: static;
    margin-left: 2px;
  }
}

.list-query-bar__panel {
  position: relative;
  display: flex;
  flex: 0 0 auto;
  flex-direction: column;
  width: var(--list-query-bar-width, 240px);
  max-width: min(100vw - 48px, var(--list-query-bar-width, 240px));
  min-width: 0;
  height: 32px;
  box-sizing: border-box;

  &.has-chips {
    width: max(var(--list-query-bar-width, 240px), 280px);
    max-width: min(100vw - 48px, max(var(--list-query-bar-width, 240px), 280px));
  }

  &.is-idle-chips {
    width: fit-content;
  }
}

.list-query-bar__field {
  display: flex;
  flex-wrap: nowrap;
  align-items: center;
  gap: 6px;
  width: 100%;
  max-width: 100%;
  min-width: 0;
  height: 32px;
  min-height: 32px;
  max-height: 32px;
  padding: 0 8px;
  border: 1px solid var(--el-border-color);
  border-radius: var(--el-border-radius-base);
  background: var(--el-fill-color-blank);
  box-sizing: border-box;
  overflow: hidden;
  transition: border-color 0.15s ease;

  &:focus-within {
    border-color: var(--el-color-primary);
  }
}

.list-query-bar__chips {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  flex: 0 1 auto;
  min-width: 0;
  max-width: calc(100% - 76px);
}

.list-query-bar__inline-chip {
  max-width: 120px;
}

.list-query-bar__more-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  height: 22px;
  padding: 0 6px;
  border: 1px solid color-mix(in srgb, var(--el-color-primary) 28%, transparent);
  border-radius: var(--el-border-radius-base);
  background: color-mix(in srgb, var(--el-color-primary) 12%, transparent);
  color: var(--el-color-primary);
  font-size: 11px;
  font-weight: 600;
  line-height: 20px;
  cursor: pointer;
  user-select: none;
  box-sizing: border-box;
  transition: background-color 0.15s ease;

  &:hover {
    background: color-mix(in srgb, var(--el-color-primary) 20%, transparent);
  }
}

.list-query-bar__prefix-icon {
  flex-shrink: 0;
  color: var(--el-text-color-placeholder);
  font-size: 15px;
}

.list-query-bar__input {
  --el-input-width: auto;

  flex: 1 1 56px;
  min-width: 48px !important;
  width: auto !important;
  max-width: 100%;

  .list-query-bar__field.is-idle-chips & {
    flex: 0 0 auto;
    flex-grow: 0;
    flex-shrink: 0;
    min-width: 0.6em !important;
    width: 0.6em !important;
    max-width: 0.6em !important;
  }

  &.has-draft {
    flex: 1 1 56px;
    min-width: 48px !important;
    width: auto !important;
    max-width: 100%;
  }

  :deep(.el-input__wrapper) {
    width: 100% !important;
    min-width: 0 !important;
    max-width: 100% !important;
    height: 24px;
    min-height: 24px;
    padding-left: 0 !important;
    padding-right: 4px !important;
    border-radius: 0 !important;
    background: transparent !important;
    background-color: transparent !important;
    box-shadow: none !important;

    &:hover,
    &:focus,
    &.is-focus,
    &:focus-within {
      background: transparent !important;
      background-color: transparent !important;
      box-shadow: none !important;
    }
  }

  :deep(.el-input__inner) {
    height: 24px;
    line-height: 24px;
    min-width: 0 !important;
    width: 100% !important;
    padding: 0 !important;
  }

  &:not(.has-draft) {
    :deep(.el-input__suffix) {
      display: none !important;
    }
  }

  :deep(.el-input__suffix) {
    color: var(--art-gray-500);
  }
}

.list-query-bar__clear {
  display: inline-flex;
  align-items: center;
  flex-shrink: 0;
  height: 32px;
  margin: 0;
  padding: 0 4px;
  align-self: center;
}

.list-query-bar__error {
  position: absolute;
  top: calc(100% + 2px);
  left: 0;
  z-index: 20;
  margin: 0;
  padding: 2px 6px;
  border-radius: var(--el-border-radius-base);
  background: var(--el-bg-color-overlay);
  box-shadow: var(--el-box-shadow-light);
  color: var(--el-color-danger);
  font-size: 12px;
  line-height: 1.4;
  white-space: nowrap;
}

.list-query-bar__hover {
  max-width: 320px;
}

.list-query-bar__hover-title {
  margin: 4px 0;
  font-size: 12px;
  font-weight: 600;
  opacity: 0.9;
}

.list-query-bar__hover-line {
  margin-bottom: 4px;
  font-size: 12px;
  line-height: 1.45;
  word-break: break-word;
}

.list-query-bar__hover-list {
  margin: 0;
  padding: 0;
  list-style: none;
}

.list-query-bar__hover-item {
  padding: 2px 0;
  font-size: 12px;
  line-height: 1.45;
  word-break: break-word;
}

@media (max-width: 900px) {
  .list-query-bar.is-expanded {
    max-width: 100%;
  }

  .list-query-bar__panel {
    width: min(100%, var(--list-query-bar-width, 240px));
    max-width: 100%;

    &.is-idle-chips {
      width: fit-content;
    }
  }
}
</style>


<style lang="scss">
.list-query-bar__hover-popper.el-popper {
  max-width: 340px;
}

.list-query-bar__overflow-popper.el-popper {
  width: max-content !important;
  min-width: 0 !important;
  max-width: 320px;
  padding: 10px 12px !important;
}

.list-query-bar__overflow-content {
  display: inline-flex;
  flex-direction: column;
  gap: 8px;
  width: max-content;
  max-width: 296px;
}

.list-query-bar__overflow-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  width: 100%;

  .el-button {
    padding: 0 !important;
    margin: 0 !important;
    height: auto !important;
  }
}

.list-query-bar__overflow-title {
  color: var(--el-text-color-secondary);
  font-size: 12px;
  font-weight: 600;
}

.list-query-bar__overflow-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.list-query-bar__overflow-chip {
  max-width: 240px !important;
}

.list-query-bar__suggest {
  margin: 0;
  padding: 4px 0;
  list-style: none;
  max-height: min(320px, 50vh);
  overflow: auto;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: var(--el-border-radius-base);
  background: var(--el-bg-color-overlay, var(--el-bg-color));
  box-shadow: var(--el-box-shadow-light);
  box-sizing: border-box;
}

.list-query-bar__suggest-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  font-size: 13px;
  line-height: 1.4;
  cursor: pointer;

  &:hover,
  &.is-active {
    background: color-mix(in srgb, var(--theme-color) 10%, transparent);
  }

  &.is-hint {
    color: var(--el-text-color-secondary);
    cursor: default;

    &:hover {
      background: transparent;
    }
  }
}

.list-query-bar__suggest-icon {
  display: inline-flex;
  flex-shrink: 0;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border-radius: var(--el-border-radius-base);
  background: color-mix(in srgb, var(--el-color-warning) 18%, transparent);
  color: var(--el-color-warning-dark-2, var(--el-color-warning));
  font-size: 14px;
}

.list-query-bar__suggest-main {
  min-width: 0;
  flex: 0 1 auto;
}

.list-query-bar__suggest-label {
  display: block;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-weight: 500;
  color: var(--art-gray-900);
}

.list-query-bar__suggest-detail {
  flex: 1 1 auto;
  min-width: 0;
  margin-left: auto;
  text-align: right;
  color: var(--el-text-color-secondary);
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
