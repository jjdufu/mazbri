import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router";
import { Loader2, Pencil } from "lucide-react";
import {
  fetchChannelById,
  type Channel,
  type ChannelBoundRoute,
} from "@/api/route-management";
import {
  Badge,
  Button,
  CopyableText,
  Drawer,
  EmptyState,
  StatusBadge,
} from "@/components/ui";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import { EMPTY_PLACEHOLDER, formatDateTime } from "@/utils/format";
import { isBizRole } from "@/utils/ops/roles";
import { logger } from "@/utils/sys/logger";
import { showErrorToast } from "@/utils/ui/message-guard";
import { formatCostPrice } from "./costPrice";

export interface ChannelDetailDrawerProps {
  open: boolean;
  channelId: string;
  reloadToken?: number;
  onClose: () => void;
  onEdit: (channel: Channel) => void;
}

function formatRouteProduct(row: ChannelBoundRoute) {
  if (row.product_name && row.product_code) {
    return `${row.product_name} (${row.product_code})`;
  }
  return row.product_code || row.product_name || row.product_uuid;
}

function formatRouteTenant(row: ChannelBoundRoute) {
  if (row.tenant_name && row.tenant_code) {
    return `${row.tenant_name} (${row.tenant_code})`;
  }
  return row.tenant_code || row.tenant_name || row.tenant_id || EMPTY_PLACEHOLDER;
}

function formatSwitchCondition(cond: Record<string, unknown> | undefined) {
  const timeout =
    typeof cond?.timeout_ms === "number" ? `${cond.timeout_ms}ms` : EMPTY_PLACEHOLDER;
  const retries =
    typeof cond?.max_retries === "number" ? `${cond.max_retries}` : EMPTY_PLACEHOLDER;
  return `${timeout} / ${retries}`;
}

function statusKey(status: string): string {
  if (status === "active") return "ACTIVE";
  if (status === "suspended") return "SUSPENDED";
  return "DISABLED";
}

