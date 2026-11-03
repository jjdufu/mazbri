<template>
  <slot v-if="allowed" />
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useOpsRole } from "@/composables/ops/useOpsRole";
import type { RoleGateLevel } from "@/utils/ops/role-gated-loader";
import {
  canAccessRoles,
  ROLES_ADMIN,
  ROLES_ALL,
  ROLES_BIZ,
} from "@/utils/ops/roles";

defineOptions({ name: "RoleGate" });

const props = withDefaults(
  defineProps<{
    /** 显式允许角色列表；与 `gate` 二选一，优先 `roles`。 */
    roles?: readonly string[];
    /** 快捷门控：all / biz / admin。 */
    gate?: RoleGateLevel;
  }>(),
  {
    roles: undefined,
    gate: "all",
  },
);

const GATE_ROLES: Record<RoleGateLevel, readonly string[]> = {
  all: ROLES_ALL,
  biz: ROLES_BIZ,
  admin: ROLES_ADMIN,
};

const { role } = useOpsRole();

const allowed = computed(() => {
  if (props.roles && props.roles.length > 0) {
    return canAccessRoles(role.value, props.roles);
  }
  return canAccessRoles(role.value, GATE_ROLES[props.gate]);
});
</script>
