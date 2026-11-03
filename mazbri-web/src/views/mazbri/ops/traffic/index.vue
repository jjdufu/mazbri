<template>
  <OpsMergedShell
    :title="t('menus.mazbri.opsTraffic')"
    :tabs="tabs"
    :default-tab="defaultTab"
  />
</template>

<script setup lang="ts">
import { useI18n } from "vue-i18n";
import OpsMergedShell from "@/components/ops/OpsMergedShell.vue";
import { useMergedShellTabs } from "@/composables/ops/useMergedShellTabs";

defineOptions({ name: "OpsTraffic" });

const LABEL_MAP = {
  red: "menus.mazbri.trafficRed",
  latency: "menus.mazbri.trafficLatency",
  errors: "menus.mazbri.trafficErrors",
} as const;

const { t } = useI18n();
const { tabs, defaultTab } = useMergedShellTabs("traffic", LABEL_MAP);
</script>
