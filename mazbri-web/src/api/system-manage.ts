import request from "@/utils/http";
import { AppRouteRecord } from "@/types/router";
export function fetchGetUserList(params: Api.SystemManage.UserSearchParams) {
  return request.get<Api.SystemManage.UserList>({
    url: "/api/user/list",
    params,
  });
}
export function fetchGetRoleList(params: Api.SystemManage.RoleSearchParams) {
  return request.get<Api.SystemManage.RoleList>({
    url: "/api/role/list",
    params,
  });
}
export function fetchGetMenuList() {
  return request.get<AppRouteRecord[]>({
    url: "/api/v3/system/menus",
  });
}
