<template>
  <SettingsCard
    v-loading="loading"
    icon="ri:speed-line"
    :title="$t('rateLimit.title')"
  >
    <template #actions>
      <ElButton
        v-roles="ROLES_BIZ"
        type="primary"
        :loading="saving"
        @click="handleSave"
      >
        {{ $t("common.actions.save") }}
      </ElButton>
    </template>

    <CommonSettingRow
      mode="inline"
      mobile-stack
      :label="$t('rateLimit.enablePlan')"
    >
      <ElSwitch v-model="form.enabled" :disabled="!canWriteBusiness" />
    </CommonSettingRow>
    <CommonSettingRow
      v-roles="ROLES_BIZ"
      mode="inline"
      mobile-stack
      :label="$t('rateLimit.templates.label')"
    >
      <div class="template-buttons">
        <ElButton
          v-for="tpl in planTemplates"
          :key="tpl.id"
          plain
          @click="applyTemplate(tpl)"
        >
          {{ $t(tpl.nameKey) }}
        </ElButton>
      </div>
    </CommonSettingRow>

    <div
      class="strategy-body-content"
      :class="{ 'is-disabled': !form.enabled || !canWriteBusiness }"
    >
      <CommonSettingRow
        mode="inline"
        mobile-stack
        :label="$t('rateLimit.planQps')"
      >
        <ElInputNumber
          v-model="form.max_concurrent"
          :min="1"
          :step="10"
          :disabled="!form.enabled || !canWriteBusiness"
          class="rate-input-number"
          @change="onTenantQpsChange"
        />
      </CommonSettingRow>
      <CommonSettingRow
        mode="inline"
        mobile-stack
        :label="$t('rateLimit.planBurst')"
      >
        <ElInputNumber
          v-model="form.burst_size"
          :min="1"
          :step="10"
          :disabled="!form.enabled || !canWriteBusiness"
          class="rate-input-number"
          @change="onTenantBurstChange"
        />
      </CommonSettingRow>
      <CommonSettingRow
        mode="inline"
        mobile-stack
        :label="$t('rateLimit.clientIpQps')"
      >
        <ElInputNumber
          v-model="form.client_ip_qps"
          :min="1"
          :max="form.max_concurrent"
          :step="10"
          :disabled="!form.enabled || !canWriteBusiness"
          class="rate-input-number"
          @change="markClientIpEdited"
        />
      </CommonSettingRow>
      <CommonSettingRow
        mode="inline"
        mobile-stack
        :label="$t('rateLimit.clientIpBurst')"
      >
        <ElInputNumber
          v-model="form.client_ip_burst"
          :min="1"
          :max="form.burst_size"
          :step="10"
          :disabled="!form.enabled || !canWriteBusiness"
          class="rate-input-number"
          @change="markClientIpEdited"
        />
      </CommonSettingRow>
      <CommonSettingRow
        mode="inline"
        mobile-stack
        :label="$t('rateLimit.bandwidthMbps')"
      >
        <ElInputNumber
          v-model="form.max_bandwidth_mbps"
          :min="0"
          :step="10"
          :disabled="!form.enabled || !canWriteBusiness"
          class="rate-input-number"
        />
      </CommonSettingRow>
      <div class="collapsible-section">
        <button
          type="button"
          class="collapse-trigger"
          :disabled="!form.enabled || !canWriteBusiness"
          @click="apiOverridesExpanded = !apiOverridesExpanded"
        >
          <ArtSvgIcon
            :icon="
              apiOverridesExpanded
                ? 'ri:arrow-down-s-line'
                : 'ri:arrow-right-s-line'
            "
            class="collapse-icon"
          />
          <span class="section-title-label">{{
            $t("rateLimit.apiOverridesSection")
          }}</span>
          <ElTag
            v-if="apiLimitsList.length > 0"
            size="small"
            type="info"
            class="ml-2"
          >
            {{ apiLimitsList.length }}
          </ElTag>
        </button>

        <div v-show="apiOverridesExpanded" class="collapse-body">
          <div class="section-actions-row">
            <span />
            <ElButton
              v-roles="ROLES_BIZ"
              link
              type="primary"
              :disabled="!form.enabled || !canWriteBusiness"
              @click="addApiLimit"
            >
              <ArtSvgIcon icon="ri:add-circle-line" class="mr-1" />
              {{ $t("rateLimit.addOverride") }}
            </ElButton>
          </div>

          <ArtTable
            :data="apiLimitsList"
            :columns="apiLimitColumns"
            border
            stripe
            empty-height="auto"
            class="api-limits-table"
          >
            <template #path="{ row }">
              <ElInput
                v-model="row.path"
                :disabled="!form.enabled || !canWriteBusiness"
                :placeholder="$t('rateLimit.pathPlaceholder')"
              />
            </template>
            <template #qps="{ row }">
              <ElInputNumber
                v-model="row.qps"
                :min="1"
                :max="form.max_concurrent"
                :disabled="!form.enabled || !canWriteBusiness"
                class="w-full"
              />
            </template>
            <template #burst="{ row }">
              <ElInputNumber
                v-model="row.burst"
                :min="1"
                :max="form.burst_size"
                :disabled="!form.enabled || !canWriteBusiness"
                class="w-full"
              />
            </template>
            <template #operations="{ $index }">
              <TableActionRow>
                <ElButton
                  v-roles="ROLES_BIZ"
                  link
                  type="danger"
                  :disabled="!form.enabled || !canWriteBusiness"
                  @click="removeApiLimit($index)"
                >
                  <ArtSvgIcon icon="ri:delete-bin-line" />
                </ElButton>
              </TableActionRow>
            </template>
            <template #empty>
              <span class="text-secondary">{{
                $t("rateLimit.apiOverridesEmpty")
              }}</span>
            </template>
          </ArtTable>
        </div>
      </div>
    </div>

    <div class="collapsible-section">
      <button
        type="button"
        class="collapse-trigger"
        @click="opsExpanded = !opsExpanded"
      >
        <ArtSvgIcon
          :icon="
            opsExpanded ? 'ri:arrow-down-s-line' : 'ri:arrow-right-s-line'
          "
          class="collapse-icon"
        />
        <span class="section-title-label">{{
          $t("rateLimit.opsTitle")
        }}</span>
        <ElTag
          v-if="activeCommandCount > 0"
          size="small"
          type="warning"
          class="ml-2"
        >
          {{ activeCommandCount }}
        </ElTag>
      </button>

      <div v-show="opsExpanded" class="collapse-body">
        <div class="section-actions-row">
          <span />
          <ElButton
            link
            type="primary"
            :loading="commandsLoading"
            @click="loadCommands"
          >
            <ArtSvgIcon icon="ri:refresh-line" class="mr-1" />
            {{ $t("common.actions.refresh") }}
          </ElButton>
        </div>

        <div
          v-roles="ROLES_BIZ"
          class="ops-actions-panel mb-4"
        >
          <div class="ops-action-row">
            <ElInputNumber
              v-model="denyMinutes"
              :min="1"
              :max="1440"
              :step="5"
              class="ops-input"
            />
            <span class="ops-label">{{
              $t("rateLimit.denyForMinutes")
            }}</span>
            <ElButton
              type="danger"
              plain
              :loading="opsBusy"
              @click="handleIssueDeny"
            >
              {{ $t("rateLimit.issueDeny") }}
            </ElButton>
          </div>
          <div class="ops-action-row">
            <ElInputNumber
              v-model="quotaMultiplier"
              :min="0.1"
              :max="10"
              :step="0.1"
              :precision="1"
              class="ops-input"
            />
            <span class="ops-label">{{
              $t("rateLimit.quotaMultiplier")
            }}</span>
            <ElInputNumber
              v-model="quotaExpiresMinutes"
              :min="1"
              :max="10080"
              :step="10"
              class="ops-input"
            />
            <span class="ops-label">{{
              $t("rateLimit.expiresInMinutes")
            }}</span>
            <ElButton
              type="warning"
              plain
              :loading="opsBusy"
              @click="handleIssueQuota"
            >
              {{ $t("rateLimit.issueQuota") }}
            </ElButton>
          </div>
          <div class="ops-action-row">
            <ElInput
              v-model="commandReason"
              :placeholder="$t('rateLimit.reason')"
              class="ops-reason"
              clearable
            />
            <ElButton
              type="primary"
              plain
              :loading="opsBusy"
              @click="handleIssueRefresh"
            >
              {{ $t("rateLimit.issueRefresh") }}
            </ElButton>
          </div>
        </div>

        <ArtTable
          v-loading="commandsLoading"
          :data="commands"
          :columns="commandColumns"
          border
          stripe
          empty-height="auto"
          class="commands-table"
        >
          <template #command_type="{ row }">
            {{ commandTypeLabel(row.command_type) }}
          </template>
          <template #status="{ row }">
            <ElTag
              :type="row.status === 'active' ? 'success' : 'info'"
              size="small"
            >
              {{
                row.status === "active"
                  ? $t("rateLimit.statusActive")
                  : $t("rateLimit.statusExpired")
              }}
            </ElTag>
          </template>
          <template #expires_at="{ row }">
            {{ formatTime(row.expires_at) }}
          </template>
          <template #created_at="{ row }">
            {{ formatTime(row.created_at) }}
          </template>
          <template #operations="{ row }">
            <TableActionRow>
              <ElButton
                v-if="row.status === 'active'"
                v-roles="ROLES_BIZ"
                link
                type="danger"
                :loading="opsBusy"
                @click="handleCancelCommand(row)"
              >
                {{ $t("rateLimit.cancelCommand") }}
              </ElButton>
            </TableActionRow>
          </template>
          <template #empty>
            <span class="text-secondary">{{
              $t("rateLimit.noCommands")
            }}</span>
          </template>
        </ArtTable>
      </div>
    </div>

    <div class="collapsible-section">
      <button
        type="button"
        class="collapse-trigger"
        @click="reasonGuideExpanded = !reasonGuideExpanded"
      >
        <ArtSvgIcon
          :icon="
            reasonGuideExpanded
              ? 'ri:arrow-down-s-line'
              : 'ri:arrow-right-s-line'
          "
          class="collapse-icon"
        />
        <span class="section-title-label">{{
          $t("rateLimit.reasons.title")
        }}</span>
      </button>
      <div v-show="reasonGuideExpanded" class="collapse-body">
        <div class="reason-guide-list">
          <div
            v-for="item in reasonGuide"
            :key="item.reason"
            class="reason-guide-row"
          >
            <code class="reason-code">{{ item.reason }}</code>
            <div class="reason-meta">
              <span class="reason-talk">{{ $t(item.talkKey) }}</span>
              <router-link
                class="footer-link"
                :to="reasonNavLocation(item.target, props.tenantCode)"
              >
                {{ $t("rateLimit.reasons.goConfig") }}
              </router-link>
            </div>
          </div>
        </div>
      </div>
    </div>

    <CommonInlineHint block size="xs">
      <span>{{ $t("rateLimit.upstreamFooter") }}</span>
      <router-link
        class="footer-link"
        :to="{ path: '/operations/upstream-supply' }"
      >
        {{ $t("rateLimit.upstreamFooterLink") }}
      </router-link>
    </CommonInlineHint>
  </SettingsCard>
