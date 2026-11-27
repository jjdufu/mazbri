import type { ComputedRef } from "vue";
import type { AuthorizationActionButton } from "./buttons";
import { useAuthorizationManagementButtons } from "./buttons";
import { useAuthorizationManagementActions } from "./actions";
import {
  useAuthorizationManagementState,
  type AuthorizationManagementContext,
} from "./state";

export type AuthorizationManagementPageContext = AuthorizationManagementContext & {
  actionButtons: ComputedRef<AuthorizationActionButton[]>;
};

export function useAuthorizationManagementPage(): AuthorizationManagementPageContext {
  const management = useAuthorizationManagementState();
  const authorizationActions = useAuthorizationManagementActions({
    state: management,
  });
  const actionButtons = useAuthorizationManagementButtons({
    state: management,
    actions: authorizationActions,
  });
  return {
    ...management,
    actionButtons,
  };
}
