<template>
  <button
    v-if="visible"
    type="button"
    class="cert-expiry-badge"
    @click="openCerts"
  >
    <ElTag type="warning" effect="dark">
      {{ t("ops.overview.certExpiryBadge", { days: displayDays }) }}
    </ElTag>
  </button>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import {
  CERT_LIST_PATH,
  shouldShowCertExpiryBadge,
} from "@/utils/ops/cert-badge";

defineOptions({ name: "OpsOverviewCertExpiryBadge" });

const props = defineProps<{
  days?: number | null;
}>();

const { t } = useI18n();
const router = useRouter();

const visible = computed(() => shouldShowCertExpiryBadge(props.days));

const displayDays = computed(() => {
  const days = props.days;
  if (typeof days !== "number" || !Number.isFinite(days)) return "—";
  return String(Math.floor(days));
});

function openCerts() {
  void router.push(CERT_LIST_PATH);
}
</script>

<style scoped>
.cert-expiry-badge {
  padding: 0;
  border: none;
  background: transparent;
  cursor: pointer;
}
</style>
