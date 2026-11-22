<template>
  <div class="wb-metabar" :inert="!canWrite || undefined">
    <label class="wb-metabar__field wb-metabar__field--tag">
      <span class="wb-metabar__label">{{ $t("upstreamConnector.tagLabel") }}</span>
      <input
        v-model="endpoint.tag"
        type="text"
        list="wb-ep-tag-datalist"
        class="wb-metabar__input"
        :placeholder="$t('upstreamConnector.tagPlaceholder')"
        :aria-label="$t('upstreamConnector.tagLabel')"
        spellcheck="false"
      />
      <datalist id="wb-ep-tag-datalist">
        <option v-for="t in existingTags" :key="t" :value="t" />
      </datalist>
    </label>

    <span class="wb-metabar__sep" aria-hidden="true" />

    <label class="wb-metabar__field wb-metabar__field--name">
      <span class="wb-metabar__label">{{ $t("upstreamConnector.fields.epName") }}</span>
      <input
        v-model="endpoint.name"
        type="text"
        class="wb-metabar__input"
        :placeholder="$t('upstreamConnector.fields.epName')"
        :aria-label="$t('upstreamConnector.fields.epName')"
        spellcheck="false"
      />
    </label>

    <span class="wb-metabar__sep" aria-hidden="true" />

    <label class="wb-metabar__field wb-metabar__field--mono">
      <span class="wb-metabar__label">{{ $t("upstreamConnector.fields.channelCode") }}</span>
      <input
        v-model="endpoint.channel_code"
        type="text"
        class="wb-metabar__input is-mono"
        :placeholder="suggestChannelCode(endpoint, activeIndex)"
        :aria-label="$t('upstreamConnector.fields.channelCode')"
        spellcheck="false"
      />
    </label>

    <span class="wb-metabar__sep" aria-hidden="true" />

    <label class="wb-metabar__field wb-metabar__field--mono">
      <ElTooltip
        :content="$t('upstreamConnector.pathPrefixTooltip')"
        placement="top"
      >
        <span class="wb-metabar__label is-help">{{
          $t("upstreamConnector.gatewayPrefixLabel")
        }}</span>
      </ElTooltip>
      <input
        v-model="endpoint.path_prefix"
        type="text"
        class="wb-metabar__input is-mono"
        :placeholder="endpoint.path || '/'"
        :aria-label="$t('upstreamConnector.fields.pathPrefix')"
        spellcheck="false"
      />
    </label>

    <div class="wb-metabar__right">
      <button
        type="button"
        class="wb-metabar__gate"
        :class="{ on: endpoint.gate !== false }"
        :title="
          endpoint.gate === false
            ? $t('upstreamConnector.gateOffHint')
            : $t('upstreamConnector.gateOnHint')
        "
        @click="endpoint.gate = endpoint.gate === false"
      >
        <span class="wb-metabar__gate-dot" />
        <span>{{ $t("upstreamConnector.gate") }}</span>
      </button>

      <WorkbenchAdapterChip
        :adapter="endpoint.adapter || 'http_json'"
        :encoding="resolvedBodyEncoding(endpoint)"
        :append-to="appendTo"
        :disabled="!canWrite"
        @select="emit('select-adapter', $event)"
        @edit="emit('edit-adapter')"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import type { ConnectorEndpoint } from "@/api/upstream-connectors";
import { resolvedBodyEncoding, suggestChannelCode } from "./workbenchHelpers";
import WorkbenchAdapterChip from "./hopp/WorkbenchAdapterChip.vue";

const props = defineProps<{
  endpoint: ConnectorEndpoint;
  endpoints: ConnectorEndpoint[];
  activeIndex: number;
  canWrite: boolean;
  appendTo?: HTMLElement;
}>();

const emit = defineEmits<{
  "select-adapter": [string];
  "edit-adapter": [];
}>();

const existingTags = computed(() => {
  const s = new Set<string>();
  for (const ep of props.endpoints) {
    const t = (ep.tag || "").trim();
    if (t) s.add(t);
  }
  return [...s];
});
</script>

<style scoped lang="scss">
.wb-metabar {
  display: flex;
  flex-shrink: 0;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
  min-height: 34px;
  padding: 4px 12px;
  background: var(--wb-surface);
  border-bottom: 1px solid var(--wb-divider-light);
}

.wb-metabar__sep {
  width: 1px;
  height: 14px;
  background: var(--wb-divider);
}

.wb-metabar__field {
  display: inline-flex;
  gap: 6px;
  align-items: center;
  min-width: 0;

  &--tag {
    flex: 0 1 130px;
  }

  &--name {
    flex: 1 1 150px;
  }

  &--mono {
    flex: 1 1 160px;
  }
}

.wb-metabar__label {
  flex-shrink: 0;
  font-size: 11px;
  font-weight: 500;
  color: var(--wb-text-faint);
  white-space: nowrap;

  &.is-help {
    cursor: help;
    border-bottom: 1px dashed var(--wb-divider);
  }
}

.wb-metabar__input {
  flex: 1;
  min-width: 56px;
  height: 24px;
  padding: 0 6px;
  font-size: 12px;
  color: var(--wb-text);
  background: transparent;
  border: 1px solid transparent;
  border-radius: var(--wb-radius);
  outline: none;

  &.is-mono {
    font-family: var(--wb-font-mono);
    font-size: 11.5px;
  }

  &:hover {
    background: var(--wb-surface-input);
  }

  &:focus {
    background: var(--wb-surface);
    border-color: var(--wb-divider);
  }

  &::placeholder {
    color: var(--wb-text-faint);
  }
}

.wb-metabar__right {
  display: inline-flex;
  flex-shrink: 0;
  gap: 6px;
  align-items: center;
  margin-left: auto;

  :deep(.wb-adapter-chip) {
    min-height: 26px;
    padding: 2px 8px;
    font-size: 11.5px;
  }
}

.wb-metabar__gate {
  display: inline-flex;
  gap: 5px;
  align-items: center;
  height: 26px;
  padding: 0 8px;
  font-size: 11.5px;
  font-weight: 500;
  color: var(--wb-text-muted);
  cursor: pointer;
  background: var(--wb-surface-subtle);
  border: 1px solid var(--wb-divider);
  border-radius: var(--wb-radius);

  &.on {
    color: var(--wb-text);
  }
}

.wb-metabar__gate-dot {
  width: 6px;
  height: 6px;
  background: transparent;
  border: 1px solid var(--wb-text-faint);
  border-radius: 50%;
}

.wb-metabar__gate.on .wb-metabar__gate-dot {
  background: var(--wb-success);
  border-color: var(--wb-success);
}
</style>
