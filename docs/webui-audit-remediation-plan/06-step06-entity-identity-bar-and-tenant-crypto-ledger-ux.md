# Step 06：轻量实体身份顶栏原语、全站状态圆点徽标统一与租户/产品/供应商详情精修

## 1. 覆盖问题清单与源码精确定位

| 编号 | 关联审计项 | 级别 | 问题描述与核验纠偏 | 精确源码位置 |
| :--- | :---: | :---: | :--- | :--- |
| **W06-1** | `B-06`, `B-08` | P1 | **组合优于重型壳纠偏**：产品详情手写 `.product-detail-shell`（首 Tab 误标为 `产品信息与限流` 却无任何限流属性，`内部ID` 未用 `CopyableText`，修改/删除按钮图标风格不一）；供应商详情手写 `.supplier-detail-page`（缺失顶部实体名称与编码身份条，卡片右上角重复渲染 `[启用]` Tag 与刺眼实心红删除按钮，`ElDescriptions` 窄标签把「密钥仓」拆出孤字 `钥仓`，编辑态残留复合动作文案 `saveAndBack`） | [products/[id]/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/products/%5Bid%5D/index.vue#L1-L95)、[suppliers/[id]/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/suppliers/%5Bid%5D/index.vue#L35-L115) |
| **W06-2** | `B-07` | P1 | `上游供给 -> 供应商列表` 编码列定宽过窄，将 `UPUPT-ENV-CC`、`UPUPT-ENV-GCM`、`UPUPT-ENV-SM4` 等 7 个不同套件的供应商截成一模一样的 `UPUPT-EN...`；全站行内「启用/停用」生命周期状态分裂为方框 `<ElTag>`、圆点 `<StatusIndicator>` 与无圆点纯绿字三种视觉形态 | [suppliers/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/suppliers/index.vue#L60-L92)、[channels/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/channels/index.vue#L70-L80) |
| **W06-3** | `B-02` | P1 | 租户详情 `概览 360` 五张指标卡无差异化语义图标、`时段内消费` 缺失货币单位且仅单卡带时间窗副标题；下方仅两列的 `错误分布` 表横向拉伸 `1740px` 造成 `850px+` 跨列空白；`API 密钥` 与 `IP 白名单` 子表内联表单项未清零底部边距；`page-templates.md` 关于详情 Tab 控件的文档描述与 8-Tab `ElTabs` 实现脱节 | [overview360.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/tenants/%5Bcode%5D/tabs/overview360.vue#L1-L65)、[page-templates.md](file:///workspace/rust-proj/mazbri/mazbri-web/docs/page-templates.md#L42-L53) |
| **W06-4** | `B-03` | P1 | **状态显式化纠偏**：租户详情 `传输加密` 中 `CryptoSuiteSelect` 代码虽已实现双向互斥清空，但界面以“推荐套件下拉框为空 + 高级折叠面板展开”隐式表达自定义模式，用户难以感知二选一边界；且左右 `50%:50%` 对称网格在左高（`~790px`）右矮（`~190px`）时产生 `600px` 右下空白 | [CryptoSuiteSelect.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/crypto/CryptoSuiteSelect.vue#L1-L121)、[transport-encryption.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/tenants/%5Bcode%5D/tabs/transport-encryption.vue#L31-L242) |
| **W06-5** | `B-04`, `B-05` | P1/P2 | 租户 `账户余额` 状态词裸浮无标签、中部 `1100px` 空白断层、流水 `余额变化` 用除号斜杠 `/` 替代流向箭头 `→`；`产品治理` 目录表仅 5 列跨屏留白 `350px+` 却隐藏了 `超时时间` 与 `高级熔断` 关键治理字段 | [account.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/tenants/%5Bcode%5D/tabs/account.vue#L20-L95)、[products/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/products/index.vue#L169-L204) |

---

## 2. 第一性原理本质归因

1. **轻量原语组合优于上帝壳层继承（Composition over God-Shell，红队修正准则）**：租户（8-Tab 运营工作空间）、产品（双页签属性与绑定）、供应商（原地读写切换表单 + 工作台跳板）的主体交互流拓扑完全不同。强行抽象单一重型详情布局壳会导致条件插槽膨胀；真正跨实体恒定的契约是**顶栏实体身份锚点（名称 + 可复制业务编码 + 生命周期状态徽标 + 右侧主次动作簇）**与**标准页外边距 `var(--ops-page-padding, 16px)`**。
2. **视觉符号双射原则（Bijective Visual Encoding，公理 U3）**：在同一设计系统内，「运行/启停状态」与「静态枚举分类（如协议类型、计费模式）」必须映射到互不相交的视觉原语：前者绑定圆点徽标 `<StatusIndicator>`，后者绑定方框标签 `<ElTag effect="plain">`，消除视觉词法歧义。
3. **有限状态机显式投射**：当领域模型为互斥和类型 `Suite(id) | CustomPrimitives(data_alg, mac_alg)` 时，UI 必须提供显式二选一模式控制器（`<ElSegmented>`），禁止依赖“清空 A 控件表示激活 B 控件”的隐式约定。

---

## 3. 详尽可落地技术方案

### 3.1 抽离 `<EntityIdentityBar>` 组合原语并统一全站 `<StatusIndicator>` 规范（`mazbri-web`）

1. **新建公共组件 `components/common/entity-identity-bar/index.vue`（`W06-1`）**
   - Props：`name: string`、`code?: string`、`codeLabel?: string`、`statusType?: StatusType`、`statusText?: string`；Slots：`tags`（附加分类枚举标签）、`actions`（右侧标准 `32px` 操作按钮组）。
   - 在 `TenantDetailShell`、[products/[id]/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/products/%5Bid%5D/index.vue) 与 [suppliers/[id]/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/suppliers/%5Bid%5D/index.vue) 统一挂载 `<EntityIdentityBar>`：
     - **产品详情**：修正 `zh.json` / `en.json` 首 Tab 词条为准确的 `基本信息` / `Overview`；为 `内部ID` 挂载 `<CopyableText>` 并移除描述表内与顶栏完全重复的编码展示；为「修改」按钮补齐 `ri:edit-line` 图标，与旁侧浅红描边「删除」（`type="danger" plain`）保持视觉对称。
     - **供应商详情**：顶栏展示供应商名称、可复制 `supplier_code` 与 `<StatusIndicator>` 状态，右侧收敛 `[工作台编辑] [编辑] [删除(plain danger)]`（编辑态切换为简洁的 `[取消] [保存]`，清除违反微文案规范的复合回调词条 `保存并返回`）；移除卡片头重复的 `[启用]` Tag，为下方 `ElDescriptions` 显式设置 `:label-width="168"` 与标签样式 `word-break: keep-all`，根除「密钥仓」孤字断行。
   - 同步更新 [page-templates.md](file:///workspace/rust-proj/mazbri/mazbri-web/docs/page-templates.md)：明确 ≤4 个紧凑切换项可用 `ElSegmented`，≥5 个详情页签统一使用 `ElTabs`，并补充 `<EntityIdentityBar>` 与 `<StatusIndicator>` 规范说明。

2. **统一行内生命周期状态与前缀密集编码展示（`W06-2`, `W06-5`）**
   - 全站巡检将表格行内「启用 / 停用 / 正常 / 冻结」状态列（含 [suppliers/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/suppliers/index.vue#L88-L90)、[products/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/products/index.vue#L185-L189) 及账单相关状态列）统一收敛为 `<StatusIndicator>` 圆点徽标；`<ElTag>` 仅保留给 `透明代理 / 微服务`、`按量计费` 等分类枚举。
   - 放宽 [suppliers/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/suppliers/index.vue) 的 `supplier_code` 列 `minWidth: 200`（完整展示 `UPUPT-ENV-SM4` 等 20 字符内编码），并在 `CopyableText` 支持超长编码（`> 24` 字符）保留首部 8 字符 + 尾部 8 字符高熵区分段，防止同前缀编码坍缩。
   - 在 [products/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/products/index.vue#L169-L203) 补入产品实体已携带的 `超时时间`（右对齐，`{timeout_secs}s`）与 `高级熔断`（`<StatusIndicator>`）列，消除 5 列跨屏留白断层。

### 3.2 租户详情 `概览 360`、`传输加密` 与 `账户余额` 深度精修（`W06-3`, `W06-4`, `W06-5`）

1. **`概览 360` 语义图标、货币单位与双栏错误分布（[overview360.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/tenants/%5Bcode%5D/tabs/overview360.vue)）**
   - 为 5 张 `StatCard` 分别传入语义图标（调用量 `ri:exchange-box-line`、成功率 `ri:checkbox-circle-line`、P95 `ri:timer-flash-line`、时段内消费 `ri:money-cny-circle-line`、健康分 `ri:heart-pulse-line`）。
   - 为 `时段内消费` 补齐币种前缀/单位并复用 Step 03 的 `minDigits: 2, maxDigits: 4` 自适应精度（如 `¥84.00` 或亚分 `¥0.0045`），将统计时间窗移至卡片组右上角统一展示，对齐 5 卡底部基线。
   - 将 `错误分布` 升级为双栏编排（有数据时左栏渲染紧凑横向占比条形图 `CategoryBarChart horizontal`，右栏渲染含 `错误码`、`次数`、`占比` 三列的紧凑右对齐明细表），消除两列横跨 `1740px` 的空旷割裂；同步清零 `api-keys.vue` 与 `whitelist.vue` 工具栏内 `.el-form-item` 底部边距。

2. **`传输加密` 显式二选一模式切换与 `7:5` 重心平衡（[CryptoSuiteSelect.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/crypto/CryptoSuiteSelect.vue) & [transport-encryption.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/tenants/%5Bcode%5D/tabs/transport-encryption.vue)）**
   - 在 `CryptoSuiteSelect.vue` 顶部引入显式模式切换 `<ElSegmented v-model="selectionMode" :options="[{ label: $t('tenant.transportEncryption.modeSuite'), value: 'suite' }, { label: $t('tenant.transportEncryption.modeCustom'), value: 'custom' }]" />`：
     - `suite` 模式下展示推荐套件选择器并以只读胶囊回显其对应的底层数据加密与 MAC 算法三元组，隐藏自定义下拉框；
     - `custom` 模式下直接平铺展开数据算法与 MAC 算法选择器（移除易产生歧义的折叠框外壳），并在选中 Legacy 弱算法（如 `des-cbc-pkcs5`、`aes-128-ecb`）时紧随渲染醒目合规风险提示条。
   - 在 `transport-encryption.vue` 将双栏比例优化为 `minmax(0, 7fr) minmax(0, 5fr)`（移动端自动单栏），并在右栏 `传输密钥管理` 下方补充紧凑的「当前生效接入契约摘要」只读预览卡（汇总当前加密模式、生效算法组、激活密钥版本、签名要求与重放窗口），既辅助租户联调自检又自然平衡左右栏高度。

3. **`账户余额` 四栏指标条与账本流向符号（[account.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/tenants/%5Bcode%5D/tabs/account.vue)）**
   - 将 `账户概览` 重构为水平对齐的 4 栏紧凑指标组（`现金余额` | `授信 / 可用额度` | `账户状态`（挂载 `<StatusIndicator>` 承托「正常 / 冻结」） | `近期消费`）+ 右侧对齐的 `[充值]` `[退款]` 按钮组，消除中部 `1100px` 空白断层。
   - 将下方交易记录表中 `余额变化` 列的分隔符由易误读为除法的 `' / '` 替换为语义清晰的流向箭头 `' → '`（如 `999.90 → 1,989.81`），并为 `金额` 与 `余额变化` 开启右对齐与 `tabular-nums`。

---

## 4. 二阶、三阶影响与红队边界守卫

| 风险场景与反例 | 连锁影响分析 | 防护与消解措施 |
| :--- | :--- | :--- |
| **反例 1：存量租户已持久化自定义算法（`suite: null, data_alg: 'des-cbc-pkcs5'`）反开页面时的模式回显** | 若 `selectionMode` 默认硬编码初始化为 `'suite'`，存量自定义算法租户打开页面会看到错误的模式页签甚至在误点保存时清空原配置。 | `selectionMode` 通过 `watch(() => [modelValue.suite, modelValue.dataAlg], ..., { immediate: true })` 精确派生：凡 `dataAlg` 非空且 `suite` 为空时自动定位至 `'custom'`，无损兼容全部存量数据库记录。 |
| **反例 2：供应商详情切换至 `editing` 编辑态时的焦点与离开保护** | 将操作按钮移至顶部 `<EntityIdentityBar #actions>` 后，需确保表单校验引用 `formRef` 仍然直接连通。 | `<EntityIdentityBar>` 为纯受控展示与事件透传组件（`@edit`、`@save`、`@cancel`、`@delete`），表单状态与校验实例完全保留在 `suppliers/[id]/index.vue` 自身作用域内，零跨组件状态耦合。 |

---

## 5. 验收门禁与实测清单

1. **自动化单测必测项**
   - 为 `CryptoSuiteSelect` 状态派生逻辑编写单测：验证 `{ suite: 'suite/tenant-default-2026-11', dataAlg: null }` 初始化为 `'suite'` 模式，`{ suite: null, dataAlg: 'des-cbc-pkcs5' }` 初始化为 `'custom'` 模式，且切换至 `'suite'` 选定套件后 `dataAlg` 与 `macAlg` 保持 `null` 正交契约。
2. **构建、热替换与实机核验**
   - 执行 `mazbri-web` `pnpm build && pnpm test` 与 `mazbri-bin -p mazbri-mgmt` 门禁，热替换重启 `mazbri-mazbri-mgmt-1`。
   - 浏览器巡检：① 对比租户详情、产品详情、供应商详情顶栏身份条一致性，验证产品详情首 Tab 显示 `基本信息`、供应商详情无孤字断行与重复状态标签；② 在 `/#/operations/upstream-supply` 确认 7 个 `UPUPT-ENV-*` 供应商编码完整可辨、各模块行内启用状态统一为圆点徽标；③ 核验租户 `概览 360`、`传输加密` 与 `账户余额` 三页签在双分辨率与亮暗双主题下的排版平衡。
