<template>
  <ResourceTableShell
    :columns="[]"
    :loading="loading"
    :data="presets"
    :show-toolbar="false"
    table-class="surface-panel-table"
    stripe
    border
    @selection-change="handleSelectionChange"
  >
    <template #header>
      <SurfaceCardHeader :title="$t('certificate.presets.list.title')">
        <template #actions>
          <ElTooltip
            :disabled="!authorizationDisabled"
            :content="authorizationDisabledReason"
          >
            <span>
              <ElButton
                :disabled="authorizationDisabled"
                @click="emit('import')"
              >
                <template #icon><ArtSvgIcon icon="ri:upload-line" /></template>
                {{ $t("certificate.presets.actions.import") }}
              </ElButton>
            </span>
          </ElTooltip>
          <TableExportDropdown
            :items="exportItems"
            :label="$t('certificate.presets.actions.export')"
            :disabled="authorizationDisabled || presets.length === 0"
            :disabled-reason="
              authorizationDisabled ? authorizationDisabledReason : undefined
            "
            @command="(type) => emit('export', type as 'all' | 'selected')"
          />
          <ElTooltip
            :disabled="!authorizationDisabled"
            :content="authorizationDisabledReason"
          >
            <span>
              <ElButton
                type="primary"
                :disabled="authorizationDisabled"
                @click="emit('create')"
              >
                <template #icon><ArtSvgIcon icon="ri:add-line" /></template>
                {{ $t("certificate.presets.actions.create") }}
              </ElButton>
            </span>
          </ElTooltip>
          <ElTooltip
            :disabled="!authorizationDisabled"
            :content="authorizationDisabledReason"
          >
            <span>
              <TableDeleteDropdown
                :total-count="presets.length"
                :selected-count="selectedCount"
                :disabled="authorizationDisabled"
                @command="(type) => emit('delete-command', type)"
              />
            </span>
          </ElTooltip>
        </template>
      </SurfaceCardHeader>
    </template>

    <template #empty>
      <ElEmpty :description="$t('certificate.presets.empty')" />
    </template>

    <template #default>
      <ElTableColumn type="selection" width="55" />
      <ElTableColumn
        prop="service_name"
        :label="$t('certificate.presets.columns.serviceName')"
        width="150"
      />
      <ElTableColumn
        prop="dns_names"
        :label="$t('certificate.presets.columns.dnsNames')"
        min-width="300"
      >
        <template #default="{ row }">
          <ElTag
            v-for="(name, idx) in row.dns_names?.slice(0, 5)"
            :key="name + idx"
            size="small"
            class="mr-1 mb-1"
          >
            {{ name }}
          </ElTag>
          <ElTag v-if="row.dns_names?.length > 5" size="small" type="info">
            +{{ row.dns_names.length - 5 }}
          </ElTag>
        </template>
      </ElTableColumn>
      <ElTableColumn
        prop="description"
        :label="$t('certificate.presets.columns.description')"
        min-width="200"
      >
        <template #default="{ row }">
          {{ row.description || "-" }}
        </template>
      </ElTableColumn>
      <ElTableColumn
        prop="created_at"
        :label="$t('certificate.presets.columns.createdAt')"
        width="180"
      >
        <template #default="{ row }">
          {{ formatDate(row.created_at) }}
        </template>
      </ElTableColumn>
      <ElTableColumn
        :label="$t('common.fields.operations')"
        width="150"
        fixed="right"
      >
        <template #default="{ row }">
          <CommonInlineRow gap="8px">
            <ElButton
              link
              type="primary"
              :disabled="authorizationDisabled"
              @click="emit('edit', row)"
              >{{ $t("common.actions.modify") }}</ElButton
            >
            <ElButton
              link
              type="danger"
              :disabled="authorizationDisabled"
              @click="emit('delete', row)"
              >{{ $t("common.actions.delete") }}</ElButton
            >
          </CommonInlineRow>
        </template>
      </ElTableColumn>
    </template>
  </ResourceTableShell>
</template>

<script setup lang="ts">
import { $t } from "@/locales";
import type { PresetDnsName } from "@/types/api/certificate";
import ResourceTableShell from "@/components/common/resource-table-shell/index.vue";
import TableDeleteDropdown from "@/components/table/TableDeleteDropdown.vue";
import TableExportDropdown from "@/components/table/TableExportDropdown.vue";
import CommonInlineRow from "@/components/common/inline-row/index.vue";
import SurfaceCardHeader from "@/components/common/surface-card-header/index.vue";
import { getLicenseDisabledReason } from "@/composables/useLicenseGate";
import { formatDateTime as formatDate } from "@/utils/format";
interface Props {
  loading: boolean;
  presets: PresetDnsName[];
  selectedCount: number;
  authorizationDisabled?: boolean;
  authorizationDisabledReason?: string;
}
const props = withDefaults(defineProps<Props>(), {
  authorizationDisabled: false,
  authorizationDisabledReason: getLicenseDisabledReason(),
});
const {
  loading,
  presets,
  selectedCount,
  authorizationDisabled,
  authorizationDisabledReason,
} = toRefs(props);
const exportItems = computed(() => [
  { command: "all", label: $t("certificate.presets.actions.exportAll") },
  {
    command: "selected",
    label: $t("certificate.presets.actions.exportSelected", {
      count: selectedCount.value,
    }),
    disabled: selectedCount.value === 0,
  },
]);
const emit = defineEmits<{
  create: [];
  import: [];
  export: [type: "all" | "selected"];
  edit: [row: PresetDnsName];
  delete: [row: PresetDnsName];
  "delete-command": [type: "all" | "selected"];
  "selection-change": [rows: PresetDnsName[]];
}>();
const handleSelectionChange = (rows: PresetDnsName[]) => {
  emit("selection-change", rows);
};
</script>
