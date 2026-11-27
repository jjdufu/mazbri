<template>
  <div class="entity-identity-bar">
    <div class="entity-identity-bar__main">
      <span class="entity-identity-bar__name" :title="name || code">
        {{ name || code || "—" }}
      </span>
      <span v-if="code" class="entity-identity-bar__code">
        <CopyableText :value="code" :label="codeLabel" />
      </span>
      <StatusIndicator
        v-if="statusText"
        :type="statusType"
        class="entity-identity-bar__status"
      >
        {{ statusText }}
      </StatusIndicator>
      <div v-if="$slots.tags" class="entity-identity-bar__tags">
        <slot name="tags" />
      </div>
    </div>
    <div v-if="$slots.actions" class="entity-identity-bar__actions">
      <slot name="actions" />
    </div>
  </div>
</template>

<script setup lang="ts">
import CopyableText from "@/components/common/copyable-text/index.vue";
import StatusIndicator from "@/components/common/StatusIndicator.vue";

defineOptions({ name: "EntityIdentityBar" });

export type EntityStatusType = "success" | "warning" | "danger" | "info" | "primary";

withDefaults(
  defineProps<{
    name?: string;
    code?: string;
    codeLabel?: string;
    statusType?: EntityStatusType;
    statusText?: string;
  }>(),
  {
    name: "",
    code: "",
    codeLabel: "",
    statusType: "info",
    statusText: "",
  },
);
</script>

<style scoped lang="scss">
.entity-identity-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  min-width: 0;
  min-height: 32px;
  width: 100%;
}

.entity-identity-bar__main {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 10px;
  min-width: 0;
  flex: 1 1 auto;
}

.entity-identity-bar__name {
  font-size: 16px;
  font-weight: 600;
  line-height: 22px;
  color: var(--el-text-color-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 360px;
}

.entity-identity-bar__code {
  display: inline-flex;
  align-items: center;
  flex-shrink: 0;
  font-size: 12px;
  color: var(--el-text-color-secondary);
}

.entity-identity-bar__status {
  flex-shrink: 0;
}

.entity-identity-bar__tags {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

.entity-identity-bar__actions {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}
</style>
