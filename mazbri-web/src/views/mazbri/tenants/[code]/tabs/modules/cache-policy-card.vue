<template>
  <ElCard shadow="hover" class="strategy-card cache-policy-card">
    <template #header>
      <SurfaceCardHeader stack-on-mobile>
        <template #left>
          <div class="header-title">
            <span>{{ $t("tenant.cachePolicy.title") }}</span>
          </div>
        </template>
        <template #actions>
          <ElButton
            v-roles="['R_ADMIN', 'R_FINANCE']"
            :loading="saving"
            type="primary"
            @click="handleSave"
          >
            {{ $t("common.actions.save") }}
          </ElButton>
        </template>
      </SurfaceCardHeader>
    </template>

    <div v-if="loading" v-loading="loading" class="loading-state" />

    <template v-else>
      <ElForm :model="form" label-position="top" class="strategy-form">
        <div class="switch-banner-panel">
          <div class="switch-label">
            <span>{{ $t("tenant.cachePolicy.fields.enabled") }}</span>
          </div>
          <CommonSwitchRow>
            <ElSwitch v-model="form.enabled" />
            <span class="switch-text ml-2">
              {{
                form.enabled
                  ? $t("rateLimit.enabledOn")
                  : $t("rateLimit.enabledOff")
              }}
            </span>
          </CommonSwitchRow>
        </div>

        <div class="switch-banner-panel">
          <div class="switch-label">
            <span>{{ $t("tenant.cachePolicy.fields.isolated") }}</span>
          </div>
          <CommonSwitchRow>
            <ElSwitch v-model="form.tenant_isolated" :disabled="!form.enabled" />
            <span class="switch-text ml-2">
              {{
                form.tenant_isolated
                  ? $t("rateLimit.enabledOn")
                  : $t("rateLimit.enabledOff")
              }}
            </span>
          </CommonSwitchRow>
        </div>

        <div class="settings-panel mb-4">
          <div class="form-row-grid">

            <div class="form-row-item">
              <div class="form-row-label">
                <span>{{ $t("tenant.cachePolicy.fields.ttl") }}</span>
              </div>
              <div class="form-row-content">
                <ElInputNumber v-model="form.default_ttl" :min="0" />
              </div>
            </div>
            <div class="form-row-item">
              <div class="form-row-label">
                <span>{{ $t("tenant.cachePolicy.fields.maxSize") }}</span>
              </div>
              <div class="form-row-content">
                <ElInputNumber v-model="form.max_cache_size" :min="0" />
              </div>
            </div>
          </div>
        </div>

        <div class="form-section-block">
          <div class="section-actions-row">
            <span class="section-title-label">{{
              $t("tenant.cachePolicy.warmupList")
            }}</span>
            <ElButton
              v-roles="['R_ADMIN', 'R_FINANCE']"
              link
              type="primary"
              @click="addWarmupKey"
            >
              <ArtSvgIcon icon="ri:add-circle-line" class="mr-1" />
              {{ $t("tenant.cachePolicy.newWarmupKeyPlaceholder") }}
            </ElButton>
          </div>

          <ArtTable
            :data="warmupKeysList"
            :columns="warmupKeysColumns"
            border
            stripe
            empty-height="auto"
            class="warmup-table"
          >
            <template #key="{ row }">
              <ElInput
                v-model="row.key"
                :placeholder="$t('tenant.cachePolicy.fields.warmupKeys')"
              />
            </template>
            <template #operations="{ $index }">
              <TableActionRow>
                <ElButton
                  v-roles="['R_ADMIN', 'R_FINANCE']"
                  link
                  type="danger"
                  @click="removeWarmupKey($index)"
                >
                  <ArtSvgIcon icon="ri:delete-bin-line" />
                </ElButton>
              </TableActionRow>
            </template>
          </ArtTable>
        </div>

        <div class="form-section-block">
          <div class="section-actions-row">
            <span class="section-title-label">{{
              $t("tenant.cachePolicy.apiOverrides")
            }}</span>
            <ElButton
              v-roles="['R_ADMIN', 'R_FINANCE']"
              link
              type="primary"
              @click="addOverride"
            >
              <ArtSvgIcon icon="ri:add-circle-line" class="mr-1" />
              {{ $t("tenant.cachePolicy.addRule") }}
            </ElButton>
          </div>

          <ArtTable
            :data="overrideList"
            :columns="overrideColumns"
            border
            stripe
            empty-height="auto"
            class="override-table"
          >
            <template #path="{ row }">
              <ElInput
                v-model="row.path"
                :placeholder="$t('tenant.cachePolicy.apiPathPlaceholder')"
              />
            </template>
            <template #ttl="{ row }">
              <ElInputNumber v-model="row.ttl" :min="0">
                <template #suffix>{{
                  $t("tenant.cachePolicy.seconds")
                }}</template>
              </ElInputNumber>
            </template>
            <template #operations="{ $index }">
              <TableActionRow>
                <ElButton
                  v-roles="['R_ADMIN', 'R_FINANCE']"
                  link
                  type="danger"
                  @click="removeOverride($index)"
                >
                  <ArtSvgIcon icon="ri:delete-bin-line" />
                </ElButton>
              </TableActionRow>
            </template>
          </ArtTable>
        </div>
      </ElForm>
    </template>
  </ElCard>
