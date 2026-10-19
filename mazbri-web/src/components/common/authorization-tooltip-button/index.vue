<template>
  <ElTooltip :disabled="!disabled || !disabledReason" :content="disabledReason">
    <span>
      <ElButton
        :type="type"
        :loading="loading"
        :disabled="disabled"
        :link="link"
        :plain="plain"
        :text="text"
        :size="size"
        @click="emit('click')"
      >
        <template v-if="icon" #icon>
          <ArtSvgIcon :icon="icon" />
        </template>
        <slot />
      </ElButton>
    </span>
  </ElTooltip>
</template>

<script setup lang="ts">
defineOptions({ name: "AuthorizationTooltipButton" });

interface Props {
  type?: "primary" | "success" | "warning" | "danger" | "info";
  size?: "large" | "default" | "small";
  disabled?: boolean;
  disabledReason?: string;
  loading?: boolean;
  link?: boolean;
  plain?: boolean;
  text?: boolean;
  icon?: string;
}

const props = withDefaults(defineProps<Props>(), {
  type: undefined,
  size: undefined,
  disabled: false,
  disabledReason: undefined,
  loading: false,
  link: false,
  plain: false,
  text: false,
  icon: undefined,
});

const emit = defineEmits<{
  click: [];
}>();

const {
  type,
  size,
  disabled,
  disabledReason,
  loading,
  link,
  plain,
  text,
  icon,
} = toRefs(props);
</script>
