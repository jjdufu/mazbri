<template>
  <span class="status-indicator" :class="[typeClass]">
    <span class="status-indicator__dot"></span>
    <span class="status-indicator__text">
      <slot>{{ label }}</slot>
    </span>
  </span>
</template>

<script setup lang="ts">
import { computed } from "vue";

interface Props {
  type?: "success" | "warning" | "danger" | "info" | "primary";
  label?: string;
}

const props = withDefaults(defineProps<Props>(), {
  type: "info",
  label: "",
});

const typeClass = computed(() => `status-indicator--${props.type}`);
</script>

<style lang="scss" scoped>
.status-indicator {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  vertical-align: middle;
  font-size: 13px;
  line-height: 1;

  .status-indicator__dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  &.status-indicator--success {
    color: var(--el-color-success);
    .status-indicator__dot {
      background-color: var(--el-color-success);
    }
  }

  &.status-indicator--warning {
    color: var(--el-color-warning);
    .status-indicator__dot {
      background-color: var(--el-color-warning);
    }
  }

  &.status-indicator--danger {
    color: var(--el-color-danger);
    .status-indicator__dot {
      background-color: var(--el-color-danger);
    }
  }

  &.status-indicator--info {
    color: var(--el-color-info);
    .status-indicator__dot {
      background-color: var(--el-color-info);
    }
  }

  &.status-indicator--primary {
    color: var(--el-color-primary);
    .status-indicator__dot {
      background-color: var(--el-color-primary);
    }
  }
}
</style>
