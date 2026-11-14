# 管理端页模板（强制）

所有业务页必须落到下列之一；禁止根节点直接堆多个无栅格 `ElCard`。

## 1. Ops 监控页 — `OpsPage`

适用：全局总览、经营看板、流量/计费面板、单栏运维图。

```vue
<OpsPage :title="…" :subtitle="时间窗或页说明">
  <template #actions><!-- 主按钮槽：时间窗 / 刷新 / 打开大屏 --></template>
  <OpsKpiRow><!-- ElCol + StatCard，卡高由 token 统一 --></OpsKpiRow>
  <OpsCard title="主区"><!-- 一主表或主图 --></OpsCard>
  <!-- 至多一辅 -->
</OpsPage>
```

- 间距 / 卡高：`--ops-page-*` / `--ops-kpi-*`（`ops-layout-tokens.scss`）。
- **禁止**给 `OpsPage` 挂固定 `height` + 纵向 flex 压扁首卡（见 `OpsPage.vue` 注释）。
- 多 Tab 监控用 `OpsTabPage` / `OpsMergedShell`（内部仍是 `OpsPage`）。

## 2. 资源列表页 — `ResourceTablePageShell`

适用：租户列表、账单列表、已开通产品等有分页的资源表。

```vue
<ControlPanelPageShell :title="…" icon="…">
  <ResourceTablePageShell
    v-model="search"
    v-model:filters="filters"
    v-model:columns="columns"
    :data="…"
    :pagination="…"
    … <!-- 搜索/导入导出/主按钮/删除经 props，勿再手拼 ActionBar -->
  />
</ControlPanelPageShell>
```

- 有 `pagination.total` 就必须渲染分页器。
- 空表用 `empty-text` 或 `#empty` + `EmptyState`。

## 3. 租户详情 Tab 页 — `TenantDetailShell` + `TenantTabShell`

适用：`tenants/[code]` 及各 tab。

```vue
<TenantDetailShell>
  <template #header><!-- ElPageHeader / 标题 --></template>
  <template #extra><!-- 状态 Tag --></template>
  <template #tabs><!-- ElSegmented --></template>
  <TenantTabShell><!-- 当前 tab 内容；KPI 用 OpsKpiRow，卡用 OpsCard 或统一 surface --></TenantTabShell>
</TenantDetailShell>
```

## 三态

| 态 | 组件 |
|----|------|
| Empty | `EmptyState` |
| Forbidden | `ForbiddenBlock`（整页无权限仍走路由 `/403`） |
| Loading | 壳上 `v-loading` / 表 `loading`；禁止闪空表 |

角色差异只体现在动作有无，**骨架同一套**。
