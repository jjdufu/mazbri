<template>
  <ControlPanelPageShell
    :title="$t('menus.mazbri.customerCallMonitor')"
    icon="ri:user-received-line"
  >
    <template #subtitle>
      <CallMonitorModeHint :hint="modeHint" />
    </template>
    <div style="display: flex; flex-direction: column; flex: 1; min-height: 0;">
      <CallMonitorTable
        mode="customer"
        :title="$t('operations.monitoring.customerTitle')"
        @mode-hint-change="modeHint = $event"
      />
    </div>
  </ControlPanelPageShell>
</template>

<script setup lang="ts">
import { ref } from "vue";
import ControlPanelPageShell from "@/components/common/control-panel-page-shell/index.vue";
import CallMonitorTable from "../modules/CallMonitorTable.vue";
import CallMonitorModeHint, {
  type CallMonitorModeHintPayload,
} from "../modules/CallMonitorModeHint.vue";
import { $t } from "@/locales";
defineOptions({ name: "CustomerCallMonitor" });

const modeHint = ref<CallMonitorModeHintPayload | null>(null);
</script>
