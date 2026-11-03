import { ref, type Ref } from "vue";
import { useOpsRole } from "./useOpsRole";
import {
  runRoleGatedTasks,
  selectRoleGatedTasks,
  type RoleGatedTask,
  type RoleGateLevel,
} from "@/utils/ops/role-gated-loader";

export type { RoleGatedTask, RoleGateLevel };

export interface UseRoleGatedLoaderOptions<TKey extends string, TResult> {
  tasks: readonly RoleGatedTask<TKey>[];
  run: (key: TKey) => Promise<TResult>;
}

/**
 * Dashboard / 概览按角色拆 Promise：观察者不打 BIZ/ADMIN API。
 * 返回的 `load` 只执行当前角色允许的任务。
 */
export function useRoleGatedLoader<TKey extends string, TResult>(
  options: UseRoleGatedLoaderOptions<TKey, TResult>,
) {
  const { role } = useOpsRole();
  const loading = ref(false);
  const results: Ref<Partial<Record<TKey, TResult>>> = ref({});

  function plannedKeys(): TKey[] {
    return selectRoleGatedTasks(role.value, options.tasks).map(
      (task) => task.key,
    );
  }

  async function load(): Promise<Partial<Record<TKey, TResult>>> {
    loading.value = true;
    try {
      const next = await runRoleGatedTasks({
        role: role.value,
        tasks: options.tasks,
        run: options.run,
      });
      results.value = next;
      return next;
    } finally {
      loading.value = false;
    }
  }

  return {
    loading,
    results,
    plannedKeys,
    load,
  };
}
