<template>
  <OpsMergedShell
    :title="t('menus.mazbri.opsAlerts')"
    :tabs="tabs"
    :default-tab="defaultTab"
  />
</template>

<script setup lang="ts">
import { useI18n } from "vue-i18n";
import OpsMergedShell from "@/components/ops/OpsMergedShell.vue";
import { useMergedShellTabs } from "@/composables/ops/useMergedShellTabs";

defineOptions({ name: "OpsAlerts" });

const LABEL_MAP = {
  events: "menus.mazbri.alertEvents",
  rules: "menus.mazbri.alertRules",
  silences: "menus.mazbri.alertSilences",
} as const;

const { t } = useI18n();
const { tabs, defaultTab } = useMergedShellTabs("alerts", LABEL_MAP);
</script>