</template>

<script setup lang="ts">
import { ElMessage } from "element-plus";
import type { TenantCacheConfig } from "@/types/api/tenant";
import { fetchTenantCacheConfig, updateTenantCacheConfig } from "@/api/tenant";
import SurfaceCardHeader from "@/components/common/surface-card-header/index.vue";
import { useUserStore } from "@/store/modules/user";
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
.cache-policy-card {
  height: 100%;

  .strategy-form {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 10px;
  }

  .switch-banner-panel {
    display: flex;
    justify-content: space-between;
    align-items: center;
    background: var(--el-fill-color-blank);
    border: 1px solid var(--el-border-color-light);
    border-radius: var(--custom-radius);
    padding: 12px 16px;
    transition: all 0.2s ease;

    &:hover {
      border-color: var(--el-border-color-hover);
    }

    .switch-label {
      display: flex;
      align-items: center;
      font-size: 14px;
      font-weight: 600;
      color: var(--el-text-color-primary);
    }

    .switch-text {
      font-size: 13px;
      color: var(--el-text-color-secondary);
    }
  }

  .header-title {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 600;
    font-size: 16px;
  }

  .header-icon {
    font-size: 20px;
    color: var(--el-color-primary);
  }

  .settings-panel {
    // Flat section inside the card: no nested bordered/gray box.
    background: transparent;
    border: none;
    border-radius: 0;
    padding: 4px 0 0;
  }

  .form-row-grid {
    display: grid;
    grid-template-columns: 1fr;
    gap: 16px;
  }

  .form-row-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    height: 32px;

    .form-row-label {
      font-size: 13px;
      font-weight: 600;
      color: var(--el-text-color-regular);
      display: flex;
      align-items: center;
      white-space: nowrap;
    }

    .form-row-content {
      flex: 1;
      max-width: 180px;
      display: flex;
      justify-content: flex-end;
      align-items: center;

      :deep(.el-input-number) {
        width: 100%;
        min-width: 120px;
      }

      &.align-right {
        justify-content: flex-end;
      }
    }

    &.is-empty {
      @media (max-width: 768px) {
        display: none;
      }
    }
  }

  .warmup-table {
    width: 100%;

    :deep(.el-input__wrapper) {
      box-shadow: none !important;
      background: transparent;
    }
  }

  .form-section-block {
    margin-top: 0;
  }

  .section-actions-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 8px;

    .section-title-label {
      font-size: 14px;
      font-weight: 600;
      color: var(--el-text-color-primary);
    }
  }

  .override-table {
    width: 100%;

    :deep(.el-input__wrapper) {
      box-shadow: none !important;
      background: transparent;
    }
  }

  .loading-state {
    height: 200px;
  }
}

@media (max-width: 768px) {
  .cache-policy-card {
    .form-row-grid {
      grid-template-columns: 1fr;
      gap: 12px;
    }

    .form-row-item {
      height: auto;

      .form-row-content {
        max-width: 160px;
      }
    }
  }
}
</style>