export function ChannelDetailDrawer({
  open,
  channelId,
  reloadToken = 0,
  onClose,
  onEdit,
}: ChannelDetailDrawerProps) {
  const { t } = useT();
  const navigate = useNavigate();
  const role = useAuthStore((s) => s.info.role);
  const canWrite = isBizRole(role);

  const [loading, setLoading] = useState(false);
  const [channel, setChannel] = useState<Channel | null>(null);

  const loadDetail = useCallback(async () => {
    if (!channelId) {
      setChannel(null);
      return;
    }
    setLoading(true);
    try {
      const data = await fetchChannelById(channelId);
      setChannel(data);
    } catch (error) {
      logger.error("Failed to load channel detail:", error);
      showErrorToast(t("channels.detail.loadFailed"));
    } finally {
      setLoading(false);
    }
  }, [channelId, t]);

  useEffect(() => {
    if (open && channelId) {
      void loadDetail();
    }
  }, [open, channelId, reloadToken, loadDetail]);

  const channelStatusLabel = (s: string) => {
    if (s === "active") return t("channels.status.active");
    if (s === "inactive") return t("channels.status.inactive");
    if (s === "suspended") return t("channels.status.suspended");
    return s || EMPTY_PLACEHOLDER;
  };

  const boundRoutes = channel?.routes ?? [];
  const hasCustomConfig = Boolean(
    channel?.config && Object.keys(channel.config).length > 0,
  );

  return (
    <Drawer
      open={open}
      onClose={onClose}
      title={channel?.name || t("channels.detail.pageTitle")}
      subtitle={channel?.channel_code || ""}
      size="md"
      testId="channel-detail-drawer"
      footer={
        canWrite && channel ? (
          <Button
            type="button"
            variant="primary"
            size="md"
            onClick={() => onEdit(channel)}
          >
            <Pencil className="h-3.5 w-3.5" />
            {t("channels.actions.edit")}
          </Button>
        ) : undefined
      }
    >
      {loading && !channel ? (
        <div className="flex items-center justify-center py-12">
          <Loader2 className="h-5 w-5 animate-spin text-primary" />
        </div>
      ) : channel ? (
        <div className="space-y-5">
          {/* Basic Info */}
          <section className="space-y-2">
            <h3 className="text-xs font-semibold text-foreground">
              {t("channels.detail.basicInfoTitle")}
            </h3>
            <dl className="grid grid-cols-1 sm:grid-cols-2 border border-border rounded-lg overflow-hidden">
              <div className="grid grid-cols-[110px_1fr] border-b border-border">
                <dt className="bg-card px-2.5 py-2 text-muted-foreground border-r border-border">
                  {t("channels.fields.channelCode")}
                </dt>
                <dd className="px-2.5 py-2 min-w-0">
                  <CopyableText value={channel.channel_code} />
                </dd>
              </div>
              <div className="grid grid-cols-[110px_1fr] border-b border-border sm:border-l">
                <dt className="bg-card px-2.5 py-2 text-muted-foreground border-r border-border">
                  {t("common.fields.status")}
                </dt>
                <dd className="px-2.5 py-2">
                  <StatusBadge
                    status={statusKey(channel.status)}
                    label={channelStatusLabel(channel.status)}
                  />
                </dd>
              </div>
              <div className="grid grid-cols-[110px_1fr] border-b border-border">
                <dt className="bg-card px-2.5 py-2 text-muted-foreground border-r border-border">
                  {t("channels.fields.name")}
                </dt>
                <dd className="px-2.5 py-2">{channel.name || EMPTY_PLACEHOLDER}</dd>
              </div>
              <div className="grid grid-cols-[110px_1fr] border-b border-border sm:border-l">
                <dt className="bg-card px-2.5 py-2 text-muted-foreground border-r border-border">
                  {t("channels.fields.pathPrefix")}
                </dt>
                <dd className="px-2.5 py-2 font-mono-tech">
                  {channel.path_prefix || EMPTY_PLACEHOLDER}
                </dd>
              </div>
              <div className="grid grid-cols-[110px_1fr] border-b border-border">
                <dt className="bg-card px-2.5 py-2 text-muted-foreground border-r border-border">
                  {t("channels.fields.costPrice")}
                </dt>
                <dd className="px-2.5 py-2 font-mono-tech tabular-nums">
                  {formatCostPrice(channel.cost_price)}
                </dd>
              </div>
              <div className="grid grid-cols-[110px_1fr] border-b border-border sm:border-l">
                <dt className="bg-card px-2.5 py-2 text-muted-foreground border-r border-border">
                  {t("channels.fields.id")}
                </dt>
                <dd className="px-2.5 py-2 min-w-0">
                  <CopyableText value={channel.id} truncate />
                </dd>
              </div>
              <div className="grid grid-cols-[110px_1fr]">
                <dt className="bg-card px-2.5 py-2 text-muted-foreground border-r border-border">
                  {t("common.fields.createdAt")}
                </dt>
                <dd className="px-2.5 py-2 font-mono-tech">
                  {formatDateTime(channel.created_at)}
                </dd>
              </div>
              <div className="grid grid-cols-[110px_1fr] border-t sm:border-t-0 border-border sm:border-l">
                <dt className="bg-card px-2.5 py-2 text-muted-foreground border-r border-border">
                  {t("common.fields.updatedAt")}
                </dt>
                <dd className="px-2.5 py-2 font-mono-tech">
                  {formatDateTime(channel.updated_at)}
                </dd>
              </div>
            </dl>
          </section>

          {/* Supplier Section */}
          <section className="space-y-2">
            <h3 className="text-xs font-semibold text-foreground">
              {t("channels.detail.supplierSectionTitle")}
            </h3>
            <dl className="grid grid-cols-1 sm:grid-cols-2 border border-border rounded-lg overflow-hidden">
              <div className="grid grid-cols-[110px_1fr]">
                <dt className="bg-card px-2.5 py-2 text-muted-foreground border-r border-border">
                  {t("channels.fields.supplier")}
                </dt>
                <dd className="px-2.5 py-2 flex items-center gap-2 flex-wrap">
                  <span>{channel.supplier_name || EMPTY_PLACEHOLDER}</span>
                  {channel.supplier_id && (
                    <Button
                      type="button"
                      variant="link"
                      size="xs"
                      onClick={() => {
                        onClose();
                        navigate(
                          `/operations/upstream-supply/suppliers/${channel.supplier_id}`,
                        );
                      }}
                    >
                      {t("channels.detail.viewSupplier")}
                    </Button>
                  )}
                </dd>
              </div>
              <div className="grid grid-cols-[110px_1fr] border-t sm:border-t-0 border-border sm:border-l">
                <dt className="bg-card px-2.5 py-2 text-muted-foreground border-r border-border">
                  {t("supplier.fields.code")}
                </dt>
                <dd className="px-2.5 py-2 min-w-0">
                  <CopyableText value={channel.supplier_code} />
                </dd>
              </div>
            </dl>
          </section>

          {/* Rate Limit & Egress */}
          <section className="space-y-2">
            <h3 className="text-xs font-semibold text-foreground">
              {t("channels.detail.healthSectionTitle")}
            </h3>
            <dl className="grid grid-cols-1 sm:grid-cols-2 border border-border rounded-lg overflow-hidden">
              <div className="grid grid-cols-[110px_1fr]">
                <dt className="bg-card px-2.5 py-2 text-muted-foreground border-r border-border">
                  {t("channels.fields.qpsLimit")}
                </dt>
                <dd className="px-2.5 py-2">
                  {channel.qps_limit === 0 ? (
                    <Badge variant="outline">
                      {t("channels.fields.qpsUnlimited")}
                    </Badge>
                  ) : (
                    <span className="font-mono-tech tabular-nums">
                      {channel.qps_limit} {t("channels.fields.qpsUnit")}
                    </span>
                  )}
                </dd>
              </div>
              <div className="grid grid-cols-[110px_1fr] border-t sm:border-t-0 border-border sm:border-l">
                <dt className="bg-card px-2.5 py-2 text-muted-foreground border-r border-border">
                  {t("channels.detail.egressMode")}
                </dt>
                <dd className="px-2.5 py-2">
                  {hasCustomConfig
                    ? t("channels.detail.egressConfigured")
                    : t("channels.detail.egressDefault")}
                </dd>
              </div>
            </dl>
          </section>

          {/* Bound Routes */}
          <section className="space-y-2">
            <h3 className="text-xs font-semibold text-foreground">
              {t("channels.detail.routesSectionTitle")}
            </h3>
            {boundRoutes.length > 0 ? (
              <div className="overflow-x-auto rounded-lg border border-border">
                <table className="w-full border-collapse text-xs">
                  <thead>
                    <tr className="border-b border-border bg-card text-muted-foreground">
                      <th className="h-8 px-2.5 text-left font-medium">
                        {t("channels.detail.routeProduct")}
                      </th>
                      <th className="h-8 px-2.5 text-left font-medium">
                        {t("channels.detail.routeTenant")}
                      </th>
                      <th className="h-8 px-2.5 text-right font-medium w-16">
                        {t("channels.detail.routePriority")}
                      </th>
                      <th className="h-8 px-2.5 text-right font-medium w-16">
                        {t("channels.detail.routeOrder")}
                      </th>
                      <th className="h-8 px-2.5 text-right font-medium w-28">
                        {t("channels.detail.routeTimeout")}
                      </th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-border-subtle">
                    {boundRoutes.map((r) => (
                      <tr key={r.binding_id} className="hover:bg-hover-bg/60">
                        <td className="px-2.5 py-2">{formatRouteProduct(r)}</td>
                        <td className="px-2.5 py-2">
                          {!r.tenant_id ? (
                            <Badge variant="outline">
                              {t("channels.detail.routeGlobal")}
                            </Badge>
                          ) : (
                            formatRouteTenant(r)
                          )}
                        </td>
                        <td className="px-2.5 py-2 text-right font-mono-tech tabular-nums">
                          {r.priority}
                        </td>
                        <td className="px-2.5 py-2 text-right font-mono-tech tabular-nums">
                          #{r.channel_order}
                        </td>
                        <td className="px-2.5 py-2 text-right font-mono-tech tabular-nums">
                          {formatSwitchCondition(r.switch_condition)}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            ) : (
              <EmptyState title={t("channels.detail.routesEmpty")} />
            )}
          </section>
        </div>
      ) : null}
    </Drawer>
  );
}
