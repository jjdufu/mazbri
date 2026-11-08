<template>
  <SettingsCard
    v-loading="loading"
    icon="ri:database-2-line"
    :title="$t('tenant.cachePolicy.title')"
  >
    <template #actions>
      <ElButton
        v-roles="ROLES_BIZ"
        :loading="saving"
        type="primary"
        @click="handleSave"
      >
        {{ $t("common.actions.save") }}
      </ElButton>
    </template>

    <CommonSettingRow
      mode="inline"
      mobile-stack
      :label="$t('tenant.cachePolicy.fields.ttl')"
    >
      <ElInputNumber
        v-model="form.default_ttl"
        :min="0"
        :disabled="!canWriteBusiness"
        class="cache-input-number"
      />
    </CommonSettingRow>
    <CommonSettingRow
      mode="inline"
      mobile-stack
      :label="$t('tenant.cachePolicy.fields.maxSize')"
    >
      <ElInputNumber
        v-model="form.max_cache_size"
        :min="0"
        :disabled="!canWriteBusiness"
        class="cache-input-number"
      />
    </CommonSettingRow>
    <SettingsToggleGrid>
      <CommonToggleTile :label="$t('tenant.cachePolicy.fields.enabled')">
        <ElSwitch v-model="form.enabled" :disabled="!canWriteBusiness" />
      </CommonToggleTile>
      <CommonToggleTile :label="$t('tenant.cachePolicy.fields.isolated')">
        <ElSwitch
          v-model="form.tenant_isolated"
          :disabled="!form.enabled || !canWriteBusiness"
        />
      </CommonToggleTile>
    </SettingsToggleGrid>

    <CommonSettingRow mode="block">
      <template #label>
        <div class="table-row-label">
          <span>{{ $t("tenant.cachePolicy.warmupList") }}</span>
          <ElButton
            v-roles="ROLES_BIZ"
            link
            type="primary"
            @click="addWarmupKey"
          >
            <ArtSvgIcon icon="ri:add-circle-line" class="mr-1" />
            {{ $t("tenant.cachePolicy.newWarmupKeyPlaceholder") }}
          </ElButton>
        </div>
      </template>
      <ArtTable
        :data="warmupKeysList"
        :columns="warmupKeysColumns"
        border
        stripe
        empty-height="auto"
        class="inline-edit-table"
      >
        <template #key="{ row }">
          <ElInput
            v-model="row.key"
            :placeholder="$t('tenant.cachePolicy.fields.warmupKeys')"
            :disabled="!canWriteBusiness"
          />
        </template>
        <template #operations="{ $index }">
          <TableActionRow>
            <ElButton
              v-roles="ROLES_BIZ"
              link
              type="danger"
              @click="removeWarmupKey($index)"
            >
              <ArtSvgIcon icon="ri:delete-bin-line" />
            </ElButton>
          </TableActionRow>
        </template>
      </ArtTable>
    </CommonSettingRow>

    <CommonSettingRow mode="block">
      <template #label>
        <div class="table-row-label">
          <span>{{ $t("tenant.cachePolicy.apiOverrides") }}</span>
          <ElButton
            v-roles="ROLES_BIZ"
            link
            type="primary"
            @click="addOverride"
          >
            <ArtSvgIcon icon="ri:add-circle-line" class="mr-1" />
            {{ $t("tenant.cachePolicy.addRule") }}
          </ElButton>
        </div>
      </template>
      <ArtTable
        :data="overrideList"
        :columns="overrideColumns"
        border
        stripe
        empty-height="auto"
        class="inline-edit-table"
      >
        <template #path="{ row }">
          <ElInput
            v-model="row.path"
            :placeholder="$t('tenant.cachePolicy.apiPathPlaceholder')"
            :disabled="!canWriteBusiness"
          />
        </template>
        <template #ttl="{ row }">
          <ElInputNumber
            v-model="row.ttl"
            :min="0"
            :disabled="!canWriteBusiness"
          >
            <template #suffix>{{ $t("tenant.cachePolicy.seconds") }}</template>
          </ElInputNumber>
        </template>
        <template #operations="{ $index }">
          <TableActionRow>
            <ElButton
              v-roles="ROLES_BIZ"
              link
              type="danger"
              @click="removeOverride($index)"
            >
              <ArtSvgIcon icon="ri:delete-bin-line" />
            </ElButton>
          </TableActionRow>
        </template>
      </ArtTable>
    </CommonSettingRow>
  </SettingsCard>
</template>

