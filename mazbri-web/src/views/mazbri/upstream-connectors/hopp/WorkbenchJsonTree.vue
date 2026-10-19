<template>
  <div class="wb-json-node" :class="{ 'is-root': depth === 0 }">
    <div v-if="depth === 0" class="wb-json__filter">
      <input
        v-model="filterText"
        type="search"
        class="wb-json__filter-input"
        :placeholder="$t('upstreamConnector.jsonFilterPlaceholder')"
        spellcheck="false"
      />
    </div>
    <div
      v-if="depth === 0 && filterText.trim() && !rootVisible"
      class="wb-json__filter-empty"
    >
      {{ $t("upstreamConnector.jsonFilterEmpty") }}
    </div>
    <template v-else-if="isLeaf">
      <span v-if="hasLabel" :class="labelClass">{{ labelText }}</span>
      <span v-if="hasLabel" class="wb-json__colon">: </span>
      <span class="wb-json__leaf" :class="leafClass">{{ leafText }}</span>
    </template>
    <template v-else>
      <button
        v-if="children.length"
        type="button"
        class="wb-json__toggle"
        :title="open ? 'collapse' : 'expand'"
        @click="open = !open"
      >
        <ArtSvgIcon :icon="open ? 'ri:arrow-down-s-line' : 'ri:arrow-right-s-line'" />
      </button>
      <span v-else class="wb-json__toggle-spacer" />
      <span v-if="hasLabel" :class="labelClass">{{ labelText }}</span>
      <span v-if="hasLabel" class="wb-json__colon">: </span>
      <span class="wb-json__bracket">{{ openBracket }}</span>
      <template v-if="!open">
        <span class="wb-json__preview"> {{ children.length }} </span>
        <span class="wb-json__bracket">{{ closeBracket }}</span>
      </template>
      <template v-else>
        <div v-for="(child, i) in visibleChildren" :key="i" class="wb-json__child">
          <WorkbenchJsonTree
            :value="child.value"
            :label="child.key"
            :index="child.index"
            :depth="depth + 1"
            :filter="effectiveFilter"
          />
        </div>
        <span class="wb-json__bracket">{{ closeBracket }}</span>
      </template>
    </template>
  </div>
</template>

<script setup lang="ts">
import { $t } from "@/locales";

defineOptions({ name: "WorkbenchJsonTree" });

const props = withDefaults(
  defineProps<{
    value: unknown;
    label?: string | number | null;
    index?: boolean;
    depth?: number;
    filter?: string;
  }>(),
  {
    label: null,
    index: false,
    depth: 0,
    filter: "",
  },
);

const filterText = ref(props.depth === 0 ? "" : props.filter);

const open = ref(true);

const isLeaf = computed(
  () => props.value === null || typeof props.value !== "object",
);

const hasLabel = computed(() => props.label != null && props.label !== "");

const labelText = computed(() =>
  props.index ? String(props.label) : JSON.stringify(String(props.label ?? "")),
);

const labelClass = computed(() =>
  props.index ? "wb-json__index" : "wb-json__key",
);

const isArray = computed(() => Array.isArray(props.value));

type Child = { key: string | number; value: unknown; index: boolean };

const children = computed<Child[]>(() => {
  const v = props.value;
  if (Array.isArray(v)) {
    return v.map((value, i) => ({ key: i, value, index: true }));
  }
  if (v && typeof v === "object") {
    return Object.entries(v as Record<string, unknown>).map(([key, value]) => ({
      key,
      value,
      index: false,
    }));
  }
  return [];
});

const effectiveFilter = computed(() =>
  props.depth === 0 ? filterText.value : props.filter || "",
);

function nodeMatches(key: string | number, value: unknown, q: string): boolean {
  if (!q) return true;
  const needle = q.toLowerCase();
  if (String(key).toLowerCase().includes(needle)) return true;
  if (value == null || typeof value !== "object") {
    return String(value).toLowerCase().includes(needle);
  }
  if (Array.isArray(value)) {
    return value.some((v, i) => nodeMatches(i, v, q));
  }
  return Object.entries(value as Record<string, unknown>).some(([k, v]) =>
    nodeMatches(k, v, q),
  );
}

const visibleChildren = computed(() => {
  const q = effectiveFilter.value.trim();
  if (!q) return children.value;
  return children.value.filter((c) => nodeMatches(c.key, c.value, q));
});

const rootVisible = computed(() => {
  const q = effectiveFilter.value.trim();
  if (!q) return true;
  if (props.label != null && nodeMatches(props.label, props.value, q)) {
    return true;
  }
  return visibleChildren.value.length > 0 || nodeMatches("", props.value, q);
});

const openBracket = computed(() => (isArray.value ? "[" : "{"));
const closeBracket = computed(() => (isArray.value ? "]" : "}"));

const leafClass = computed(() => {
  const v = props.value;
  if (v === null) return "is-null";
  const t = typeof v;
  if (t === "string") return "is-string";
  if (t === "number") return "is-number";
  if (t === "boolean") return "is-bool";
  return "is-null";
});

const leafText = computed(() => {
  const v = props.value;
  if (v === null) return "null";
  if (typeof v === "string") return JSON.stringify(v);
  if (typeof v === "boolean" || typeof v === "number") return String(v);
  return String(v);
});
</script>

<style scoped lang="scss">
.wb-json-node {
  font-family: var(--wb-font-mono, ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace);
  font-size: 12px;
  line-height: 1.6;
  color: var(--wb-text, var(--el-text-color-primary));
}
.wb-json-node.is-root {
  padding: 8px 12px;
}
.wb-json__filter {
  margin: 0 0 8px;
}
.wb-json__filter-input {
  width: 100%;
  height: 28px;
  padding: 0 8px;
  border: 1px solid var(--wb-divider-subtle, var(--el-border-color-extra-light));
  border-radius: var(--wb-radius, 4px);
  background: var(--wb-surface-input, var(--el-fill-color-light));
  color: var(--wb-text, var(--el-text-color-primary));
  font-size: 12px;
  outline: none;
  &:focus {
    border-color: var(--wb-accent, var(--el-color-primary));
  }
}
.wb-json__filter-empty {
  font-size: 12px;
  color: var(--wb-text-muted, var(--el-text-color-secondary));
  padding: 12px 0;
}
.wb-json__toggle {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  margin-right: 2px;
  padding: 0;
  border: none;
  background: transparent;
  color: var(--wb-text-faint, var(--el-text-color-placeholder));
  cursor: pointer;
  vertical-align: middle;
  border-radius: var(--wb-radius, var(--art-form-control-radius));
  &:hover {
    color: var(--wb-text, var(--el-text-color-primary));
    background: var(--wb-surface-hover, var(--el-fill-color));
  }
}
.wb-json__toggle-spacer {
  display: inline-block;
  width: 18px;
}
.wb-json__key {
  color: var(--wb-json-key, var(--el-text-color-secondary));
}
.wb-json__index {
  color: var(--wb-text-faint, var(--el-text-color-placeholder));
}
.wb-json__colon,
.wb-json__bracket,
.wb-json__preview {
  color: var(--wb-text-muted, var(--el-text-color-secondary));
}
.wb-json__leaf {
  word-break: break-all;
  &.is-string {
    color: var(--wb-json-string, #22c55e);
  }
  &.is-number {
    color: var(--wb-json-number, #3b82f6);
  }
  &.is-bool {
    color: var(--wb-json-bool, #a855f7);
  }
  &.is-null {
    color: var(--wb-json-null, var(--el-text-color-placeholder));
  }
}
.wb-json__child {
  padding-left: 16px;
}
</style>
