<template>
  <OpsMergedShell
    :title="t('menus.mazbri.opsReliability')"
    :tabs="tabs"
    :default-tab="defaultTab"
  />
</template>

<script setup lang="ts">
import { useI18n } from "vue-i18n";
import OpsMergedShell from "@/components/ops/OpsMergedShell.vue";
import { useMergedShellTabs } from "@/composables/ops/useMergedShellTabs";

defineOptions({ name: "OpsReliability" });

const LABEL_MAP = {
  capacity: "menus.mazbri.capacity",
} as const;

const { t } = useI18n();
const { tabs, defaultTab } = useMergedShellTabs("reliability", LABEL_MAP);
</script>
