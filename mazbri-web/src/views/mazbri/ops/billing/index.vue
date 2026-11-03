<template>
  <OpsMergedShell
    :title="t('menus.mazbri.opsBilling')"
    :subtitle="t('ops.layout.billingSubtitle')"
    :tabs="tabs"
    :default-tab="defaultTab"
  >
    <template #health>
      <HealthPanel />
    </template>
    <template #reconcile>
      <ReconcilePanel />
    </template>
  </OpsMergedShell>
</template>

<script setup lang="ts">
import { useI18n } from "vue-i18n";
import OpsMergedShell from "@/components/ops/OpsMergedShell.vue";
import { useMergedShellTabs } from "@/composables/ops/useMergedShellTabs";
import HealthPanel from "./HealthPanel.vue";
import ReconcilePanel from "./ReconcilePanel.vue";

defineOptions({ name: "OpsBilling" });

const LABEL_MAP = {
  health: "menus.mazbri.billingHealth",
  reconcile: "menus.mazbri.billingReconcile",
} as const;

const { t } = useI18n();
const { tabs, defaultTab } = useMergedShellTabs("billing", LABEL_MAP);
</script>
