<template>
  <span class="mfa-required-hint" :class="{ 'is-block': block }">
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
import { computed } from "vue";
import { useRouter } from "vue-router";
import { useUserStore } from "@/store/modules/user";
import { ACCOUNT_SECURITY_PATH } from "@/utils/http/mfa-dialog-service";

defineOptions({ name: "MfaRequiredHint" });

withDefaults(defineProps<{ block?: boolean }>(), { block: false });

const router = useRouter();
const userStore = useUserStore();
// 只用于展示提示；是否真正需要 MFA 以后端每次请求的判定为准。
const mfaEnabled = computed(() => Boolean(userStore.info.mfa_enabled));

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