</template>

<script setup lang="ts">
import { ElMessage, ElMessageBox } from "element-plus";
import type {
  TenantRateLimitConfig,
  UpdateTenantRateLimitConfigRequest,
  RateLimitControlCommand,
} from "@/types/api/tenant";
import { HttpError } from "@/utils/http/error";
import {
  fetchTenantRateLimitConfig,
  updateTenantRateLimitConfig,
  fetchTenantRateLimitCommands,
  createTenantRateLimitCommand,
  cancelTenantRateLimitCommand,
} from "@/api/tenant";
import { $t } from "@/locales";
import SettingsCard from "@/components/common/settings-card/index.vue";
import CommonSettingRow from "@/components/common/setting-row/index.vue";
import CommonInlineHint from "@/components/common/inline-hint/index.vue";
import { logger } from "@/utils/sys";
import { useUserStore } from "@/store/modules/user";
import { ROLES_BIZ } from "@/utils/ops/roles";
import { summarizeRateLimitConflict } from "./conflict-summary";
import type { ColumnOption } from "@/types/component";
import {
  PLAN_RATE_TEMPLATES,
  RATE_LIMIT_REASON_GUIDE,
  reasonNavLocation,
  type PlanRateTemplate,
} from "@/utils/rate-limit-reason";

defineOptions({ name: "TenantRateLimitCard" });

