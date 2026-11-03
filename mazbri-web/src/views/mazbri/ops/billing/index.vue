<template>
  <OpsMergedShell
    :title="t('menus.mazbri.opsBilling')"
    :tabs="tabs"
    :default-tab="defaultTab"
  />
</template>

<script setup lang="ts">
import { useI18n } from "vue-i18n";
import OpsMergedShell from "@/components/ops/OpsMergedShell.vue";
import { useMergedShellTabs } from "@/composables/ops/useMergedShellTabs";

defineOptions({ name: "OpsBilling" });

const LABEL_MAP = {
  health: "menus.mazbri.billingHealth",
  reconcile: "menus.mazbri.billingReconcile",
} as const;

const { t } = useI18n();
const { tabs, defaultTab } = useMergedShellTabs("billing", LABEL_MAP);
</script>
