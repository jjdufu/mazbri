<template>
  <ElFormItem
    :label="$t('configCenter.serviceNameSelector.label')"
    prop="service_name"
    required
  >
    <ElSelect
      v-model="selectedService"
      :placeholder="$t('configCenter.serviceNameSelector.placeholder')"
      filterable
      clearable
      :loading="loading"
      :disabled="disabled"
      style="width: 100%"
      @change="handleServiceChange"
    >
      <ElOptionGroup
        v-if="systemServices.length > 0"
        :label="$t('configCenter.serviceNameSelector.groups.system')"
      >
        <ElOption
          v-for="item in systemServices"
          :key="item.service_name"
          :label="item.service_name"
          :value="item.service_name"
          :disabled="isExisting(item.service_name)"
        >
          <div class="service-option">
            <span
              :class="{
                'service-option-name': true,
                'is-existing': isExisting(item.service_name),
              }"
            >
              {{ item.service_name }}
            </span>
            <ElTag
              v-if="isExisting(item.service_name)"
              size="small"
              type="danger"
              >{{
                $t("configCenter.serviceNameSelector.tags.configured")
              }}</ElTag
            >
            <ElTag v-else size="small" type="info">{{
              $t("configCenter.serviceNameSelector.tags.system")
            }}</ElTag>
          </div>
        </ElOption>
      </ElOptionGroup>

      <ElOptionGroup
        v-if="userServices.length > 0"
        :label="$t('configCenter.serviceNameSelector.groups.custom')"
      >
        <ElOption
          v-for="item in userServices"
          :key="item.service_name"
          :label="item.service_name"
          :value="item.service_name"
          :disabled="isExisting(item.service_name)"
        >
          <div class="service-option">
            <span
              :class="{
                'service-option-name': true,
                'is-existing': isExisting(item.service_name),
              }"
            >
              {{ item.service_name }}
            </span>
            <ElTag
              v-if="isExisting(item.service_name)"
              size="small"
              type="danger"
              >{{
                $t("configCenter.serviceNameSelector.tags.configured")
              }}</ElTag
            >
            <ElTag v-else size="small" type="success">{{
              $t("configCenter.serviceNameSelector.tags.custom")
            }}</ElTag>
          </div>
        </ElOption>
      </ElOptionGroup>
    </ElSelect>

    <ElEmpty
      v-if="!loading && serviceList.length === 0 && showEmptyState"
      :description="$t('configCenter.serviceNameSelector.empty.description')"
      :image-size="60"
    >
      <template #description>
        <div class="empty-state-content">
          <p>{{ $t("configCenter.serviceNameSelector.empty.title") }}</p>
          <p>{{ $t("configCenter.serviceNameSelector.empty.subtitle") }}</p>
        </div>
      </template>
      <ElButton type="primary" size="small" @click="goToServiceManagement">
        {{ $t("configCenter.serviceNameSelector.actions.manage") }}
      </ElButton>
    </ElEmpty>
  </ElFormItem>
</template>

<script setup lang="ts">
import { ElMessage } from "element-plus";
import { $t } from "@/locales";
import type { Service, ServiceStatus } from "@/types/api/service-registry";
import { fetchServiceList } from "@/api/service-registry";
import { runWithLoading } from "@/composables/useAsyncState";
import { logger } from "@/utils/sys";
import { useRouter } from "vue-router";
defineOptions({ name: "ServiceNameSelector" });
interface Props {
  modelValue: string;
  disabled?: boolean;
  showEmptyState?: boolean;
  existingNames?: string[];
}
interface Emits {
  "update:modelValue": [value: string];
  change: [value: string, description?: string | null];
}
const props = withDefaults(defineProps<Props>(), {
  disabled: false,
  showEmptyState: false,
  existingNames: () => [],
});
const isExisting = (name: string): boolean => {
  return props.existingNames.includes(name);
};
const emit = defineEmits<Emits>();
const router = useRouter();
const loading = ref(false);
const serviceList = ref<Service[]>([]);
const selectedService = computed({
  get: () => props.modelValue,
  set: (val) => emit("update:modelValue", val || ""),
});
const systemServices = computed(() => {
  return serviceList.value
    .filter((item) => item.source === "system")
    .sort((a, b) => a.service_name.localeCompare(b.service_name));
});
const userServices = computed(() => {
  return serviceList.value
    .filter((item) => item.source === "user")
    .sort((a, b) => a.service_name.localeCompare(b.service_name));
});
const loadServiceNames = async () => {
  await runWithLoading(loading, async () => {
    const data = await fetchServiceList({ status: "active" as ServiceStatus });
    if (Array.isArray(data)) {
      serviceList.value = data
        .filter(
          (item) =>
            item &&
            item.service_name &&
            item.service_name.trim() !== "" &&
            item.status === "active",
        )
        .sort((a, b) => a.service_name.localeCompare(b.service_name));
    } else {
      logger.error("Service data is not an array:", typeof data, data);
      serviceList.value = [];
    }
  }).catch((error) => {
    logger.error("Failed to load service names:", error);
    serviceList.value = [];
  });
};
const handleServiceChange = (value: string) => {
  const description = serviceList.value.find(
    (item) => item.service_name === value,
  )?.description;
  emit("change", value, description);
};
const goToServiceManagement = () => {
  router.push("/control-panel/certificates/services");
};
onMounted(() => {
  loadServiceNames();
});
defineExpose({
  loadServiceNames,
});
</script>

<style scoped lang="scss">
.service-option {
  display: flex;
  justify-content: space-between;
  align-items: center;
  width: 100%;
  padding-right: 8px;
}

.service-option-name {
  flex: 1;

  &.is-existing {
    text-decoration: line-through;
    color: var(--el-text-color-placeholder);
  }
}

.empty-state-content {
  text-align: center;
  color: var(--el-text-color-secondary);
  font-size: 14px;
  padding: 10px 0;

  p {
    margin: 0;
  }
}

:deep(.el-empty) {
  padding: 20px 0;
}
</style>
