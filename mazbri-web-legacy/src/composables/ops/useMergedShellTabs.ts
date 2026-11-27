import { computed, type ComputedRef } from "vue";
import { useI18n } from "vue-i18n";
import type { OpsTabDef } from "@/utils/ops/tabs";
import {
  getMergedRouteTabs,
  MERGED_DEFAULT_TAB,
  type MergedOpsDomain,
  type MergedTabId,
} from "@/utils/ops/merged-route-tabs";

export type MergedTabLabelMap<D extends MergedOpsDomain> = Record<
  MergedTabId<D>,
  string
>;

/**
 * 为合并父路由壳注入带 i18n label 的 tabs + 默认 tab id。
 * labelMap 的 value 为 i18n key（如 menus.mazbri.infraPipeline）。
 */
export function useMergedShellTabs<D extends MergedOpsDomain>(
  domain: D,
  labelMap: MergedTabLabelMap<D>,
): {
  tabs: ComputedRef<OpsTabDef<MergedTabId<D>>[]>;
  defaultTab: MergedTabId<D>;
} {
  const { t } = useI18n();
  const seeds = getMergedRouteTabs(domain);
  const tabs = computed(() =>
    seeds.map((seed) => ({
      id: seed.id,
      roles: seed.roles,
      label: t(labelMap[seed.id]),
      disabled: seed.disabled,
    })),
  );
  return {
    tabs,
    defaultTab: MERGED_DEFAULT_TAB[domain],
  };
}
