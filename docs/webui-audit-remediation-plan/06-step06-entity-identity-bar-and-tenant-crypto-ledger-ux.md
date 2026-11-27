# Step 06：轻量实体身份顶栏、全站状态圆点双射、连接器草稿 RBAC 门控与详情精修

## 1. 覆盖问题清单与源码精确定位

| 编号 | 关联审计项 | 级别 | 问题描述与全栈契约定位 | 精确源码位置 |
| :--- | :---: | :---: | :--- | :--- |
| **W06-1** | `B-06`, `B-08` | P1 | 产品详情（3-Tab：`产品信息与限流` / `路由规则` / `已授权租户`）首 Tab 词条误写「限流」文不对题（且 `zh.json` 残留死键 `advancedInfoTitle`），`内部ID` 未挂 `<CopyableText>`，修改按钮缺图标；供应商详情缺失顶部实体身份条、卡片头重复渲染 `[启用]` Tag 与实心红删除按钮、`ElDescriptions` 窄标签把「密钥仓」拆出孤字 `钥仓`；同时 `TenantDetailShell` 的 `.tenant-detail-shell__identity` 锁死 `max-height: 28px` | [products/[id]/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/products/%5Bid%5D/index.vue#L1-L95)、[products/[id]/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/products/%5Bid%5D/index.vue#L428-L437)、[suppliers/[id]/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/suppliers/%5Bid%5D/index.vue#L35-L195)、[tenant-detail-shell/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/common/tenant-detail-shell/index.vue#L50-L58) |
| **W06-2** | `B-07`, `B-05` | P1 | 全站生命周期状态分裂为方框 `<ElTag>`（供应商、产品、用户、证书抽屉、账单、静默配置）、圆点 `<StatusIndicator>`（通道）与纯文本（经营概览余额表）三种形态；`产品目录` 表仅 5 列（`启用状态` 列空占 `726px`），未展示 DTO 已携带的 `timeout_secs` 与 `cb_pro_enabled`，且对只读 `R_VIEWER` 仍渲染多选勾选列；**`上游供给 -> 草稿箱`（`drafts.vue`）对 `R_VIEWER` 裸露新建、导入、批量删除与行内删除写控件** | [suppliers/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/suppliers/index.vue#L60-L92)、[products/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/products/index.vue#L129-L235)、[drafts.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/drafts.vue#L8-L55)、[drafts.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/drafts.vue#L140-L196) |
| **W06-3** | `B-02` | P1 | 租户详情 `概览 360` 五张指标卡共用默认图标、`时段内消费` 缺失货币单位，且前 4 卡（`近 24 小时`）与第 5 卡（`近 30 日`）统计时间窗标注不对称；下方两列 `错误分布` 表横向拉伸 `1740px`；`API 密钥` 与 `IP 白名单` 工具栏内联表单项未清零底部边距 | [overview360.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/tenants/%5Bcode%5D/tabs/overview360.vue#L1-L145)、[api-keys.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/tenants/%5Bcode%5D/tabs/api-keys.vue)、[whitelist.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/tenants/%5Bcode%5D/tabs/whitelist.vue) |
| **W06-4** | `B-03`, `B-04` | P1/P2 | 租户 `传输加密` 以「推荐套件为空 + 折叠面板展开」隐式表达自定义算法模式，缺乏显式二选一控制器与弱算法（4 种 Legacy 密码 + `hmac-md5`）风险提示，且右栏仅单行密钥表下方留空 `600px`；`账户余额` 顶部状态词裸浮无圆点徽标、中部 `1100px` 空白断层、流水 `余额变化` 用除号 `/` 替代箭头 `→` | [CryptoSuiteSelect.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/crypto/CryptoSuiteSelect.vue#L1-L121)、[cryptoCatalog.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/crypto/cryptoCatalog.ts#L209-L254)、[transport-encryption.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/tenants/%5Bcode%5D/tabs/transport-encryption.vue#L31-L435)、[account.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/tenants/%5Bcode%5D/tabs/account.vue#L20-L95) |

---

## 2. 第一性原理本质归因

1. **轻量身份原语组合优于重型壳继承（Composition over God-Shell，公理 U3）**：租户（8-Tab 运营空间）、产品（3-Tab 属性/路由/租户绑定）、供应商（原地读写表单 + 工作台入口）的主体拓扑各异；跨实体真正恒定的契约是**顶栏实体身份条 `<EntityIdentityBar>`**（名称 + 可复制编码 + `<StatusIndicator>` + 右侧 `32px` 简洁动作按钮）。
2. **视觉原语双射与统计窗口诚实性（公理 U2 / U3）**：「启停/冻结运行态」唯一映射至圆点 `<StatusIndicator>`，「协议/计费分类枚举」唯一映射至 `<ElTag effect="plain">`；同屏异构时间窗指标（`24h` 实时流量质量 vs. `30d` 累计账务消费）必须各自诚实标注真实窗口，严禁强行合并右上角标签伪造统计口径。

---

## 3. 详尽可落地技术方案

### 3.1 抽离 `<EntityIdentityBar>`、统一 `<StatusIndicator>` 与补齐 `drafts.vue` 权限（`mazbri-web`）

1. **新建 `components/common/entity-identity-bar/index.vue` 并对齐三类详情顶栏（`W06-1`）**
   - Props：`name: string`、`code?: string`、`statusType?: StatusType`、`statusText?: string`；Slots：`tags`、`actions`。
   - 解除 [tenant-detail-shell/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/common/tenant-detail-shell/index.vue#L54) `.tenant-detail-shell__identity` 上的 `max-height: 28px` 限制（改为 `min-height: 32px`），并在租户详情、产品详情、供应商详情统一挂载 `<EntityIdentityBar>`：
     - **产品详情（保留完整 3-Tab `info` / `routes` / `tenants`）**：将 `zh.json` / `en.json` 首 Tab 词条更正为 `基本信息` / `Overview`，删除废弃词条 `advancedInfoTitle`；为 `内部ID` 挂载 `<CopyableText>`，移除描述表内重复编码行，为「修改」补齐 `ri:edit-line` 图标（与浅红描边「删除」视觉对称）。
     - **供应商详情**：顶栏展示名称、可复制 `supplier_code` 与 `<StatusIndicator>`，右侧动作按钮严格遵循 Clean-Craft 2–4 字规范排布 `[工作台] [编辑] [删除]`（编辑态切换为 `[取消] [保存]`）；移除卡片头与描述表内重复的启用标签，为 `ElDescriptions` 设置 `:label-width="168"` 与 `word-break: keep-all` 消除「密钥仓」断词。
2. **全站生命周期状态圆点化、产品列表补列与 `drafts.vue` 只读门控（`W06-2`）**
   - 将全站列表与抽屉中的启停/正常/冻结状态列统一收敛为 `<StatusIndicator>`：覆盖 `suppliers/index.vue`、`products/index.vue`、`control-panel/users/index.vue`、`UserDetailDrawer.vue`、`CertificateDetailDrawer.vue`、`BillListPanel.vue`、`FinanceOverviewPanel.vue`（余额表状态列）、`tenants/[code]/tabs/products.vue`、`SilencesPanel.vue`。
   - 在 [products/index.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/products/index.vue#L169-L235)：仅在 `canWrite` 为真时注入 `{ type: "selection", width: 48 }`；补入 `超时时间`（`numeric: true`，`{timeout_secs}s`）与 `高级熔断`（`<StatusIndicator>`）列，为 `enabled` 锁定 `width: 108`（消除 `726px` 留白断层），并在第 227 行 `getProductCellText` 同步补齐新列宽度测量分支。
   - **修复 [drafts.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/upstream-connectors/drafts.vue#L15-L196) RBAC 漏口**：引入 `canWriteBusiness`（`ROLES_BIZ`），将顶部 `<TableStandardActionBar>` 的 `:show-primary`、`:show-import`、`:show-delete`、表格 `selection` 列及行内 `删除` 按钮全部绑定 `canWriteBusiness`，确保 `R_VIEWER` 零越权写入口暴露。

### 3.2 租户详情 `概览 360`、`传输加密` 与 `账户余额` 深度精修（`W06-3`, `W06-4`）

1. **`概览 360` 语义图标、双时间窗诚实标注与错误占比双栏视图（[overview360.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/tenants/%5Bcode%5D/tabs/overview360.vue)）**
   - 为 5 张 `StatCard` 传入差异化图标（调用量 `ri:exchange-box-line`、成功率 `ri:checkbox-circle-line`、P95 `ri:timer-flash-line`、消费 `ri:money-cny-circle-line`、健康分 `ri:heart-pulse-line`），消费金额经 `formatMoney` 格式化并补齐 `元` / `CNY` 单位；为前 4 卡统一补齐紧凑副标题 `近 24 小时`、第 5 卡保留 `近 30 日`（日期区间），使 5 卡底部基线齐平且统计口径 100% 真实；在 `< 1360px` 视口下将 `.tenant-kpi-grid` 平滑降为 3 列防卡片内数字挤压。
   - 基于 Step 02 修复别名遮蔽后的真实失败数据集 `errors`：无错误记录时渲染紧凑正向空态 `近 24 小时无错误记录`；有错误记录时双栏展示左侧横向占比条图（`CategoryBarChart horizontal`）与右侧 `错误码 / 次数 / 占比` 右对齐明细表；清零 `api-keys.vue` 与 `whitelist.vue` 工具栏 `.el-form-item` 底部外边距。
2. **`传输加密` 显式模式切换、全量弱算法提示与双栏重心平衡（`CryptoSuiteSelect.vue` & `transport-encryption.vue`）**
   - 在 [CryptoSuiteSelect.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/crypto/CryptoSuiteSelect.vue) 顶部引入 `<ElSegmented v-model="selectionMode">`（`推荐套件` / `自定义算法`）：
     - 独立维护组件内 `selectionMode`：外部载入初始值时若 `!modelValue.suite && modelValue.dataAlg` 则初始化为 `'custom'`，否则为 `'suite'`；用户手动切至 `'custom'` 时不触发回跳 `'suite'` 的死循环，切回 `'suite'` 时显式回填默认套件并清空 `dataAlg` / `macAlg`；
     - 查询 [cryptoCatalog.ts](file:///workspace/rust-proj/mazbri/mazbri-web/src/components/crypto/cryptoCatalog.ts#L209-L254) 的 `DATA_ALGS` 与 `MAC_ALGS` 元数据，当选中的加密算法或 MAC 算法带有 `legacy: true`（覆盖 `aes-256-ecb`、`aes-128-ecb`、`sm4-ecb-pkcs7`、`des-cbc-pkcs5` 及 **`hmac-md5`** 共 5 项）时，紧随展示合规风险提示条。
   - 保持 `transport-encryption.vue` 外层 `<SettingsCardGrid>` 既有 `[不可更改]` 的 `1fr : 1fr` 等宽双栏（确保右栏 6 列 `808px` 传输密钥表不被压窄），在右栏密钥管理卡片下方新增只读「生效接入契约摘要」卡片（汇总当前加密模式、生效算法三元组、激活密钥版本、签名与时间窗参数），自然填补右下留白。
3. **`账户余额` 四栏紧凑概览与流向箭头（[account.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/tenants/%5Bcode%5D/tabs/account.vue)）**
   - 将顶部 `账户概览` 收敛为水平对齐的 4 栏指标块（`现金余额` | `授信 / 可用额度` | `账户状态` `<StatusIndicator>` | `近 30 日消费`）+ 右侧 `[充值]` `[退款]` 按钮；下方交易表 `余额变化` 分隔符由 `' / '` 改为 `' → '` 并启用 `numeric: true` 右对齐等宽。

---

## 4. 二阶、三阶影响与边界守卫

| 边界场景 | 连锁风险分析 | 消解与防护设计 |
| :--- | :--- | :--- |
| **`CryptoSuiteSelect` 用户点击切换「自定义算法」瞬间的状态稳定性** | 若用响应式 `computed` 直接按 `Boolean(dataAlg)` 派生模式，用户刚点「自定义」尚未选算法时会被瞬间弹回「推荐套件」。 | `selectionMode` 采用本地 `ref`，仅在外部 prop 数据源发生实质身份切换时同步初始化，手动切模式保持稳定停留。 |
| **`transport-encryption` 右侧 6 列密钥表在小屏下的可读性** | 密钥表含固定右侧 `230px` 操作列，缩窄右栏比例会挤压指纹与版本列。 | 保持桌面 `1fr : 1fr` 等宽、`< 1200px` 自动上下单栏堆叠，靠右下契约摘要卡平衡高度而非压缩右栏宽度。 |

---

## 5. 验收门禁与实测清单

1. **自动化单测必测项**
   - 新增 `CryptoSuiteSelect` 模式状态机单测：覆盖套件态初始化、存量 `des-cbc-pkcs5` 自定义态初始化、手动切换 `'custom'` 不回跳及 5 种 `legacy: true` 算法风险判定。
2. **构建、热替换与 `9224` 实机核验**
   - 执行 `mazbri-web` `pnpm build && pnpm test` 与 `mazbri-bin -p mazbri-mgmt` 门禁，原子热替换重启 `mazbri-mazbri-mgmt-1`。
   - 连接 `127.0.0.1:9224` 验证：① 租户、产品（完整 3-Tab，首 Tab 为 `基本信息`）、供应商详情顶栏 `<EntityIdentityBar>` 统一对齐，供应商详情无孤字断行与重复标签；② 以 `R_VIEWER` 访问 `/#/operations/upstream-supply?tab=drafts` 确认无新建/导入/删除写控件且控制台零 `403`；③ 核验租户 `概览 360`、`传输加密`、`账户余额` 双分辨率与明暗双主题排版。
