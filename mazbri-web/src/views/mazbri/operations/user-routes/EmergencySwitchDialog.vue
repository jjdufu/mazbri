<template>
  <EditDialog
    v-model="visible"
    :title="$t('routes.dialogs.emergencySwitchTitle')"
    width="480px"
    :saving="saving"
    :confirm-text="$t('routes.actions.confirmSwitch')"
    confirm-type="danger"
    @confirm="emit('confirm')"
  >
    <ElForm label-width="92px">
      <ElFormItem :label="$t('routes.fields.targetBinding')">
        <ElInput :model-value="channelName" disabled />
      </ElFormItem>
      <ElFormItem :label="$t('routes.fields.switchReason')">
        <ElInput
          v-model="reason"
          type="textarea"
          :rows="4"
          :placeholder="$t('routes.placeholders.switchReason')"
        />
      </ElFormItem>
    </ElForm>
  </EditDialog>
</template>

<script setup lang="ts">
import EditDialog from "@/components/common/EditDialog.vue";

defineProps<{
  saving: boolean;
  channelName: string;
}>();

const visible = defineModel<boolean>({ required: true });
const reason = defineModel<string>("reason", { required: true });

const emit = defineEmits<{
  confirm: [];
}>();
</script>
