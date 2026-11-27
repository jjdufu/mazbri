# 管理端页模板（强制）

所有业务页必须落到下列之一；禁止根节点直接堆多个无栅格 `ElCard`。

## 1. Ops 监控页 — `OpsPage`

适用：全局总览、经营看板、流量/计费面板、单栏运维图。

```vue
<OpsPage :title="…" :subtitle="时间窗或页说明">
  <template #leading><!-- 可选：Trace ID 等前置身份 --></template>
  <template #actions><!-- 主按钮槽：时间窗 / 刷新 --></template>
  <OpsKpiRow :columns="6"><!-- 原生 CSS Grid + StatCard，卡高由 token 统一 --></OpsKpiRow>
  <OpsCard title="主区"><!-- 一主表或主图 --></OpsCard>
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
    …
  />
</ControlPanelPageShell>
```

- 有 `pagination.total` 就必须渲染分页器与总条数。
- 启停/正常/冻结等生命周期状态统一使用 `<StatusIndicator>`；分类/协议枚举使用 `<ElTag effect="plain">`。
- 空表用 `empty-text` 或 `#empty` + `EmptyState`。

## 3. 实体详情页 — `EntityIdentityBar` + `TenantDetailShell` / `ElTabs`

适用：租户详情、产品详情、供应商详情。

```vue
<TenantDetailShell>
  <template #identity>
    <EntityIdentityBar
      :name="entity.name"
      :code="entity.code"
      :status-type="statusType"
      :status-text="statusText"
    />
  </template>
  <template #tabs>
    <ElTabs :model-value="activeTab" @tab-change="handleTabChange">…</ElTabs>
  </template>
  <TenantTabShell><!-- 当前 tab 内容 --></TenantTabShell>
</TenantDetailShell>
```

## 三态与异常页

| 态 | 组件 / 路由 |
|----|------|
| Empty | `EmptyState`（`compact` 用于内嵌卡片/表格空态） |
| Forbidden | 路由 `/exception/403`（`ArtException` + `#meta` 当前角色胶囊 + 返回/回首页/切换账号） |
| Loading | 壳上 `v-loading` / 表 `loading`；禁止闪空表 |

角色差异只体现在动作有无，**骨架同一套**。
