<template>
  <div class="wb-auth__inject">
    <WorkbenchGhostField
      :label="$t('supplier.config.injectLocation')"
      kind="select"
      :model-value="flow.inject_to || 'header'"
      :options="injectToOptions"
      :append-to="appendTo"
      :disabled="disabled"
      @update:model-value="flow.inject_to = String($event)"
    />
    <WorkbenchGhostField
      :label="$t('supplier.config.injectKey')"
      :model-value="flow.inject_key"
      placeholder="Authorization"
      :disabled="disabled"
      @update:model-value="flow.inject_key = String($event)"
    />
    <WorkbenchGhostField
      :label="$t('supplier.config.injectFormat')"
      :model-value="flow.inject_value_format"
      placeholder="Bearer {}"
      :disabled="disabled"
      @update:model-value="flow.inject_value_format = String($event)"
    />
    <div class="wb-section wb-section--nested">
      <button
        type="button"
        class="wb-section__head is-toggle"
        @click="previewOpen = !previewOpen"
      >
        <span class="wb-section__title">{{
          $t("supplier.config.injectPreview")
        }}</span>
      </button>
      <div v-show="previewOpen" class="wb-section__body">
        <InjectPreviewCard
          :inject-to="flow.inject_to"
          :inject-key="flow.inject_key"
          :inject-value-format="flow.inject_value_format"
        />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { $t } from "@/locales";
import type { AuthFlowForm } from "@/views/mazbri/suppliers/components/SupplierTransparentConfig.vue";
import InjectPreviewCard from "@/views/mazbri/suppliers/components/InjectPreviewCard.vue";
import WorkbenchGhostField from "./WorkbenchGhostField.vue";

defineProps<{
  flow: AuthFlowForm;
  disabled?: boolean;
  appendTo?: HTMLElement | string;
}>();

const previewOpen = ref(false);
const injectToOptions = [
  { label: $t("upstreamConnector.injectToHeader"), value: "header" },
  { label: $t("upstreamConnector.injectToQuery"), value: "query" },
  { label: $t("upstreamConnector.injectToMetadata"), value: "metadata" },
  { label: $t("upstreamConnector.injectToFirstFrame"), value: "first_frame" },
];
</script>
