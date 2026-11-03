import { computed, ref, watch, type Ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useOpsRole } from "./useOpsRole";
import {
  resolveOpsTab,
  type OpsTabDef,
  type ResolveOpsTabResult,
} from "@/utils/ops/tabs";

export interface UseOpsTabOptions<T extends string> {
  tabs: OpsTabDef<T>[] | Ref<OpsTabDef<T>[]>;
  queryKey?: string;
  /** 首选默认 tab；对当前角色不可见时回落首个可见。 */
  defaultTab?: T;
  onChange?: (value: T) => void | Promise<void>;
}

function unwrapTabs<T extends string>(
  tabs: OpsTabDef<T>[] | Ref<OpsTabDef<T>[]>,
): OpsTabDef<T>[] {
  return Array.isArray(tabs) ? tabs : tabs.value;
}

/**
 * 运维合并页 Tab 同步：按角色过滤可见 Tab，读写 `?tab=`，非法深链回落。
 * 基于既有 `useTabRouteSync` 语义，并加上 RBAC 过滤（后续步骤唯一入口）。
 */
export function useOpsTab<T extends string>(options: UseOpsTabOptions<T>) {
  const queryKey = options.queryKey ?? "tab";
  const route = useRoute();
  const router = useRouter();
  const { role } = useOpsRole();

  const resolved = computed<ResolveOpsTabResult<T>>(() =>
    resolveOpsTab({
      role: role.value,
      tabs: unwrapTabs(options.tabs),
      queryTab: route.query[queryKey],
      preferredDefault: options.defaultTab,
    }),
  );

  const visibleTabs = computed(() => resolved.value.visibleTabs);
  const allowedIds = computed(() =>
    visibleTabs.value.map((tab) => tab.id),
  );

  const activeTab = ref<T | undefined>(resolved.value.activeTab) as Ref<
    T | undefined
  >;

  async function syncQuery(next: T) {
    if (route.query[queryKey] === next) {
      return;
    }
    await router.replace({
      query: {
        ...route.query,
        [queryKey]: next,
      },
    });
  }

  async function handleTabChange(value: unknown) {
    if (typeof value !== "string") {
      return;
    }
    if (!allowedIds.value.includes(value as T)) {
      return;
    }
    const next = value as T;
    activeTab.value = next;
    await syncQuery(next);
    if (options.onChange) {
      await options.onChange(next);
    }
  }

  watch(
    resolved,
    async (next) => {
      if (next.activeTab === undefined) {
        activeTab.value = undefined;
        return;
      }
      if (activeTab.value !== next.activeTab) {
        activeTab.value = next.activeTab;
        if (options.onChange) {
          await options.onChange(next.activeTab);
        }
      }
      // 深链非法或不属于当前角色：写回合法 tab，避免残留非法 id。
      if (next.fellBack || route.query[queryKey] !== next.activeTab) {
        await syncQuery(next.activeTab);
      }
    },
    { immediate: true },
  );

  return {
    activeTab,
    visibleTabs,
    allowedIds,
    handleTabChange,
    fellBack: computed(() => resolved.value.fellBack),
  };
}

export type { OpsTabDef };
