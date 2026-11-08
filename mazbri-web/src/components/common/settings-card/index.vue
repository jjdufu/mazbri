<template>
  <ElCard shadow="hover" class="settings-card">
    <template #header>
      <SurfaceCardHeader stack-on-mobile>
        <template #left>
          <div class="settings-card__title">
            <ArtSvgIcon v-if="icon" :icon="icon" class="settings-card__icon" />
            <span>{{ title }}</span>
            <ElPopover
              v-if="info || $slots.info"
              placement="bottom-start"
              :width="320"
              trigger="click"
            >
              <div class="settings-card__info-popover">
                <slot name="info">{{ info }}</slot>
              </div>
              <template #reference>
                <button
                  type="button"
                  class="settings-card__info-trigger"
                  aria-label="说明"
                >
                  <ArtSvgIcon icon="ri:information-line" />
                </button>
              </template>
            </ElPopover>
            <slot name="title-extra" />
          </div>
        </template>
        <template v-if="$slots.actions" #actions>
          <slot name="actions" />
        </template>
      </SurfaceCardHeader>
    </template>
    <div class="settings-card__body">
      <slot />
    </div>
  </ElCard>
</template>

<script setup lang="ts">
import SurfaceCardHeader from "@/components/common/surface-card-header/index.vue";

/**
 * [不可更改] 控制台设置卡片：表头 = 图标 + 标题（+ title-extra，如状态标签）+ 右侧 actions；
 * 正文 = CommonSettingRow 行（标签 140px 居左、控件居右，所有单行输入/数字步进/下拉控件统一 220px 等宽对齐），
 * 不再嵌套子卡片或卡内小节标题。行与控件尺寸统一由本组件提供，页面/模块严禁各自覆写不同宽度。
 */
defineOptions({ name: "SettingsCard" });

withDefaults(
  defineProps<{
    title: string;
    icon?: string;
    info?: string;
  }>(),
  {
    icon: "",
    info: "",
  },
);
</script>

<style scoped lang="scss">
.settings-card {
  display: flex;
  flex-direction: column;
  min-width: 0;
  overflow: visible;

  :deep(.el-card__header) {
    padding: 20px 20px 0 !important;
    border-bottom: none !important;
  }

  :deep(.el-card__body) {
    flex: 1;
    padding: 0 20px 20px !important;
    overflow: visible !important;
  }
}

.settings-card__title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 15px;
  font-weight: 600;
}

.settings-card__icon {
  font-size: 18px;
  color: var(--el-color-primary);
}

.settings-card__info-trigger {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  padding: 0;
  border: none;
  border-radius: 50%;
  background: transparent;
  color: var(--el-text-color-secondary);
  font-size: 14px;
  cursor: pointer;
  transition: color 0.15s ease, background-color 0.15s ease;

  &:hover {
    color: var(--el-color-primary);
    background: var(--el-fill-color-light);
  }
}

.settings-card__info-popover {
  color: var(--el-text-color-regular);
  font-size: 12px;
  line-height: 1.6;
  white-space: pre-line;
}

.settings-card__body {
  display: flex;
  flex-direction: column;
  gap: 16px;
  margin-top: 8px;
  padding-top: 4px;

  :deep(.common-setting-row--inline) {
    overflow: visible;
  }

  :deep(.common-setting-row__label) {
    width: 140px;
    font-size: 14px;
    font-weight: 500;
    white-space: nowrap;
  }

  :deep(.common-setting-row__control) {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    position: relative;
    overflow: visible;
  }

  :deep(.common-setting-row--block .common-setting-row__label) {
    width: auto;
  }

  :deep(.common-setting-row--block .common-setting-row__control) {
    justify-content: flex-start;
  }

  :deep(.setting-input),
  :deep(.setting-input-number),
  :deep(.setting-select),
  :deep(.rate-input-number),
  :deep(.cache-input-number),
  :deep(.transport-control),
  :deep(.common-setting-row--inline > .common-setting-row__control > .el-input),
  :deep(.common-setting-row--inline > .common-setting-row__control > .el-input-number),
  :deep(.common-setting-row--inline > .common-setting-row__control > .el-select) {
    width: min(100%, 220px) !important;
  }
}

@media (max-width: 768px) {
  .settings-card__body {
    :deep(.common-setting-row--mobile-stack .common-setting-row__label) {
      width: auto;
      white-space: normal;
    }

    :deep(.common-setting-row--mobile-stack .common-setting-row__control) {
      justify-content: flex-start;
      width: 100%;
    }
  }
}
</style>
