<template>
  <div
    class="route-config-page"
    :class="{ 'route-config-page--embedded': embedded }"
  >
    <ResourceTableShell
      v-model:columns="columns"
      :title="
        embedded
          ? $t('routes.embeddedTitle')
          : $t('menus.mazbri.advancedRouteConfig')
      "
      :loading="loading"
      :data="tableData"
      :pagination="pagination"
      @refresh="getData"
      @selection-change="handleSelectionChange"
      @pagination:size-change="handleSizeChange"
      @pagination:current-change="handleCurrentChange"
    >
      <template #actions>
        <ListQueryBar
          v-model:q="searchForm.q"
          v-model:filters="listFilters"
          :filter-catalog="routesCatalog"
          @search="handleSearch"
        />
        <ElButton v-roles="['R_ADMIN', 'R_FINANCE']" @click="openRouteImport">
          <template #icon><ArtSvgIcon icon="ri:upload-line" /></template>
          {{ $t("common.actions.import") }}
        </ElButton>
        <TableExportDropdown
          :disabled="tableData.length === 0"
          :items="routeExportItems"
          @command="
            (command) => handleRouteExportCommand(command as ExportCommand)
          "
        />
        <ElButton
          v-roles="['R_ADMIN', 'R_FINANCE']"
          type="primary"
          @click="openCreateRoute"
        >
          <template #icon><ArtSvgIcon icon="ri:add-line" /></template>
          {{
            lockProduct
              ? $t("routes.actions.createProductRoute")
              : $t("routes.actions.createRoute")
          }}
        </ElButton>
        <TableDeleteDropdown
          v-roles="['R_ADMIN', 'R_FINANCE']"
          :total-count="deletableRows.length"
          :selected-count="deletableSelectedRows.length"
          @command="
            (type) =>
              handleRouteDeleteCommand(
                type,
                type === 'all' ? deletableRows : deletableSelectedRows,
              )
          "
        />
      </template>

      <template #tenant_id="{ row }">
        {{ routeTenantName(row) }}
      </template>
      <template #product_id="{ row }">
        {{ routeProductName(row) }}
      </template>
      <template #status="{ row }">
        <StatusIndicator :type="routeStatusType(row.status)">
          {{ routeStatusLabel(row.status) }}
        </StatusIndicator>
      </template>
      <template #metadata="{ row }">
        <div class="metadata-tags">
          <ElTag v-if="isEmergencySwitchEnabled(row.metadata)" type="danger">
            {{ $t("routes.labels.emergencySwitch") }}
          </ElTag>
          <ElTag v-if="isExampleRoute(row)" type="warning">
            {{ $t("routes.labels.example") }}
          </ElTag>
          <ElTag v-if="routePurposeLabelFromMetadata(row.metadata)" type="info">
            {{ routePurposeLabelFromMetadata(row.metadata) }}
          </ElTag>
          <ElTag
            v-if="
              !isEmergencySwitchEnabled(row.metadata) &&
              !isExampleRoute(row) &&
              !routePurposeLabelFromMetadata(row.metadata) &&
              hasMetadata(row.metadata)
            "
            type="info"
          >
            {{ $t("routes.labels.configured") }}
          </ElTag>
          <span v-if="!hasMetadata(row.metadata)">-</span>
        </div>
      </template>
      <template #created_at="{ row }">
        {{ formatDate(row.created_at) }}
      </template>
      <template #operations="{ row }">
        <TableActionRow>
          <ElButton link type="primary" @click="openBindings(row)">{{
            $t("routes.actions.bindings")
          }}</ElButton>
          <ElButton
            v-roles="['R_ADMIN', 'R_FINANCE']"
            link
            type="primary"
            :disabled="isExampleRoute(row)"
            @click="openEditRoute(row)"
          >
            {{ $t("common.actions.modify") }}
          </ElButton>
          <ElButton
            v-roles="['R_ADMIN', 'R_FINANCE']"
            link
            type="danger"
            :disabled="isExampleRoute(row)"
            @click="handleDeleteRoute(row)"
          >
            {{ $t("common.actions.delete") }}
          </ElButton>
        </TableActionRow>
      </template>
    </ResourceTableShell>

    <CenteredDialog
      v-model="bindingDialogVisible"
      :title="$t('routes.dialogs.bindingTitle')"
      width="920px"
    >
      <div class="dialog-toolbar">
        <span class="dialog-toolbar__title">{{ currentRuleTitle }}</span>
        <ElButton
          v-roles="['R_ADMIN', 'R_FINANCE']"
          type="primary"
          :disabled="!currentRule || isExampleRoute(currentRule)"
          @click="openCreateBinding"
        >
          <template #icon><ArtSvgIcon icon="ri:add-line" /></template>
          {{ $t("routes.actions.addUpstreamBinding") }}
        </ElButton>
      </div>

      <ArtTable
        :data="bindingRows"
        :loading="bindingLoading"
        :columns="bindingColumns"
        stripe
        border
      >
        <template #channel_order="{ row }">
          {{ row.channel_order }}
        </template>
        <template #channel_name="{ row }">
          {{ channelName(row.channel_id) }}
        </template>
        <template #switch_condition="{ row }">
          {{ switchConditionSummary(row.switch_condition) }}
        </template>
        <template #operations="{ row }">
          <TableActionRow>
            <ElButton
              v-roles="['R_ADMIN', 'R_FINANCE']"
              link
              type="primary"
              :disabled="isCurrentRuleExample"
              @click="openEditBinding(row)"
            >
              {{ $t("common.actions.modify") }}
            </ElButton>
            <ElButton
              v-roles="['R_ADMIN', 'R_FINANCE']"
              link
              type="danger"
              :disabled="isCurrentRuleExample"
              @click="handleDeleteBinding(row)"
            >
              {{ $t("common.actions.delete") }}
            </ElButton>
            <ElButton
              v-roles="['R_ADMIN', 'R_FINANCE']"
              link
              type="danger"
              :disabled="isCurrentRuleExample"
              @click="openEmergencySwitch(row)"
            >
              <template #icon
                ><ArtSvgIcon icon="ri:flashlight-line"
              /></template>
              {{ $t("routes.actions.emergencySwitch") }}
            </ElButton>
          </TableActionRow>
        </template>
      </ArtTable>
    </CenteredDialog>

    <EditDialog
      v-model="routeDialogVisible"
      :title="
        editingRoute
          ? $t('routes.dialogs.editRouteTitle')
          : $t('routes.dialogs.createRouteTitle')
      "
      width="720px"
      :saving="routeSaving"
      @confirm="submitRoute"
    >
      <ElForm
        ref="routeFormRef"
        :model="routeForm"
        :rules="routeRules"
        label-width="110px"
      >
        <ElFormItem :label="$t('routes.fields.product')" prop="product_id">
          <ElSelect
            v-model="routeForm.product_id"
            filterable
            :placeholder="$t('routes.placeholders.product')"
            :disabled="Boolean(editingRoute) || lockProduct"
          >
            <ElOption
              v-for="product in productRows"
              :key="product.id"
              :label="`${product.name} (${product.product_id})`"
              :value="product.id"
            />
          </ElSelect>
        </ElFormItem>
        <ElFormItem
          v-if="routeForm.scope === 'tenant'"
          :label="$t('routes.fields.tenant')"
          prop="tenant_id"
        >
          <ElSelect
            v-model="routeForm.tenant_id"
            filterable
            :placeholder="$t('routes.placeholders.tenant')"
            :disabled="Boolean(editingRoute)"
          >
            <ElOption
              v-for="tenant in tenantRouteOptions"
              :key="tenant.tenant_id"
              :label="`${tenant.tenant_name} (${tenant.tenant_code})`"
              :value="tenant.tenant_id"
            />
          </ElSelect>
        </ElFormItem>
        <ElFormItem :label="$t('routes.fields.priority')" prop="priority">
          <ElInputNumber v-model="routeForm.priority" :min="0" :step="1" />
        </ElFormItem>
        <ElFormItem :label="$t('routes.fields.status')" prop="status">
          <ElSelect v-model="routeForm.status">
            <ElOption :label="$t('routes.status.active')" value="active" />
            <ElOption :label="$t('routes.status.inactive')" value="inactive" />
            <ElOption
              :label="$t('routes.status.suspended')"
              value="suspended"
            />
          </ElSelect>
        </ElFormItem>
        <ElFormItem
          :label="$t('routes.fields.routePurpose')"
          prop="route_purpose"
        >
          <ElSelect
            v-model="routeForm.route_purpose"
            :placeholder="$t('routes.placeholders.routePurpose')"
          >
            <ElOption
              v-for="item in routePurposeOptions"
              :key="item.value"
              :label="item.label"
              :value="item.value"
            />
          </ElSelect>
        </ElFormItem>
        <ElFormItem
          :label="$t('routes.fields.effectiveWindow')"
          prop="effective_range"
        >
          <div class="form-stack">
            <ElRadioGroup v-model="routeForm.effective_mode">
              <ElRadioButton label="immediate">{{
                $t("routes.effectiveMode.immediate")
              }}</ElRadioButton>
              <ElRadioButton label="window">{{
                $t("routes.effectiveMode.window")
              }}</ElRadioButton>
            </ElRadioGroup>
            <DatePicker
              v-if="routeForm.effective_mode === 'window'"
              v-model="routeForm.effective_range"
              type="datetimerange"
              value-format="YYYY-MM-DDTHH:mm:ssZ"
              :start-placeholder="$t('common.startTime')"
              :end-placeholder="$t('common.endTime')"
            />
          </div>
        </ElFormItem>

        <ElFormItem
          v-if="!editingRoute"
          :label="$t('routes.fields.upstreamChannel')"
          prop="channel_id"
        >
          <ElSelect
            v-model="routeForm.channel_id"
            filterable
            :placeholder="$t('routes.placeholders.upstreamChannel')"
          >
            <ElOption
              v-for="channel in channelRows"
              :key="channel.id"
              :label="`${channel.supplier_name} / ${channel.name} (${channel.channel_code})`"
              :value="channel.id"
            >
              <span
                >{{ channel.supplier_name }} / {{ channel.name }} ({{
                  channel.channel_code
                }})</span
              >
              <ElTag
                v-if="channel.status !== 'active'"
                class="option-tag"
                size="small"
                type="info"
              >
                {{ channelStatusLabel(channel.status) }}
              </ElTag>
            </ElOption>
          </ElSelect>
        </ElFormItem>
        <ElFormItem
          v-if="!editingRoute"
          :label="$t('routes.fields.timeoutMs')"
          prop="timeout_ms"
        >
          <ElInputNumber
            v-model="routeForm.timeout_ms"
            :min="100"
            :step="100"
          />
          <span class="field-unit">{{ $t("routes.units.ms") }}</span>
        </ElFormItem>
        <ElFormItem
          v-if="!editingRoute"
          :label="$t('routes.fields.maxRetries')"
          prop="max_retries"
        >
          <ElInputNumber v-model="routeForm.max_retries" :min="0" :step="1" />
        </ElFormItem>
      </ElForm>
    </EditDialog>

    <ImportDialog
      v-model="routeImportVisible"
      :title="$t('routes.import.title')"
      :columns="routeImportColumns"
      :format-hint="routeImportFormatHint"
      unique-key="key"
      :existing-data="routeImportExistingData"
      :validator="validateRouteImportRow"
      :transformer="transformRouteImportRow"
      :on-import="handleRouteImport"
      :show-success-message="false"
      @success="getData"
    />

    <EditDialog
      v-model="bindingFormVisible"
      :title="
        editingBinding
          ? $t('routes.dialogs.editUpstreamBindingTitle')
          : $t('routes.dialogs.createUpstreamBindingTitle')
      "
      width="680px"
      :saving="bindingSaving"
      @confirm="submitBinding"
    >
      <ElForm
        ref="bindingFormRef"
        :model="bindingForm"
        :rules="bindingRules"
        label-width="120px"
      >
        <ElFormItem
          :label="$t('routes.fields.upstreamChannel')"
          prop="channel_id"
        >
          <ElSelect
            v-model="bindingForm.channel_id"
            filterable
            :placeholder="$t('routes.placeholders.upstreamChannel')"
          >
            <ElOption
              v-for="channel in channelRows"
              :key="channel.id"
              :label="`${channel.supplier_name} / ${channel.name} (${channel.channel_code})`"
              :value="channel.id"
            >
              <span
                >{{ channel.supplier_name }} / {{ channel.name }} ({{
                  channel.channel_code
                }})</span
              >
              <ElTag
                v-if="channel.status !== 'active'"
                class="option-tag"
                size="small"
                type="info"
              >
                {{ channelStatusLabel(channel.status) }}
              </ElTag>
            </ElOption>
          </ElSelect>
        </ElFormItem>
        <ElFormItem
          :label="$t('routes.fields.channelOrder')"
          prop="channel_order"
        >
          <ElInputNumber
            v-model="bindingForm.channel_order"
            :min="1"
            :step="1"
          />
        </ElFormItem>
        <ElFormItem :label="$t('routes.fields.timeoutMs')" prop="timeout_ms">
          <ElInputNumber
            v-model="bindingForm.timeout_ms"
            :min="100"
            :step="100"
          />
          <span class="field-unit">{{ $t("routes.units.ms") }}</span>
        </ElFormItem>
        <ElFormItem :label="$t('routes.fields.maxRetries')" prop="max_retries">
          <ElInputNumber v-model="bindingForm.max_retries" :min="0" :step="1" />
        </ElFormItem>
        <ElFormItem :label="$t('routes.fields.failoverEnabled')">
          <ElSwitch
            v-model="bindingForm.failover_enabled"
            :active-text="$t('routes.failover.activeText')"
            :inactive-text="$t('routes.failover.inactiveText')"
          />
        </ElFormItem>
        <ElFormItem
          v-if="bindingForm.failover_enabled"
          :label="$t('routes.fields.failureTypes')"
        >
          <ElCheckboxGroup v-model="bindingForm.failure_types">
            <ElCheckbox
              v-for="item in failureTypeOptions"
              :key="item.value"
              :label="item.value"
            >
              {{ item.label }}
            </ElCheckbox>
          </ElCheckboxGroup>
        </ElFormItem>
        <ElFormItem :label="$t('routes.fields.description')">
          <ElInput
            v-model="bindingForm.description"
            type="textarea"
            :rows="3"
            :placeholder="$t('routes.placeholders.description')"
          />
        </ElFormItem>
      </ElForm>
    </EditDialog>

    <EmergencySwitchDialog
      v-model="switchDialogVisible"
      v-model:reason="switchForm.reason"
      :saving="switching"
      :channel-name="channelName(switchForm.channel_id)"
      @confirm="submitEmergencySwitch"
    />
  </div>
