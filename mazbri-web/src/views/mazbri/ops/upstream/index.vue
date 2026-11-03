<template>
  <OpsMergedShell
    :title="t('menus.mazbri.opsUpstream')"
    :tabs="tabs"
    :default-tab="defaultTab"
  />
</template>

<script setup lang="ts">
import { useI18n } from "vue-i18n";
import OpsMergedShell from "@/components/ops/OpsMergedShell.vue";
import { useMergedShellTabs } from "@/composables/ops/useMergedShellTabs";

defineOptions({ name: "OpsUpstream" });

const LABEL_MAP = {
  sla: "menus.mazbri.upstreamSla",
  breaker: "menus.mazbri.upstreamBreaker",
} as const;

const { t } = useI18n();
const { tabs, defaultTab } = useMergedShellTabs("upstream", LABEL_MAP);
</script>
