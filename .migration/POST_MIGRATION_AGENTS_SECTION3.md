## 三、UI 规范与公共组件铁律（`[不可更改]`）

1. **技术栈与 Token 单源收敛（React 19 + Tailwind CSS v4 + 自研原语）**
   - 前端统一基于 `React 19 + TypeScript + Tailwind CSS v4` 与自研 UI 原语层（`src/components/ui/`），严禁引入 Element Plus、Ant Design、Art Design Pro 等重型第三方 UI 组件库或新增 Sass 样式覆盖层。
   - 所有颜色、表面层级、边框、Hover/Active 反馈必须绑定 `src/styles/globals.css` 语义 CSS 变量（`--canvas-bg`、`--card`、`--hover-bg`、`--active-bg`、`--border`、`--border-hover`、`--primary`、`--primary-hover`），严禁在业务组件中硬编码亮色 Hex 背景色（如 `bg-white`、`#ffffff`），确保亮/暗双模式与 Hover 对比度达标且暗色下严禁出现纯白 Tooltip。
   - 几何与尺寸严格收敛：圆角仅允许 `rounded-md`（6px，按钮/输入框/下拉/分段器）、`rounded-lg`（10px，卡片/弹窗/弹层）、`rounded-full`（状态胶囊/头像）；按钮与控件高度仅允许 `32px`（标准）/ `28px`（紧凑）/ `24px`（表格行内 Link）。

2. **强制复用公共骨架与原语，禁止“各写各的”**
   - **页头与标题**：页面级主标题统一由顶栏面包屑承载，页面内部禁止再渲染与菜单/面包屑同名的重复大标题或重复记录数标签。
   - **列表页**：统一使用 `DataTablePage`（搜索、筛选、操作收进主卡片 Header，左筛右操；0 选中时隐藏或禁用批量危险操作；真服务端分页与准确 `total`；数值列右对齐且表头/单元格一致）。
   - **详情页**：统一使用 `EntityDetailLayout`（身份页头 + 全局操作 → Tab 栏 → 带标题内容卡片）。
   - **设置页**：统一使用 `SettingsSectionLayout`（双栏等高拉伸、图标风格统一，严禁铺设全宽空白保存横条）。
   - **弹窗与抽屉**：统一使用 `FormModal` / `Modal` / `FormDrawer` / `Drawer` / `ConfirmDialog`（强制 Header 底线 + 可滚 Body + Footer 顶线；宽度锁定 `480 / 600 / 760px` 三档；表单默认 `label-top` 防折行重叠；超 6 字段、向导或代码编辑器一律使用右侧 Drawer）。
   - **图表与可观测**：统一使用 `ChartCard` 与标准 ECharts 包装器（同排空态一致、Y 轴单位统一、强制 `UTC+8` 时区、亮/暗主题联动与 `ResizeObserver` 自适应）。

3. **核心业务与体验不变量（`U1 ~ U4` 永久守卫）**
   - **U1**：`MfaStepUpCancelledError` 与网络取消严格正交，受 MFA 保护端点取消提权后不得打开空白可保存表单。
   - **U2**：金额（元/分/微元）与东八区墙钟（`UTC+8`）全链路同构，严禁量纲错位或裸 `Z` 时区偏移。
   - **U3**：真服务端分页、无硬截断编码列、高饱和告警色仅用于真实异常。
   - **U4**：保持各权限安全域边界不降级；中/英切换必须无刷新热切，不销毁当前未保存表单状态；全仓 `t("...")` 词条须在双语字典中 100% 存在。
