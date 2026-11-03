<template>
  <OpsMergedShell
    :title="t('menus.mazbri.opsFlow')"
    :tabs="tabs"
    :default-tab="defaultTab"
  />
</template>

<script setup lang="ts">
import { useI18n } from "vue-i18n";
import OpsMergedShell from "@/components/ops/OpsMergedShell.vue";
import { useMergedShellTabs } from "@/composables/ops/useMergedShellTabs";

defineOptions({ name: "OpsFlow" });

const LABEL_MAP = {
  ratelimit: "menus.mazbri.flowRatelimit",
  cache: "menus.mazbri.flowCache",
} as const;

const { t } = useI18n();
const { tabs, defaultTab } = useMergedShellTabs("flow", LABEL_MAP);
</script>