const props = defineProps<{
  tenantCode: string;
}>();

const userStore = useUserStore();
const canWriteBusiness = computed(() => userStore.canWriteBusiness);

const loading = ref(false);
const saving = ref(false);
const commandsLoading = ref(false);
const opsBusy = ref(false);
const commands = ref<RateLimitControlCommand[]>([]);
const denyMinutes = ref(5);
const quotaMultiplier = ref(0.5);
const quotaExpiresMinutes = ref(60);
const commandReason = ref("");

const apiOverridesExpanded = ref(false);
const opsExpanded = ref(false);
const reasonGuideExpanded = ref(false);

const planTemplates = PLAN_RATE_TEMPLATES;
const reasonGuide = RATE_LIMIT_REASON_GUIDE;

const apiLimitsList = ref<
  {
    path: string;
    qps: number;
    burst: number;
  }[]
>([]);

const activeCommandCount = computed(
  () => commands.value.filter((c) => c.status === "active").length,
);

const apiLimitColumns = ref<
  ColumnOption<{
    path: string;
    qps: number;
    burst: number;
  }>[]
>([
  {
    prop: "path",
    label: $t("rateLimit.columns.path"),
    minWidth: 120,
    useSlot: true,
  },
  {
    prop: "qps",
    label: $t("rateLimit.columns.qps"),
    width: 140,
    useSlot: true,
  },
  {
    prop: "burst",
    label: $t("rateLimit.columns.burst"),
    width: 140,
    useSlot: true,
  },
  {
    prop: "operations",
    label: $t("rateLimit.columns.operations"),
    width: 80,
    align: "center",
    useSlot: true,
  },
]);

