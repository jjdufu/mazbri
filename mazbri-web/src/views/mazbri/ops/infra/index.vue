<template>
  <OpsMergedShell
    :title="t('menus.mazbri.opsInfra')"
    :tabs="tabs"
    :default-tab="defaultTab"
  />
</template>

<script setup lang="ts">
import { useI18n } from "vue-i18n";
import OpsMergedShell from "@/components/ops/OpsMergedShell.vue";
import { useMergedShellTabs } from "@/composables/ops/useMergedShellTabs";

defineOptions({ name: "OpsInfra" });

const LABEL_MAP = {
  pipeline: "menus.mazbri.infraPipeline",
  middleware: "menus.mazbri.infraMiddleware",
  hosts: "menus.mazbri.infraHosts",
} as const;

const { t } = useI18n();
const { tabs, defaultTab } = useMergedShellTabs("infra", LABEL_MAP);
</script>