</template>

<script setup lang="ts">
import type { ListFilter } from "@/types/list-query";
import { routesCatalog } from "@/list-catalog/common-resources";
import { h, ref } from "vue";
import {
  ElMessage,
  ElMessageBox,
  type FormInstance,
  type FormRules,
} from "element-plus";
import { $t } from "@/locales";
import ResourceTableShell from "@/components/common/resource-table-shell/index.vue";
import CenteredDialog from "@/components/common/CenteredDialog.vue";
import EditDialog from "@/components/common/EditDialog.vue";
import ImportDialog from "@/components/common/ImportDialog.vue";
import StatusIndicator from "@/components/common/StatusIndicator.vue";
import ListQueryBar from "@/components/common/list-query-bar/index.vue";
import TableDeleteDropdown from "@/components/table/TableDeleteDropdown.vue";
import TableExportDropdown from "@/components/table/TableExportDropdown.vue";
import { formatDateTime as formatDate } from "@/utils/format";
import type { ColumnOption } from "@/types/component";
import type { ColumnConfig } from "@/composables/useResourceCsvImportExport";
import {
  exportResourceCsv,
  runResourceCsvImport,
} from "@/composables/useResourceCsvImportExport";
import { getUserFacingErrorMessage } from "@/utils/http/error";
import { logger } from "@/utils/sys";
import { fetchAuthorizations, fetchProductList } from "@/api/product";
import type { AuthorizationDetail, Product } from "@/types/api/product";
import {
  createRouteBinding,
  createRouteRule,
  deleteRouteBinding,
  deleteRouteRule,
  emergencySwitchRoute,
  fetchChannels,
  fetchRouteBindings,
  fetchRouteRules,
  updateRouteBinding,
  updateRouteRule,
  type Channel,
  type RouteBinding,
  type RouteRule,
} from "@/api/route-management";
import { useTableSelection } from "@/composables/useTableSelection";
import {
  DEFAULT_TIMEOUT_MS,
  DEFAULT_MAX_RETRIES,
  DEFAULT_FAILURE_TYPES,
  routePurposeOptions,
  failureTypeOptions,
  getRecord,
  getString,
  getNumber,
  getStringArray,
  hasMetadata,
  routePurposeLabel,
  routeStatusLabel,
  routeStatusType,
  channelStatusLabel,
  failureTypeLabel,
  switchConditionSummary,
  isEmergencySwitchEnabled,
  routePurposeLabelFromMetadata,
  isExampleRoute,
  dedupeTenants,
  type TenantRouteOption,
} from "./helpers";
import type {
  RouteScope,
  RouteFormState,
  BindingFormState,
  RouteImportRow,
} from "./types";
defineOptions({ name: "RouteConfig" });
const props = withDefaults(
  defineProps<{
    embedded?: boolean;
    initialProductId?: string;
    lockProduct?: boolean;
  }>(),
  {
    embedded: false,
    initialProductId: "",
    lockProduct: false,
  },
);
const embedded = computed(() => props.embedded);
const lockProduct = computed(
  () => props.lockProduct && Boolean(props.initialProductId),
);
const loading = ref(false);
const bindingLoading = ref(false);
const routeSaving = ref(false);
const bindingSaving = ref(false);
const switching = ref(false);
const bindingDialogVisible = ref(false);
const routeDialogVisible = ref(false);
const routeImportVisible = ref(false);
const bindingFormVisible = ref(false);
const switchDialogVisible = ref(false);
const tableData = ref<RouteRule[]>([]);
const bindingRows = ref<RouteBinding[]>([]);
const channelRows = ref<Channel[]>([]);
const productRows = ref<Product[]>([]);
const authorizations = ref<AuthorizationDetail[]>([]);
const currentRule = ref<RouteRule | null>(null);
const editingRoute = ref<RouteRule | null>(null);
const editingBinding = ref<RouteBinding | null>(null);
const routeFormRef = ref<FormInstance>();
const bindingFormRef = ref<FormInstance>();
const pagination = reactive({
  current: 1,
  size: 20,
  total: 0,
});
const searchForm = reactive({
  q: "",
});
const listFilters = ref<ListFilter[]>([]);
const routeImportColumns: ColumnConfig[] = [
  {
    key: "product_code",
    label: $t("routes.import.columns.productCode"),
    required: true,
  },
  {
    key: "tenant_code",
    label: $t("routes.import.columns.tenantCode"),
    required: true,
  },
  {
    key: "channel_code",
    label: $t("routes.import.columns.channelCode"),
    required: true,
  },
  {
    key: "priority",
    label: $t("routes.import.columns.priority"),
    required: false,
    type: "number",
  },
  { key: "status", label: $t("routes.import.columns.status"), required: false },
  {
    key: "route_purpose",
    label: $t("routes.import.columns.routePurpose"),
    required: false,
  },
];
const routeImportFormatHint = [
  $t("routes.import.formatHintHeader"),
  $t("routes.import.formatHintExample"),
].join("\n");
const routeForm = reactive<RouteFormState>({
  scope: "tenant",
  tenant_id: "",
  product_id: props.initialProductId,
  priority: 100,
  status: "active",
  route_purpose: "standard",
  effective_mode: "immediate",
  effective_range: [],
  channel_id: "",
  timeout_ms: DEFAULT_TIMEOUT_MS,
  max_retries: DEFAULT_MAX_RETRIES,
});
const bindingForm = reactive<BindingFormState>({
  channel_id: "",
  channel_order: 1,
  timeout_ms: DEFAULT_TIMEOUT_MS,
  max_retries: DEFAULT_MAX_RETRIES,
  failover_enabled: true,
  failure_types: [...DEFAULT_FAILURE_TYPES],
  description: "",
});
const switchForm = reactive({
  channel_id: "",
  reason: "",
});
const routeRules: FormRules<RouteFormState> = {
  product_id: [
    {
      required: true,
      message: $t("routes.validation.productRequired"),
      trigger: "change",
    },
  ],
  tenant_id: [
    {
      validator: (_rule, value, callback) => {
        if (routeForm.scope === "tenant" && !value) {
          callback(new Error($t("routes.validation.tenantRequired")));
          return;
        }
        callback();
      },
      trigger: "change",
    },
  ],
  priority: [
    {
      required: true,
      message: $t("routes.validation.priorityRequired"),
      trigger: "change",
    },
  ],
  status: [
    {
      required: true,
      message: $t("routes.validation.statusRequired"),
      trigger: "change",
    },
  ],
  channel_id: [
    {
      required: true,
      message: $t("routes.validation.bindingChannelRequired"),
      trigger: "change",
    },
  ],
  route_purpose: [
    {
      required: true,
      message: $t("routes.validation.routePurposeRequired"),
      trigger: "change",
    },
  ],
  effective_range: [
    {
      validator: (_rule, value, callback) => {
        if (
          routeForm.effective_mode === "window" &&
          (!Array.isArray(value) || value.length !== 2)
        ) {
          callback(new Error($t("routes.validation.effectiveWindowRequired")));
          return;
        }
        callback();
      },
      trigger: "change",
    },
  ],
};
const bindingRules: FormRules<BindingFormState> = {
  channel_id: [
    {
      required: true,
      message: $t("routes.validation.bindingChannelRequired"),
      trigger: "change",
    },
  ],
  channel_order: [
    {
      required: true,
      message: $t("routes.validation.channelOrderRequired"),
      trigger: "change",
    },
  ],
  timeout_ms: [
    {
      required: true,
      message: $t("routes.validation.timeoutRequired"),
      trigger: "change",
    },
  ],
  max_retries: [
    {
      required: true,
      message: $t("routes.validation.maxRetriesRequired"),
      trigger: "change",
    },
  ],
};
const columns = ref<ColumnOption<RouteRule>[]>([
  {
    type: "selection",
    width: 55,
    selectable: (row: RouteRule) => !isExampleRoute(row),
  },
  {
    prop: "tenant_id",
    label: $t("routes.columns.tenant"),
    minWidth: 120,
    useSlot: true,
    showOverflowTooltip: true,
  },
  {
    prop: "product_id",
    label: $t("routes.columns.product"),
    minWidth: 140,
    useSlot: true,
    showOverflowTooltip: true,
  },
  { prop: "priority", label: $t("routes.columns.priority"), width: 90 },
  {
    prop: "status",
    label: $t("routes.columns.status"),
    width: 100,
    useSlot: true,
  },
  {
    prop: "metadata",
    label: $t("routes.columns.metadata"),
    width: 190,
    useSlot: true,
  },
  {
    prop: "created_at",
    label: $t("common.fields.createdAt"),
    width: 180,
    useSlot: true,
  },
  {
    prop: "operations",
    label: $t("common.fields.operations"),
    width: 220,
    fixed: "right",
    useSlot: true,
  },
]);
const bindingColumns = ref<ColumnOption<RouteBinding>[]>([
  {
    prop: "channel_order",
    label: $t("routes.bindingColumns.order"),
    width: 80,
    useSlot: true,
  },
  {
    prop: "channel_name",
    label: $t("routes.bindingColumns.channel"),
    minWidth: 140,
    useSlot: true,
  },
  {
    prop: "channel_id",
    label: $t("routes.bindingColumns.channelId"),
    minWidth: 140,
    showOverflowTooltip: true,
  },
  {
    prop: "switch_condition",
    label: $t("routes.bindingColumns.switchCondition"),
    minWidth: 140,
    useSlot: true,
    showOverflowTooltip: true,
  },
  {
    prop: "operations",
    label: $t("common.fields.operations"),
    width: 230,
    fixed: "right",
    useSlot: true,
  },
]);
const productMap = computed(() => {
  return new Map(productRows.value.map((product) => [product.id, product]));
});
const channelMap = computed(() => {
  return new Map(channelRows.value.map((channel) => [channel.id, channel]));
});
const tenantMap = computed(() => {
  const map = new Map<string, TenantRouteOption>();
  for (const item of authorizations.value) {
    if (!map.has(item.tenant_id)) {
      map.set(item.tenant_id, {
        tenant_id: item.tenant_id,
        tenant_code: item.tenant_code,
        tenant_name: item.tenant_name,
        product_id: item.product_id,
        product_code: item.product_code,
      });
    }
  }
  return map;
});
const tenantRouteOptions = computed(() => {
  const options = authorizations.value.map((item) => ({
    tenant_id: item.tenant_id,
    tenant_code: item.tenant_code,
    tenant_name: item.tenant_name,
    product_id: item.product_id,
    product_code: item.product_code,
  }));
  if (!routeForm.product_id) {
    return dedupeTenants(options);
  }
  const matched = options.filter(
    (item) => item.product_id === routeForm.product_id,
  );
  return dedupeTenants(matched.length ? matched : options);
});
const isCurrentRuleExample = computed(() =>
  Boolean(currentRule.value && isExampleRoute(currentRule.value)),
);
const deletableRows = computed(() =>
  tableData.value.filter((row) => !isExampleRoute(row)),
);
const deletableSelectedRows = computed(() =>
  selectedRows.value.filter((row) => !isExampleRoute(row)),
);
const routeExportItems = computed(() => [
  {
    command: "page",
    label: $t("routes.actions.exportPage"),
    disabled: tableData.value.length === 0,
  },
  {
    command: "selected",
    label: $t("routes.actions.exportSelected", {
      count: selectedRows.value.length,
    }),
    disabled: selectedRows.value.length === 0,
  },
]);
const routeImportExistingData = computed<Record<string, unknown>[]>(() =>
  tableData.value.map((row) => {
    const routeConfig = getRecord(row.metadata?.route_config);
    return {
      key: `${getString(routeConfig.product_code, row.product_id)}/${getString(routeConfig.tenant_code, row.tenant_id || "")}`,
    };
  }),
);
const currentRuleTitle = computed(() => {
  if (!currentRule.value) return "-";
  return `${routeTenantName(currentRule.value)} / ${routeProductName(currentRule.value)}`;
});
const channelName = (channelId: string) => {
  const channel = channelMap.value.get(channelId);
  return channel
    ? `${channel.supplier_name} / ${channel.name} (${channel.channel_code})`
    : channelId || "-";
};
const productName = (productId: string) => {
  const product = productMap.value.get(productId);
  return product ? `${product.name} (${product.product_id})` : productId || "-";
};
const tenantName = (tenantId?: string | null) => {
  if (!tenantId) return $t("routes.labels.defaultRoute");
  const tenant = tenantMap.value.get(tenantId);
  return tenant ? `${tenant.tenant_name} (${tenant.tenant_code})` : tenantId;
};
const routeProductName = (row: RouteRule) => {
  const product = productMap.value.get(row.product_id);
  if (product) return `${product.name} (${product.product_id})`;
  const routeConfig = getRecord(row.metadata?.route_config);
  const productNameValue = getString(routeConfig.product_name);
  const productCodeValue = getString(routeConfig.product_code, row.product_id);
  return productNameValue
    ? `${productNameValue} (${productCodeValue})`
    : row.product_id || "-";
};
const routeTenantName = (row: RouteRule) => {
  if (!row.tenant_id) return $t("routes.labels.defaultRoute");
  const tenant = tenantMap.value.get(row.tenant_id);
  if (tenant) return `${tenant.tenant_name} (${tenant.tenant_code})`;
  const routeConfig = getRecord(row.metadata?.route_config);
  const tenantNameValue = getString(routeConfig.tenant_name);
  const tenantCodeValue = getString(routeConfig.tenant_code, row.tenant_id);
  return tenantNameValue
    ? `${tenantNameValue} (${tenantCodeValue})`
    : row.tenant_id;
};
const {
  selectedRows,
  handleSelectionChange,
  handleDeleteCommand: handleRouteDeleteCommand,
} = useTableSelection<RouteRule>({
  deleteApi: async (row) => {
    await deleteRouteRule(row.id);
  },
  getItemName: (row) => `${routeTenantName(row)} / ${routeProductName(row)}`,
  onDeleteSuccess: () => getData(),
});
const openRouteImport = async () => {
  await getReferenceData();
  routeImportVisible.value = true;
};
const getChannels = async () => {
  const res = await fetchChannels({ page: 1, page_size: 100 });
  channelRows.value = res.channels;
};
const getReferenceData = async () => {
  const [productsResult, authResult] = await Promise.allSettled([
    fetchProductList({ page: 1, page_size: 100 }),
    fetchAuthorizations({ page: 1, page_size: 100 }),
  ]);
  if (productsResult.status === "fulfilled") {
    productRows.value = productsResult.value.products;
  }
  if (authResult.status === "fulfilled") {
    authorizations.value = authResult.value.authorizations;
  }
};
const getData = async () => {
  loading.value = true;
  try {
    const [routes] = await Promise.all([
      fetchRouteRules({
        filters: listFilters.value.length
          ? JSON.stringify(listFilters.value)
          : undefined,
        page: pagination.current,
        page_size: pagination.size,
        q: searchForm.q.trim() || undefined,
        product_id: props.initialProductId || undefined,
      }),
      getChannels(),
      getReferenceData(),
    ]);
    tableData.value = routes.route_rules;
    pagination.total = routes.pagination?.total ?? 0;
  } catch (error: unknown) {
    ElMessage.error(
      getUserFacingErrorMessage(error, $t("routes.messages.fetchRoutesFailed")),
    );
  } finally {
    loading.value = false;
  }
};
const refreshBindings = async () => {
  if (!currentRule.value) return;
  bindingLoading.value = true;
  try {
    const res = await fetchRouteBindings(currentRule.value.id);
    bindingRows.value = res.bindings;
  } catch (error: unknown) {
    ElMessage.error(
      getUserFacingErrorMessage(
        error,
        $t("routes.messages.fetchBindingsFailed"),
      ),
    );
  } finally {
    bindingLoading.value = false;
  }
};
const openBindings = async (row: RouteRule) => {
  currentRule.value = row;
  bindingDialogVisible.value = true;
  await refreshBindings();
};
const resetRouteForm = () => {
  Object.assign(routeForm, {
    scope: "tenant",
    tenant_id: "",
    product_id: props.initialProductId || "",
    priority: 100,
    status: "active",
    route_purpose: "standard",
    effective_mode: "immediate",
    effective_range: [],
    channel_id: "",
    timeout_ms: DEFAULT_TIMEOUT_MS,
    max_retries: DEFAULT_MAX_RETRIES,
  });
  routeFormRef.value?.clearValidate();
};
const openCreateRoute = async () => {
  editingRoute.value = null;
  resetRouteForm();
  await getReferenceData();
  routeDialogVisible.value = true;
};
const openEditRoute = async (row: RouteRule) => {
  if (isExampleRoute(row)) {
    ElMessage.info($t("routes.messages.exampleNotEditable"));
    return;
  }
  editingRoute.value = row;
  await getReferenceData();
  const routeConfig = getRecord(row.metadata?.route_config);
  const effectiveMode =
    getString(routeConfig.effective_mode) === "window" ? "window" : "immediate";
  const effectiveStartAt = getString(routeConfig.effective_start_at);
  const effectiveEndAt = getString(routeConfig.effective_end_at);
  Object.assign(routeForm, {
    scope: row.tenant_id ? "tenant" : "default",
    tenant_id: row.tenant_id || "",
    product_id: row.product_id,
    priority: row.priority,
    status: row.status,
    route_purpose: getString(routeConfig.purpose, "standard"),
    effective_mode: effectiveMode,
    effective_range:
      effectiveMode === "window" && effectiveStartAt && effectiveEndAt
        ? [effectiveStartAt, effectiveEndAt]
        : [],
  });
  routeDialogVisible.value = true;
};
const buildRouteMetadata = () => {
  const base = editingRoute.value?.metadata
    ? { ...editingRoute.value.metadata }
    : {};
  const [effectiveStartAt, effectiveEndAt] =
    routeForm.effective_mode === "window"
      ? routeForm.effective_range
      : ["", ""];
  const product = productMap.value.get(routeForm.product_id);
  const tenant = routeForm.tenant_id
    ? tenantMap.value.get(routeForm.tenant_id)
    : undefined;
  return {
    ...base,
    route_config: {
      purpose: routeForm.route_purpose,
      purpose_label: routePurposeLabel(routeForm.route_purpose),
      effective_mode: routeForm.effective_mode,
      effective_start_at: effectiveStartAt || null,
      effective_end_at: effectiveEndAt || null,
      product_name: product?.name || null,
      product_code: product?.product_id || routeForm.product_id,
      tenant_name: tenant?.tenant_name || null,
      tenant_code: tenant?.tenant_code || null,
      updated_at: new Date().toISOString(),
    },
  };
};
const buildRouteConfirmText = () => {
  const lines = [
    `${$t("routes.confirm.scope")}${routeForm.scope === "tenant" ? $t("routes.scope.tenant") : $t("routes.scope.default")}`,
    `${$t("routes.confirm.product")}${productName(routeForm.product_id)}`,
    routeForm.scope === "tenant"
      ? `${$t("routes.confirm.tenant")}${tenantName(routeForm.tenant_id)}`
      : "",
    `${$t("routes.confirm.priority")}${routeForm.priority}`,
    `${$t("routes.confirm.status")}${routeStatusLabel(routeForm.status)}`,
    `${$t("routes.confirm.routePurpose")}${routePurposeLabel(routeForm.route_purpose)}`,
    routeForm.effective_mode === "window"
      ? `${$t("routes.confirm.effectiveWindow")}${routeForm.effective_range[0]} ${$t("common.to")} ${routeForm.effective_range[1]}`
      : $t("routes.confirm.effectiveNow"),
  ].filter(Boolean);
  return lines.join("\n");
};
const confirmSave = async (message: string, title: string) => {
  try {
    await ElMessageBox.confirm(
      h(
        "pre",
        {
          style: {
            margin: "0",
            whiteSpace: "pre-wrap",
            fontFamily: "inherit",
            lineHeight: "1.6",
          },
        },
        message,
      ),
      title,
      {
        type: "warning",
        confirmButtonText: $t("routes.actions.confirmSave"),
        cancelButtonText: $t("routes.actions.backToEdit"),
      },
    );
    return true;
  } catch {
    return false;
  }
};
const submitRoute = async () => {
  const valid = await routeFormRef.value?.validate().catch(() => false);
  if (!valid) return;
  const shouldSave = await confirmSave(
    buildRouteConfirmText(),
    $t("routes.dialogs.confirmSaveRoute"),
  );
  if (!shouldSave) return;
  const metadata = buildRouteMetadata();
  routeSaving.value = true;
  try {
    if (editingRoute.value) {
      await updateRouteRule(editingRoute.value.id, {
        priority: routeForm.priority,
        status: routeForm.status,
        metadata,
      });
    } else {
      await createRouteRule({
        tenant_id: routeForm.scope === "tenant" ? routeForm.tenant_id : null,
        product_id: routeForm.product_id,
        priority: routeForm.priority,
        status: routeForm.status,
        metadata,
        channel_id: routeForm.channel_id,
        timeout_ms: routeForm.timeout_ms,
        max_retries: routeForm.max_retries,
      });
    }
    ElMessage.success($t("common.feedback.saveSuccess"));
    routeDialogVisible.value = false;
    await getData();
  } catch (error: unknown) {
    ElMessage.error(
      getUserFacingErrorMessage(error, $t("routes.messages.saveRouteFailed")),
    );
  } finally {
    routeSaving.value = false;
  }
};
const handleDeleteRoute = async (row: RouteRule) => {
  if (isExampleRoute(row)) {
    ElMessage.info($t("routes.messages.exampleNotDeletable"));
    return;
  }
  await ElMessageBox.confirm(
    $t("routes.messages.deleteRouteConfirm", {
      tenant: routeTenantName(row),
      product: routeProductName(row),
    }),
    $t("common.warning"),
    { type: "warning" },
  );
  try {
    await deleteRouteRule(row.id);
    ElMessage.success($t("common.feedback.deleteSuccess"));
    await getData();
  } catch (error: unknown) {
    ElMessage.error(
      getUserFacingErrorMessage(error, $t("routes.messages.deleteRouteFailed")),
    );
  }
};
const resetBindingForm = () => {
  Object.assign(bindingForm, {
    channel_id: "",
    channel_order: nextBindingOrder(),
    timeout_ms: DEFAULT_TIMEOUT_MS,
    max_retries: DEFAULT_MAX_RETRIES,
    failover_enabled: true,
    failure_types: [...DEFAULT_FAILURE_TYPES],
    description: "",
  });
  bindingFormRef.value?.clearValidate();
};
const nextBindingOrder = () => {
  const maxOrder = bindingRows.value.reduce(
    (max, row) => Math.max(max, row.channel_order),
    0,
  );
  return maxOrder + 1;
};
const openCreateBinding = async () => {
  if (!currentRule.value) return;
  if (isExampleRoute(currentRule.value)) {
    ElMessage.info($t("routes.messages.exampleNotAddBinding"));
    return;
  }
  editingBinding.value = null;
  resetBindingForm();
  await getChannels();
  bindingFormVisible.value = true;
};
const openEditBinding = async (row: RouteBinding) => {
  if (isCurrentRuleExample.value) {
    ElMessage.info($t("routes.messages.exampleNotEditable"));
    return;
  }
  editingBinding.value = row;
  await getChannels();
  const switchCondition = row.switch_condition || {};
  Object.assign(bindingForm, {
    channel_id: row.channel_id,
    channel_order: row.channel_order,
    timeout_ms: getNumber(switchCondition.timeout_ms, DEFAULT_TIMEOUT_MS),
    max_retries: getNumber(switchCondition.max_retries, DEFAULT_MAX_RETRIES),
    failover_enabled: switchCondition.failover_enabled !== false,
    failure_types: getStringArray(switchCondition.failure_types).length
      ? getStringArray(switchCondition.failure_types)
      : [...DEFAULT_FAILURE_TYPES],
    description: getString(switchCondition.description),
  });
  bindingFormVisible.value = true;
};
const buildSwitchCondition = () => {
  const base = editingBinding.value?.switch_condition
    ? { ...editingBinding.value.switch_condition }
    : {};
  return {
    ...base,
    max_retries: bindingForm.max_retries,
    timeout_ms: bindingForm.timeout_ms,
    failover_enabled: bindingForm.failover_enabled,
    failure_types: bindingForm.failover_enabled
      ? bindingForm.failure_types
      : [],
    description: bindingForm.description.trim() || null,
  };
};
const buildBindingConfirmText = () => {
  const lines = [
    `${$t("routes.confirm.route")}${currentRuleTitle.value}`,
    `${$t("routes.confirm.upstreamChannel")}${channelName(bindingForm.channel_id)}`,
    `${$t("routes.confirm.channelOrder")}${bindingForm.channel_order}`,
    `${$t("routes.confirm.timeout")}${bindingForm.timeout_ms} ${$t("routes.units.ms")}`,
    `${$t("routes.confirm.maxRetries")}${bindingForm.max_retries}`,
    `${$t("routes.confirm.failover")}${bindingForm.failover_enabled ? $t("routes.failover.activeText") : $t("routes.failover.inactiveText")}`,
    bindingForm.failover_enabled
      ? `${$t("routes.confirm.failureTypes")}${bindingForm.failure_types.map(failureTypeLabel).join("、") || $t("routes.labels.unselected")}`
      : "",
    bindingForm.description.trim()
      ? `${$t("routes.confirm.description")}${bindingForm.description.trim()}`
      : "",
  ].filter(Boolean);
  return lines.join("\n");
};
const submitBinding = async () => {
  if (!currentRule.value) return;
  if (isCurrentRuleExample.value) {
    ElMessage.info($t("routes.messages.exampleNotSaveBinding"));
    return;
  }
  const valid = await bindingFormRef.value?.validate().catch(() => false);
  if (!valid) return;
  const shouldSave = await confirmSave(
    buildBindingConfirmText(),
    $t("routes.dialogs.confirmSaveBinding"),
  );
  if (!shouldSave) return;
  const switchCondition = buildSwitchCondition();
  bindingSaving.value = true;
  try {
    if (editingBinding.value) {
      await updateRouteBinding(editingBinding.value.id, {
        channel_id: bindingForm.channel_id,
        channel_order: bindingForm.channel_order,
        switch_condition: switchCondition,
      });
    } else {
      await createRouteBinding(currentRule.value.id, {
        channel_id: bindingForm.channel_id,
        channel_order: bindingForm.channel_order,
        switch_condition: switchCondition,
      });
    }
    ElMessage.success($t("common.feedback.saveSuccess"));
    bindingFormVisible.value = false;
    await refreshBindings();
  } catch (error: unknown) {
    ElMessage.error(
      getUserFacingErrorMessage(error, $t("routes.messages.saveBindingFailed")),
    );
  } finally {
    bindingSaving.value = false;
  }
};
const handleDeleteBinding = async (row: RouteBinding) => {
  if (isCurrentRuleExample.value) {
    ElMessage.info($t("routes.messages.exampleNotDeletable"));
    return;
  }
  await ElMessageBox.confirm(
    $t("routes.messages.deleteBindingConfirm", {
      name: channelName(row.channel_id),
    }),
    $t("common.warning"),
    {
      type: "warning",
    },
  );
  try {
    await deleteRouteBinding(row.id);
    ElMessage.success($t("common.feedback.deleteSuccess"));
    await refreshBindings();
  } catch (error: unknown) {
    ElMessage.error(
      getUserFacingErrorMessage(
        error,
        $t("routes.messages.deleteBindingFailed"),
      ),
    );
  }
};
const openEmergencySwitch = (row: RouteBinding) => {
  if (isCurrentRuleExample.value) {
    ElMessage.info($t("routes.messages.exampleNotSwitch"));
    return;
  }
  switchForm.channel_id = row.channel_id;
  switchForm.reason = "";
  switchDialogVisible.value = true;
};
const submitEmergencySwitch = async () => {
  if (!currentRule.value) return;
  if (isCurrentRuleExample.value) {
    ElMessage.info($t("routes.messages.exampleNotSwitch"));
    return;
  }
  if (!switchForm.reason.trim()) {
    ElMessage.warning($t("routes.validation.switchReasonRequired"));
    return;
  }
  switching.value = true;
  try {
    await emergencySwitchRoute(currentRule.value.id, {
      channel_id: switchForm.channel_id,
      reason: switchForm.reason.trim(),
    });
    ElMessage.success($t("routes.messages.emergencySwitchSuccess"));
    switchDialogVisible.value = false;
    await refreshBindings();
    await getData();
  } catch (error: unknown) {
    ElMessage.error(
      getUserFacingErrorMessage(
        error,
        $t("routes.messages.emergencySwitchFailed"),
      ),
    );
  } finally {
    switching.value = false;
  }
};
const handleSizeChange = (size: number) => {
  pagination.size = size;
  pagination.current = 1;
  getData();
};
const handleCurrentChange = (page: number) => {
  pagination.current = page;
  getData();
};
const handleSearch = () => {
  pagination.current = 1;
  getData();
};
type ExportCommand = "page" | "selected";
type CsvRow = Record<string, string | number | null | undefined>;
const handleRouteExportCommand = (command: ExportCommand) => {
  const rows = command === "page" ? tableData.value : selectedRows.value;
  const csvRows: CsvRow[] = rows.map((row) => {
    const routeConfig = getRecord(row.metadata?.route_config);
    return {
      tenant: routeTenantName(row),
      product: routeProductName(row),
      priority: row.priority,
      status: routeStatusLabel(row.status),
      route_purpose:
        routePurposeLabelFromMetadata(row.metadata) ||
        routePurposeLabel(getString(routeConfig.purpose)),
      created_at: row.created_at,
    };
  });
  exportResourceCsv(
    csvRows,
    [
      { key: "tenant", label: $t("routes.columns.tenant") },
      { key: "product", label: $t("routes.columns.product") },
      { key: "priority", label: $t("routes.columns.priority") },
      { key: "status", label: $t("routes.columns.status") },
      { key: "route_purpose", label: $t("routes.fields.routePurpose") },
      { key: "created_at", label: $t("common.fields.createdAt") },
    ],
    `user-routes-${command}-${Date.now().toString(36)}.csv`,
  );
};
const validateRouteImportRow = (row: Record<string, string>) => {
  if (!row.product_code?.trim()) {
    return {
      valid: false,
      reason: $t("routes.import.validation.productCodeRequired"),
    };
  }
  if (!row.tenant_code?.trim()) {
    return {
      valid: false,
      reason: $t("routes.import.validation.tenantCodeRequired"),
    };
  }
  if (!row.channel_code?.trim()) {
    return {
      valid: false,
      reason: $t("routes.import.validation.channelCodeRequired"),
    };
  }
  if (row.priority?.trim() && Number.isNaN(Number(row.priority))) {
    return {
      valid: false,
      reason: $t("routes.import.validation.priorityNumeric"),
    };
  }
  if (
    row.status?.trim() &&
    !["active", "inactive", "suspended"].includes(row.status.trim())
  ) {
    return {
      valid: false,
      reason: $t("routes.import.validation.statusInvalid"),
    };
  }
  if (
    row.route_purpose?.trim() &&
    !routePurposeOptions.some((item) => item.value === row.route_purpose.trim())
  ) {
    return {
      valid: false,
      reason: $t("routes.import.validation.routePurposeInvalid"),
    };
  }
  return { valid: true };
};
const transformRouteImportRow = (
  row: Record<string, string>,
): RouteImportRow => {
  const productCode = row.product_code.trim();
  const tenantCode = row.tenant_code.trim();
  const channelCode = row.channel_code.trim();
  return {
    key: `${productCode}/${tenantCode}`,
    product_code: productCode,
    tenant_code: tenantCode,
    channel_code: channelCode,
    priority: Number(row.priority?.trim() || 100),
    status: row.status?.trim() || "active",
    route_purpose: row.route_purpose?.trim() || "standard",
  };
};
const handleRouteImport = async (items: Record<string, unknown>[]) => {
  const productByCode = new Map(
    productRows.value.map((product) => [product.product_id, product]),
  );
  const authorizationByKey = new Map(
    authorizations.value.map((item) => [
      `${item.product_code}/${item.tenant_code}`,
      item,
    ]),
  );
  const channelByCode = new Map(
    channelRows.value.map((channel) => [channel.channel_code, channel]),
  );
  await runResourceCsvImport<RouteImportRow>(
    items,
    async (item) => {
      const productCode = item.product_code;
      const tenantCode = item.tenant_code;
      const channelCode = item.channel_code;
      const product = productByCode.get(productCode);
      const authorization = authorizationByKey.get(
        `${productCode}/${tenantCode}`,
      );
      const channel = channelByCode.get(channelCode);
      if (!product || !authorization) {
        throw new Error(
          `Unknown authorized tenant route: ${productCode} / ${tenantCode}`,
        );
      }
      if (!channel) {
        throw new Error(`Unknown channel code: ${channelCode}`);
      }
      const routePurpose = item.route_purpose || "standard";
      await createRouteRule({
        tenant_id: authorization.tenant_id,
        product_id: product.id,
        priority: item.priority || 100,
        status: item.status || "active",
        metadata: {
          route_config: {
            purpose: routePurpose,
            purpose_label: routePurposeLabel(routePurpose),
            effective_mode: "immediate",
            effective_start_at: null,
            effective_end_at: null,
            product_name: product.name,
            product_code: product.product_id,
            tenant_name: authorization.tenant_name,
            tenant_code: authorization.tenant_code,
            updated_at: new Date().toISOString(),
          },
        },
        channel_id: channel.id,
      });
    },
    {
      onError: (error) => {
        logger.error("Failed to import route:", error);
      },
    },
  );
};
onMounted(() => {
  getData();
});
watch(
  () => props.initialProductId,
  (nextProductId) => {
    if (props.lockProduct) {
      routeForm.product_id = nextProductId || "";
    }
    pagination.current = 1;
    getData();
  },
);
</script>

<style scoped lang="scss">
.route-config-page {
  padding: 0;
}

.route-config-page--embedded {
  :deep(.art-table-card) {
    margin-bottom: 0;
  }
}

.metadata-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.dialog-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 12px;
}

.dialog-toolbar__title {
  min-width: 0;
  overflow: hidden;
  font-size: 14px;
  color: var(--el-text-color-secondary);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.mb-4 {
  margin-bottom: 16px;
}

.option-tag {
  margin-left: 8px;
}

.form-stack {
  display: flex;
  flex-direction: column;
  gap: 10px;
  width: 100%;
}

.field-unit {
  margin-left: 8px;
  color: var(--el-text-color-secondary);
}
</style>