const commandColumns = ref<ColumnOption<RateLimitControlCommand>[]>([
  {
    prop: "command_type",
    label: $t("rateLimit.columns.type"),
    minWidth: 96,
    useSlot: true,
  },
  {
    prop: "status",
    label: $t("rateLimit.columns.status"),
    width: 100,
    useSlot: true,
  },
  {
    prop: "expires_at",
    label: $t("rateLimit.columns.expires"),
    minWidth: 112,
    useSlot: true,
  },
  {
    prop: "created_at",
    label: $t("rateLimit.columns.created"),
    minWidth: 112,
    useSlot: true,
  },
  {
    prop: "operations",
    label: $t("rateLimit.columns.operations"),
    width: 90,
    align: "center",
    useSlot: true,
  },
]);

const form = reactive<UpdateTenantRateLimitConfigRequest>({
  max_concurrent: 1000,
  max_bandwidth_mbps: 0,
  api_limits: {},
  burst_size: 100,
  client_ip_qps: 1000,
  client_ip_burst: 100,
  enabled: true,
});

const clientIpManuallyEdited = ref(false);

const applyConfig = (config: TenantRateLimitConfig) => {
  const tenantQps = config.max_concurrent;
  const burst = config.burst_size;
  form.max_concurrent = tenantQps;
  form.max_bandwidth_mbps = config.max_bandwidth_mbps;
  form.api_limits = config.api_limits || {};
  form.burst_size = burst;
  form.client_ip_qps =
    typeof config.client_ip_qps === "number" && config.client_ip_qps > 0
      ? config.client_ip_qps
      : tenantQps;
  form.client_ip_burst =
    typeof config.client_ip_burst === "number" && config.client_ip_burst > 0
      ? config.client_ip_burst
      : burst;
  form.enabled = config.enabled;
  form.expected_updated_at = config.updated_at;
  clientIpManuallyEdited.value =
    form.client_ip_qps !== tenantQps || form.client_ip_burst !== burst;
  apiLimitsList.value = Object.entries(config.api_limits || {}).map(
    ([path, rule]) => ({
      path,
      qps: rule.qps,
      burst: rule.burst,
    }),
  );
};

const markClientIpEdited = () => {
  clientIpManuallyEdited.value = true;
  if (form.client_ip_qps > form.max_concurrent) {
    form.client_ip_qps = form.max_concurrent;
  }
  if (form.client_ip_burst > form.burst_size) {
    form.client_ip_burst = form.burst_size;
  }
};

const onTenantQpsChange = (value: number | undefined) => {
  if (typeof value !== "number" || value < 1) return;
  if (!clientIpManuallyEdited.value || form.client_ip_qps > value) {
    form.client_ip_qps = value;
  }

  for (const item of apiLimitsList.value) {
    if (item.qps > value) {
      item.qps = value;
    }
  }
};

const onTenantBurstChange = (value: number | undefined) => {
  if (typeof value !== "number" || value < 1) return;
  if (!clientIpManuallyEdited.value || form.client_ip_burst > value) {
    form.client_ip_burst = value;
  }
  for (const item of apiLimitsList.value) {
    if (item.burst > value) {
      item.burst = value;
    }
  }
};

const addApiLimit = () => {
  apiLimitsList.value.push({
    path: "",
    qps: Math.min(1000, form.max_concurrent),
    burst: Math.min(1000, form.burst_size),
  });
  if (!apiOverridesExpanded.value) {
    apiOverridesExpanded.value = true;
  }
};

