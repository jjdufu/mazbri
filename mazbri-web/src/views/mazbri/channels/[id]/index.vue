<template>
  <div class="channel-detail-page">
    <ElPageHeader @back="goBack">
      <template #content>
        <span class="text-lg font-medium">{{
          channel?.name || $t("channels.detail.pageTitle")
        }}</span>
      </template>
      <template #extra>
        <ElTag :type="channel?.status === 'active' ? 'success' : 'info'">
          {{
            channel?.status
              ? $t(`channels.status.${channel.status}`)
              : "-"
          }}
        </ElTag>
      </template>
    </ElPageHeader>

    <ElCard v-loading="loading" class="mt-5" shadow="never">
      <template #header>
        <SurfaceCardHeader
          :title="$t('channels.detail.basicInfoTitle')"
          stack-on-mobile
        >
          <template #actions>
            <ElButton
              v-roles="['R_ADMIN', 'R_FINANCE']"
              type="primary"
              @click="handleEdit"
            >
              {{ $t("common.actions.modify") }}
            </ElButton>
            <ElButton
              v-roles="['R_ADMIN', 'R_FINANCE']"
              type="danger"
              @click="handleDelete"
            >
              <template #icon>
                <ArtSvgIcon icon="ri:delete-bin-line" />
              </template>
              {{ $t("common.actions.delete") }}
            </ElButton>
          </template>
        </SurfaceCardHeader>
      </template>

      <ElDescriptions :column="2" border>
        <ElDescriptionsItem :label="$t('channels.fields.id')">
          {{ channel?.id || "-" }}
        </ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('channels.fields.channelCode')">
          {{ channel?.channel_code || "-" }}
        </ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('channels.fields.name')">
          {{ channel?.name || "-" }}
        </ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('channels.fields.supplier')">
          <template v-if="channel">
            {{ channel.supplier_name }} ({{ channel.supplier_code }})
          </template>
          <template v-else>-</template>
        </ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('channels.fields.pathPrefix')">
          {{ channel?.path_prefix || "-" }}
        </ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('channels.fields.costPrice')">
          {{ channel?.cost_price || "-" }}
        </ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('channels.fields.qpsLimit')">
          {{ channel?.qps_limit ?? "-" }}
        </ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('common.fields.status')">
          <ElTag :type="channel?.status === 'active' ? 'success' : 'info'">
            {{
              channel?.status
                ? $t(`channels.status.${channel.status}`)
                : "-"
            }}
          </ElTag>
        </ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('common.fields.createdAt')">
          {{ formatDateTime(channel?.created_at) }}
        </ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('common.fields.updatedAt')">
          {{ formatDateTime(channel?.updated_at) }}
        </ElDescriptionsItem>
      </ElDescriptions>
    </ElCard>
  </div>
</template>

<script setup lang="ts">
import { ElMessage, ElMessageBox } from "element-plus";
import { computed, onMounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import {
  deleteChannel,
  fetchChannelById,
  type Channel,
} from "@/api/route-management";
import SurfaceCardHeader from "@/components/common/surface-card-header/index.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { $t } from "@/locales";
import { formatDateTime } from "@/utils/format";
import { logger } from "@/utils/sys";

defineOptions({ name: "ChannelDetail" });

const route = useRoute();
const router = useRouter();
const loading = ref(false);
const channel = ref<Channel | null>(null);
const channelId = computed(() => route.params.id as string);

const goBack = () => {
  router.push({ name: "UpstreamSupply", query: { tab: "channels" } });
};

const loadData = async () => {
  try {
    await runWithLoading(loading, async () => {
      channel.value = await fetchChannelById(channelId.value);
    });
  } catch (error) {
    logger.error("Failed to load channel:", error);
    ElMessage.error($t("channels.detail.loadFailed"));
  }
};

const handleEdit = () => {
  router.push({
    name: "ChannelEdit",
    params: { id: channelId.value },
  });
};

const handleDelete = async () => {
  if (!channel.value) return;
  await ElMessageBox.confirm(
    $t("channels.messages.deleteConfirm", { name: channel.value.name }),
    $t("common.warning"),
    { type: "warning" },
  );
  await deleteChannel(channel.value.id);
  ElMessage.success($t("common.feedback.deleteSuccess"));
  goBack();
};

onMounted(loadData);
</script>
