<template>
  <span
    v-if="loaded"
    class="mfa-required-hint"
    :class="{ 'is-block': block }"
  >
    <ArtSvgIcon
      :icon="mfaEnabled ? 'ri:shield-keyhole-line' : 'ri:error-warning-line'"
      class="mfa-required-hint__icon"
      :class="mfaEnabled ? 'is-muted' : 'is-warning'"
    />
    <span :class="mfaEnabled ? 'is-muted' : 'is-warning'">
      {{
        mfaEnabled
          ? $t("mfaRequiredHint.enabled")
          : $t("mfaRequiredHint.notEnabled")
      }}
    </span>
    <ElLink
      v-if="!mfaEnabled"
      type="primary"
      underline="never"
      class="mfa-required-hint__link"
      @click="goToSetup"
    >
      {{ $t("mfaRequiredHint.goToSetup") }}
    </ElLink>
  </span>
</template>

<script setup lang="ts">
import { computed, onMounted } from "vue";
import { useRouter } from "vue-router";
import { storeToRefs } from "pinia";
import { useMfaStepUpStore } from "@/store/modules/mfa-step-up";
import { ACCOUNT_SECURITY_PATH } from "@/utils/http/mfa-dialog-service";

defineOptions({ name: "MfaRequiredHint" });

withDefaults(defineProps<{ block?: boolean }>(), { block: false });

const router = useRouter();
const stepUpStore = useMfaStepUpStore();
const { mfaEnabled: enabled, loaded } = storeToRefs(stepUpStore);
const mfaEnabled = computed(() => enabled.value);

onMounted(() => {
  if (!loaded.value) void stepUpStore.refresh().catch(() => undefined);
});

function goToSetup(): void {
  void router.push(ACCOUNT_SECURITY_PATH);
}
</script>

<style scoped lang="scss">
.mfa-required-hint {
  display: inline-flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 4px;
  font-size: 12px;
  line-height: 1.5;

  &.is-block {
    display: flex;
  }
}

.mfa-required-hint__icon {
  flex-shrink: 0;
  font-size: 14px;
}

.mfa-required-hint__link {
  font-size: 12px;
}

.is-muted {
  color: var(--el-text-color-secondary);
}

.is-warning {
  color: var(--el-color-warning);
}
</style>