<script setup lang="ts">
import { ElMessage } from "element-plus";
import type { TenantCacheConfig } from "@/types/api/tenant";
import { fetchTenantCacheConfig, updateTenantCacheConfig } from "@/api/tenant";
import SettingsCard from "@/components/common/settings-card/index.vue";
import SettingsToggleGrid from "@/components/common/settings-toggle-grid/index.vue";
import CommonSettingRow from "@/components/common/setting-row/index.vue";
import CommonToggleTile from "@/components/common/toggle-tile/index.vue";
import { useUserStore } from "@/store/modules/user";
import { ROLES_BIZ } from "@/utils/ops/roles";
import { logger } from "@/utils/sys";
import { $t } from "@/locales";
import type { ColumnOption } from "@/types/component";

const props = defineProps<{
  tenantCode: string;
}>();

const loading = ref(false);
const saving = ref(false);

const userStore = useUserStore();
const canWriteBusiness = computed(() => userStore.canWriteBusiness);

const warmupKeysList = ref<{ key: string }[]>([]);
const warmupKeysColumns = ref<
  ColumnOption<{
    key: string;
  }>[]
>([
  {
    prop: "key",
    label: $t("tenant.cachePolicy.fields.warmupKeys"),
    useSlot: true,
  },
  {
    prop: "operations",
    label: $t("common.fields.operations"),
    width: 80,
    align: "center",
    useSlot: true,
  },
]);

const overrideColumns = ref<
  ColumnOption<{
    path: string;
    ttl: number;
  }>[]
>([
  {
    prop: "path",
    label: $t("tenant.cachePolicy.fields.apiPath"),
    useSlot: true,
  },
  {
    prop: "ttl",
    label: $t("tenant.cachePolicy.fields.overrideTtl"),
    width: 200,
    useSlot: true,
  },
  {
    prop: "operations",
    label: $t("common.fields.operations"),
    width: 80,
    align: "center",
    useSlot: true,
  },
]);

const form = reactive<TenantCacheConfig>({
  tenant_id: "",
  enabled: true,
  default_ttl: 300,
  max_cache_size: 104857600,
  tenant_isolated: true,
  api_ttl: {},
  warmup_enabled: false,
  warmup_keys: [],
});

const overrideList = ref<
  {
    path: string;
    ttl: number;
  }[]
>([]);

const addOverride = () => {
  overrideList.value.push({ path: "", ttl: 300 });
};

const removeOverride = (index: number) => {
  overrideList.value.splice(index, 1);
};

const addWarmupKey = () => {
  warmupKeysList.value.push({ key: "" });
};

const removeWarmupKey = (index: number) => {
  warmupKeysList.value.splice(index, 1);
};

const loadData = async () => {
  loading.value = true;
  try {
    const res = await fetchTenantCacheConfig(props.tenantCode);
    if (res) {
      if (res.default_ttl !== undefined && res.default_ttl !== null)
        form.default_ttl = res.default_ttl;
      if (res.max_cache_size !== undefined && res.max_cache_size !== null)
        form.max_cache_size = res.max_cache_size;
      if (res.tenant_isolated !== undefined && res.tenant_isolated !== null)
        form.tenant_isolated = res.tenant_isolated;
      if (res.warmup_enabled !== undefined && res.warmup_enabled !== null)
        form.warmup_enabled = res.warmup_enabled;
      if (res.warmup_keys) form.warmup_keys = res.warmup_keys;
      if (res.api_ttl) form.api_ttl = res.api_ttl;
      if (res.enabled !== undefined) form.enabled = res.enabled;
      if (res.tenant_id) form.tenant_id = res.tenant_id;
    }
    overrideList.value = Object.entries(form.api_ttl || {}).map(
      ([path, ttl]) => ({
        path,
        ttl: ttl as number,
      }),
    );
    warmupKeysList.value = (form.warmup_keys || []).map((key) => ({
      key,
    }));
  } catch (error) {
    logger.error("Failed to load cache policy:", error);
  } finally {
    loading.value = false;
  }
};

const handleSave = async () => {
  saving.value = true;
  try {
    const api_ttl: Record<string, number> = {};
    overrideList.value.forEach((item) => {
      if (item.path.trim()) {
        api_ttl[item.path.trim()] = item.ttl;
      }
    });
    const warmup_keys = warmupKeysList.value
      .map((item) => item.key.trim())
      .filter(Boolean);
    const warmup_enabled = warmup_keys.length > 0;

    form.warmup_enabled = warmup_enabled;
    form.warmup_keys = warmup_keys;

    const payload = { ...form, api_ttl, warmup_keys, warmup_enabled };
    await updateTenantCacheConfig(props.tenantCode, payload);
    ElMessage.success($t("common.feedback.saveSuccess"));
    loadData();
  } catch (error) {
    logger.error("Failed to save cache policy:", error);
  } finally {
    saving.value = false;
  }
};

onMounted(loadData);
</script>

<style scoped lang="scss">
.table-row-label {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.inline-edit-table {
  width: 100%;

  :deep(.el-input__wrapper) {
    box-shadow: none !important;
    background: transparent;
  }
}
</style>
