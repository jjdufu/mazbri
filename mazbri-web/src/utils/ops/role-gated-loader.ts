import { canAccessRoles, type RoleLike } from "./roles";

export type RoleGateLevel = "all" | "biz" | "admin";

export interface RoleGatedTask<TKey extends string = string> {
  key: TKey;
  /** 运行该任务所需最低门控；默认 all。 */
  gate?: RoleGateLevel;
  /** 若提供，覆盖 gate，按显式角色列表判定。 */
  roles?: readonly string[];
}

const GATE_ROLES: Record<RoleGateLevel, readonly string[]> = {
  all: ["R_ADMIN", "R_FINANCE", "R_VIEWER"],
  biz: ["R_ADMIN", "R_FINANCE"],
  admin: ["R_ADMIN"],
};

/** 选出当前角色允许执行的任务（观察者不跑 BIZ/ADMIN）。 */
export function selectRoleGatedTasks<T extends RoleGatedTask>(
  role: RoleLike,
  tasks: readonly T[],
): T[] {
  return tasks.filter((task) => {
    if (task.roles && task.roles.length > 0) {
      return canAccessRoles(role, task.roles);
    }
    const gate = task.gate ?? "all";
    return canAccessRoles(role, GATE_ROLES[gate]);
  });
}

/**
 * 按角色并行执行任务；未授权 key 不会调用 factory，结果中也不会出现。
 * factory 返回的 Promise 原样 settle（调用方可自行 catch）。
 */
export async function runRoleGatedTasks<
  TKey extends string,
  TResult,
>(options: {
  role: RoleLike;
  tasks: readonly RoleGatedTask<TKey>[];
  run: (key: TKey) => Promise<TResult>;
}): Promise<Partial<Record<TKey, TResult>>> {
  const selected = selectRoleGatedTasks(options.role, options.tasks);
  const entries = await Promise.all(
    selected.map(async (task) => {
      const value = await options.run(task.key);
      return [task.key, value] as const;
    }),
  );
  return Object.fromEntries(entries) as Partial<Record<TKey, TResult>>;
}
