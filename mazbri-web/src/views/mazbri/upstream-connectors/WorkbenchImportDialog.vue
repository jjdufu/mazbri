<template>
  <CenteredDialog
    v-model="visible"
    :title="$t('upstreamConnector.import')"
    width="640px"
  >
    <ElRadioGroup v-if="showMode" v-model="mode" class="wb-import__mode">
      <ElRadioButton value="merge">{{ $t("upstreamConnector.importMerge") }}</ElRadioButton>
      <ElRadioButton value="replace">{{ $t("upstreamConnector.importReplace") }}</ElRadioButton>
    </ElRadioGroup>
    <ElTabs v-model="importTab">
      <ElTabPane label="cURL" name="curl">
        <ElInput
          v-model="importCurlText"
          type="textarea"
          :rows="10"
          placeholder="curl -X GET 'https://api.example.com/posts' ..."
        />
      </ElTabPane>
      <ElTabPane label="Postman" name="postman">
        <ElInput
          v-model="importJsonText"
          type="textarea"
          :rows="10"
          placeholder="Paste Postman Collection v2.1 JSON"
        />
      </ElTabPane>
      <ElTabPane label="OpenAPI" name="openapi">
        <ElInput
          v-model="importJsonText"
          type="textarea"
          :rows="10"
          placeholder="Paste OpenAPI 3 JSON"
        />
      </ElTabPane>
    </ElTabs>
    <div class="wb-import__file">
      <input ref="fileRef" type="file" accept=".json,.txt,.curl" hidden @change="onFile" />
      <ElButton @click="fileRef?.click()">
        <ArtSvgIcon icon="ri:upload-line" class="mr-1" />
        {{ $t("upstreamConnector.importFile") }}
      </ElButton>
    </div>
    <template #footer>
      <ElButton @click="visible = false">{{ $t("common.cancel") }}</ElButton>
      <ElButton type="primary" :loading="importing" @click="emit('confirm')">
        {{ $t("common.confirm") }}
      </ElButton>
    </template>
  </CenteredDialog>
</template>

<script setup lang="ts">
import CenteredDialog from "@/components/common/CenteredDialog.vue";

withDefaults(
  defineProps<{
    importing: boolean;
    showMode?: boolean;
  }>(),
  { showMode: true },
);

const visible = defineModel<boolean>({ required: true });
const importTab = defineModel<string>("importTab", { required: true });
const importCurlText = defineModel<string>("importCurlText", { required: true });
const importJsonText = defineModel<string>("importJsonText", { required: true });
const mode = defineModel<string>("mode", { default: "merge" });

const emit = defineEmits<{
  confirm: [];
}>();

const fileRef = ref<HTMLInputElement | null>(null);

function onFile(e: Event) {
  const f = (e.target as HTMLInputElement).files?.[0];
  if (!f) return;
  const reader = new FileReader();
  reader.onload = () => {
    const text = String(reader.result || "");
    if (importTab.value === "curl") importCurlText.value = text;
    else importJsonText.value = text;
  };
  reader.readAsText(f);
  (e.target as HTMLInputElement).value = "";
}
</script>

<style scoped>
.wb-import__mode {
  margin-bottom: 10px;
}
.wb-import__file {
  margin-top: 8px;
}
</style>
