<template>
  <ControlPanelPageShell :title="''" icon="ri:shield-keyhole-line">
    <div class="authorization-management-page">
      <ElRow :gutter="16">
        <ElCol :xs="24" :lg="10">
          <ElCard shadow="never" class="management-card">
            <template #header>
              <SurfaceCardHeader :title="importSectionTitle" />
            </template>

            <ElForm label-position="top">
              <FilePicker
                :model-value="packageFile"
                accept=".jws,.txt,text/plain,application/jose"
                :title="
                  $t('certificate.authorizationManagement.filePicker.title')
                "
                :hint="
                  $t('certificate.authorizationManagement.filePicker.hint')
                "
                :subtle-hint="
                  $t('certificate.authorizationManagement.filePicker.sizeHint')
                "
                :selected-hint="
                  $t(
                    'certificate.authorizationManagement.filePicker.selectedHint',
                  )
                "
                :action-label="
                  $t('certificate.authorizationManagement.filePicker.choose')
                "
                :change-action-label="
                  $t('certificate.authorizationManagement.filePicker.change')
                "
                :selected-tag-text="
                  $t('certificate.authorizationManagement.filePicker.selected')
                "
                :invalid-type-message="
                  $t(
                    'certificate.authorizationManagement.filePicker.invalidType',
                  )
                "
                empty-icon="ri:upload-cloud-2-line"
                selected-icon="ri:file-text-line"
                @update:model-value="handlePackageFileUpdate"
              />

            </ElForm>

            <div class="button-row">
              <ElButton
                v-for="button in actionButtons"
                :key="button.key"
                :type="button.type"
                :loading="button.loading"
                :disabled="button.disabled"
                @click="button.onClick"
              >
                {{ button.label }}
              </ElButton>
            </div>
          </ElCard>
        </ElCol>

        <ElCol :xs="24" :lg="14">
          <ElCard shadow="never" class="management-card">
            <template #header>
              <SurfaceCardHeader :title="detailSectionTitle" />
            </template>

            <template v-if="hasSummary">
              <ElDescriptions class="mt-4" :column="1" border>
                <ElDescriptionsItem
                  v-for="row in summaryRows"
                  :key="row.label"
                  :label="row.label"
                >
                  <ElTag v-if="row.kind === 'tag'" :type="row.tagType">
                    {{ row.value }}
                  </ElTag>
                  <code v-else-if="row.kind === 'code'" class="hash-value">
                    {{ row.value }}
                  </code>
                  <span v-else>
                    {{ row.value }}
                  </span>
                </ElDescriptionsItem>
              </ElDescriptions>
            </template>

            <template v-else>
              <ElEmpty :description="statusEmptyDescription" />
            </template>
          </ElCard>
        </ElCol>
      </ElRow>
    </div>
  </ControlPanelPageShell>
</template>

<script setup lang="ts">
import ControlPanelPageShell from "@/components/common/control-panel-page-shell/index.vue";
import FilePicker from "@/components/common/file-picker/index.vue";
import SurfaceCardHeader from "@/components/common/surface-card-header/index.vue";
import { useAuthorizationManagementPage } from "@/composables/useAuthorizationManagementPage";

defineOptions({ name: "AuthorizationManagement" });

const page = useAuthorizationManagementPage();
const {
  packageFile,
  hasSummary,
  summaryRows,
  importSectionTitle,
  detailSectionTitle,
  statusEmptyDescription,
  actionButtons,
} = page;

function handlePackageFileUpdate(value: File | File[] | null) {
  if (value instanceof File) {
    packageFile.value = value;
    return;
  }
  if (Array.isArray(value)) {
    packageFile.value = value[0] ?? null;
    return;
  }
  packageFile.value = null;
}
</script>

<style scoped lang="scss">
.authorization-management-page {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.management-card {
  min-height: 100%;
}

.passphrase-item {
  margin-top: 16px;
}

.button-row {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  margin-top: 16px;
}

.hash-value {
  display: inline-block;
  max-width: 100%;
  word-break: break-all;
  white-space: pre-wrap;
}
</style>
