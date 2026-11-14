<template>
  <span
    v-if="loaded"
    class="mfa-required-hint"
    :class="{ 'is-block': block, 'is-warning': !mfaEnabled }"
  >
    <ArtSvgIcon
      :icon="mfaEnabled ? 'ri:shield-keyhole-line' : 'ri:error-warning-line'"
      class="mfa-required-hint__icon"
    />
    <span class="mfa-required-hint__body">
      <span>
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
  </span>
</template>

<script setup lang="ts">
import { computed, onMounted } from "vue";
import { storeToRefs } from "pinia";
import { useMfaStepUpStore } from "@/store/modules/mfa-step-up";
import { usePersonalSecurityStore } from "@/store/modules/personal-security";

defineOptions({ name: "MfaRequiredHint" });

withDefaults(defineProps<{ block?: boolean }>(), { block: false });

const stepUpStore = useMfaStepUpStore();
const personalSecurityStore = usePersonalSecurityStore();
const { mfaEnabled: enabled, loaded } = storeToRefs(stepUpStore);
const mfaEnabled = computed(() => enabled.value);

onMounted(() => {
  if (!loaded.value) void stepUpStore.refresh().catch(() => undefined);
});

function goToSetup(): void {
  personalSecurityStore.openDrawer("mfa");
}
</script>

<style scoped lang="scss">
/* [不可更改] 盾牌图标固定左列 + 右侧弹性文本流，严禁 flex-wrap: wrap 导致盾牌单独断行占一行 */
.mfa-required-hint {
  display: flex;
  align-items: flex-start;
  flex-wrap: nowrap;
  gap: 8px;
  padding: 8px 12px;
  border-radius: var(--el-border-radius-base);
  background-color: var(--el-fill-color-light);
  color: var(--el-text-color-regular);
  font-size: 12px;
  line-height: 1.5;

  &.is-block {
    width: 100%;
    box-sizing: border-box;
  }

  &__icon {
    flex-shrink: 0;
    margin-top: 2px;
    font-size: 14px;
    color: var(--el-color-primary);
  }

  &__body {
    flex: 1;
    min-width: 0;
  }

  &__link {
    margin-left: 4px;
    font-size: 12px;
    vertical-align: baseline;
  }

  &.is-warning {
    color: var(--el-color-warning);

    .mfa-required-hint__icon {
      color: var(--el-color-warning);
    }
  }
}
</style>
