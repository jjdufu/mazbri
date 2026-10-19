import { ref, watch, type Ref } from "vue";
import { useRoute, useRouter } from "vue-router";

export interface UseTabRouteSyncOptions<T extends string> {
  queryKey?: string;
  defaultValue: T;
  allowedValues: T[];
  onChange?: (value: T) => void | Promise<void>;
  resolver?: (queryVal: unknown) => T | undefined;
}

export function useTabRouteSync<T extends string>(
  options: UseTabRouteSyncOptions<T>,
) {
  const {
    queryKey = "tab",
    defaultValue,
    allowedValues,
    onChange,
    resolver,
  } = options;
  const route = useRoute();
  const router = useRouter();

  const normalizeQueryTab = (value: unknown) => {
    if (Array.isArray(value)) {
      return value[0];
    }
    return value;
  };

  const resolveTab = (): T => {
    const queryVal = normalizeQueryTab(route.query[queryKey]);

    if (resolver) {
      const resolved = resolver(queryVal);
      if (resolved !== undefined) {
        return resolved;
      }
    }

    if (queryVal && allowedValues.includes(queryVal as T)) {
      return queryVal as T;
    }
    return defaultValue;
  };

  const activeTab = ref<T>(resolveTab()) as Ref<T>;

  const handleTabChange = async (value: unknown) => {
    if (!allowedValues.includes(value as T)) {
      return;
    }
    await router.replace({
      query: {
        ...route.query,
        [queryKey]: value,
      } as any,
    });
  };

  watch(
    () => route.fullPath,
    async () => {
      const nextTab = resolveTab();
      if (activeTab.value !== nextTab) {
        activeTab.value = nextTab;
        if (onChange) {
          await onChange(nextTab);
        }
      }
    },
  );

  return {
    activeTab,
    handleTabChange,
  };
}