const applyTemplate = (tpl: PlanRateTemplate) => {
  form.enabled = tpl.enabled;
  form.max_concurrent = tpl.max_concurrent;
  form.burst_size = tpl.burst_size;
  form.max_bandwidth_mbps = tpl.max_bandwidth_mbps;
  form.client_ip_qps = tpl.client_ip_qps ?? tpl.max_concurrent;
  form.client_ip_burst = tpl.client_ip_burst ?? tpl.burst_size;
  clientIpManuallyEdited.value = false;
  if (tpl.clearApiLimits) {
    apiLimitsList.value = [];
    form.api_limits = {};
  }
  ElMessage.success(
    $t("rateLimit.templates.applied", { name: $t(tpl.nameKey) }),
  );
};

const removeApiLimit = (index: number) => {
  apiLimitsList.value.splice(index, 1);
};

const loadConfig = async () => {
  loading.value = true;
  try {
    const config = await fetchTenantRateLimitConfig(props.tenantCode);
    applyConfig(config);
  } catch (error) {
    logger.error("Failed to load tenant rate-limit config:", error);
  } finally {
    loading.value = false;
  }
};

const loadCommands = async () => {
  commandsLoading.value = true;
  try {
    const res = await fetchTenantRateLimitCommands(props.tenantCode, 50);
    commands.value = res.commands || [];
  } catch (error) {
    logger.error("Failed to load rate-limit commands:", error);
  } finally {
    commandsLoading.value = false;
  }
};

const getPayload = () => {
  const api_limits: Record<string, { qps: number; burst: number }> = {};
  for (const item of apiLimitsList.value) {
    if (item.path.trim()) {
      api_limits[item.path.trim()] = { qps: item.qps, burst: item.burst };
    }
  }
  return {
    ...form,
    api_limits,
  };
};

const validateApiOverrides = (): boolean => {
  const planQps = form.max_concurrent;
  const planBurst = form.burst_size;
  for (const item of apiLimitsList.value) {
    const path = item.path.trim();
    if (!path) continue;
    if (typeof item.qps !== "number" || item.qps < 1) {
      ElMessage.warning($t("rateLimit.overrideInvalidLimit", { path }));
      return false;
    }
    if (typeof item.burst !== "number" || item.burst < 1) {
      ElMessage.warning($t("rateLimit.overrideInvalidBurst", { path }));
      return false;
    }
    if (item.qps > planQps) {
      ElMessage.warning(
        $t("rateLimit.overrideExceedsPlan", {
          path,
          limit: item.qps,
          planQps,
        }),
      );
      return false;
    }
    if (item.burst > planBurst) {
      ElMessage.warning(
        $t("rateLimit.overrideExceedsBurst", {
          path,
          burst: item.burst,
          planBurst,
        }),
      );
      return false;
    }
  }
  return true;
};

const handleSave = async () => {
  if (!validateApiOverrides()) {
    if (!apiOverridesExpanded.value) {
      apiOverridesExpanded.value = true;
    }
    return;
  }

  saving.value = true;
  let payload: UpdateTenantRateLimitConfigRequest | null = null;
  try {
    payload = getPayload();
    const config = await updateTenantRateLimitConfig(props.tenantCode, payload);
    applyConfig(config);
    ElMessage.success($t("common.feedback.saveSuccess"));
  } catch (error) {
    if (payload && error instanceof HttpError && error.code === 409) {
      const latestConfig = await fetchTenantRateLimitConfig(props.tenantCode);
      applyConfig(latestConfig);
      await ElMessageBox.alert(
        summarizeRateLimitConflict(payload, latestConfig),
        $t("common.conflict.title"),
        { type: "warning" },
      );
    }
    logger.error("Failed to update tenant rate-limit config:", error);
  } finally {
    saving.value = false;
  }
};

import { commandTypeLabel, formatTime } from "./rate-limit-helpers";

const handleIssueDeny = async () => {
  try {
    await ElMessageBox.confirm($t("rateLimit.confirmDeny"), {
      type: "warning",
    });
  } catch {
    return;
  }
  opsBusy.value = true;
  try {
    await createTenantRateLimitCommand(props.tenantCode, {
      command_type: "deny_until",
      deny_for_secs: Math.round(denyMinutes.value * 60),
      reason: commandReason.value || undefined,
    });
    await loadCommands();
  } catch (error) {
    logger.error("Failed to issue deny_until command:", error);
  } finally {
    opsBusy.value = false;
  }
};

const handleIssueQuota = async () => {
  try {
    await ElMessageBox.confirm($t("rateLimit.confirmQuota"), {
      type: "warning",
    });
  } catch {
    return;
  }
  opsBusy.value = true;
  try {
    await createTenantRateLimitCommand(props.tenantCode, {
      command_type: "quota_correction",
      quota_multiplier: quotaMultiplier.value,
      expires_in_secs: Math.round(quotaExpiresMinutes.value * 60),
      reason: commandReason.value || undefined,
    });
    await loadCommands();
  } catch (error) {
    logger.error("Failed to issue quota_correction command:", error);
  } finally {
    opsBusy.value = false;
  }
};

const handleIssueRefresh = async () => {
  try {
    await ElMessageBox.confirm($t("rateLimit.confirmRefresh"), {
      type: "info",
    });
  } catch {
    return;
  }
  opsBusy.value = true;
  try {
    await createTenantRateLimitCommand(props.tenantCode, {
      command_type: "refresh_policy",
      reason: commandReason.value || undefined,
    });
    await loadCommands();
  } catch (error) {
    logger.error("Failed to issue refresh_policy command:", error);
  } finally {
    opsBusy.value = false;
  }
};

const handleCancelCommand = async (row: RateLimitControlCommand) => {
  try {
    await ElMessageBox.confirm($t("rateLimit.confirmCancel"), {
      type: "warning",
    });
  } catch {
    return;
  }
  opsBusy.value = true;
  try {
    await cancelTenantRateLimitCommand(props.tenantCode, row.id);
    await loadCommands();
  } catch (error) {
    logger.error("Failed to cancel rate-limit command:", error);
  } finally {
    opsBusy.value = false;
  }
};

onMounted(() => {
  loadConfig();
  loadCommands();
});
</script>

<style scoped lang="scss">
.template-buttons {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 6px;
}

.strategy-body-content {
  display: flex;
  flex-direction: column;
  gap: 16px;
  transition: all 0.3s ease;

  &.is-disabled {
    opacity: 0.55;
    pointer-events: none;
    filter: grayscale(0.2);
  }
}

.reason-guide-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.reason-guide-row {
  display: flex;
  flex-wrap: wrap;
  gap: 8px 12px;
  align-items: flex-start;
  padding-bottom: 8px;
  border-bottom: 1px dashed var(--el-border-color-lighter);

  &:last-child {
    border-bottom: none;
    padding-bottom: 0;
  }

  .reason-code {
    flex-shrink: 0;
    font-size: 12px;
    padding: 2px 6px;
    border-radius: var(--art-form-control-radius-sm);
    background: var(--el-fill-color);
    color: var(--el-color-primary);
  }

  .reason-meta {
    flex: 1;
    min-width: 180px;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .reason-talk {
    font-size: 12px;
    color: var(--el-text-color-secondary);
  }
}

.collapsible-section {
  border: 1px solid var(--el-border-color-light);
  border-radius: var(--custom-radius);
  overflow: hidden;
}

.collapse-trigger {
  display: flex;
  align-items: center;
  width: 100%;
  padding: 12px 16px;
  margin: 0;
  border: none;
  background: var(--el-fill-color-blank);
  cursor: pointer;
  text-align: left;
  transition: background 0.15s ease;

  &:hover:not(:disabled) {
    background: var(--el-fill-color-light);
  }

  &:disabled {
    cursor: not-allowed;
    opacity: 0.6;
  }

  .collapse-icon {
    font-size: 18px;
    margin-right: 4px;
    color: var(--el-text-color-secondary);
  }
}

.collapse-body {
  padding: 0 16px 16px;
  border-top: 1px solid var(--el-border-color-lighter);
}

.ops-actions-panel {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.ops-action-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}

.ops-input {
  width: 140px;
}

.ops-label {
  font-size: 12px;
  color: var(--el-text-color-secondary);
  margin-right: 4px;
}

.ops-reason {
  flex: 1;
  min-width: 160px;
  max-width: 320px;
}

.section-actions-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin: 8px 0;
  gap: 8px;
}

.section-title-label {
  font-size: 14px;
  font-weight: 500;
  color: var(--el-text-color-primary);
}

.api-limits-table,
.commands-table {
  width: 100%;

  :deep(.el-input__wrapper) {
    box-shadow: none !important;
    background: transparent;
  }
}

.footer-link {
  margin-left: 4px;
  color: var(--el-color-primary);
  font-weight: 500;
  text-decoration: none;

  &:hover {
    text-decoration: underline;
  }
}
</style>
