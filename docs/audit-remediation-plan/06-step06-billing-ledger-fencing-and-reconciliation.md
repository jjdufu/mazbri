# Step 06：分布式账务精度同构、单调租约围栏与全链路对账单一事实源

## 1. 检测出的问题清单与源码定位

| 编号 | 级别 | 问题描述 | 精确源码位置 |
| :--- | :--- | :--- | :--- |
| **S06-1** | Critical | `mazbri-billing-service` RPC 金额转换三重错账：`from_amount` 误写 `100i64.pow(decimals)`（`decimals=2` 时除以 `10,000`，且 `decimals >= 10` 溢出 Panic）；`to_amount` 硬编码 `decimals=2` 将默认 `0.001` 元毫元单价截断为 `0`；`check_balance` 漏做单位换算将预估成本放大 | [conversion.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-billing-service/src/tarpc_service/conversion.rs#L42-L66)、[handlers.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-billing-service/src/tarpc_service/handlers.rs#L124-L137) |
| **S06-2** | Critical | 异步扣费队列 `usage_charge_queue` 重复扣费：慢批次超时被 `reclaim_stale_processing` 重置后，`deduct_and_record(_batch)` 不校验队列行租约与 `usage_records.charge_status`，`ledger_entries.interaction_event_id` 恒写 `NULL` 无唯一索引防重；且 `mark_failed` 无 `status = 'processing'` 与租约条件，会覆盖已完成状态触发二次扣费（`usage_outbox_worker` 同理） | [queue.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-billing/src/queue.rs#L141-L290)、[transactions.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-billing/src/repository/transactions.rs#L320-L369)、[usage_outbox_worker.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-billing/src/usage_outbox_worker.rs#L405-L469) |
| **S06-3** | High | 数据库与存储过程精度失配：`tenant_accounts.balance`/`credit_limit`/`available_balance`、`transactions.balance_before/after` 以及 PL/pgSQL 函数 `process_recharge`/`process_refund` 局部变量均为 `DECIMAL(18,4)`，而 `usage_records`/`ledger_entries` 为 `DECIMAL(18,6)`，5~6 位微额扣款与充值前后流水被截断破坏会计恒等式；`TenantProductConfig` 未拦负单价，`create_adjustment` 缺正数与下限校验，且残留无人挂载的死中间件 `mazbri_billing::middleware` | [init.sql](file:///workspace/rust-proj/mazbri-max/init-dbs/src/sql/schemas/billing/init.sql#L7-L15)、[init.sql](file:///workspace/rust-proj/mazbri-max/init-dbs/src/sql/schemas/billing/init.sql#L270-L355)、[model.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-product/src/model.rs#L187-L215)、[middleware.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-billing/src/middleware.rs#L1-L120) |
| **S06-4** | High | 网关 `BalancePrecheckCache` 每笔请求双重扣减（请求前 `reserve` + 响应后 `optimistic_subtract`，且非计费响应不退回预占），同时 `SET EX` 无限续期 Redis TTL、充值或调账不失效缓存，导致 `>0.2 QPS` 正常有余额租户永不回源而被持续误拒 `402` | [balance.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-gateway/src/gateway/balance.rs#L312-L320)、[balance_cache.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-gateway/src/balance_cache.rs#L71-L165)、[post_response.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-gateway/src/gateway/post_response.rs#L200-L215) |
| **S06-5** | High | 计费、出账、经营物化视图与对账五方口径分裂：网关 `post_response.rs` 与共享契约 `mazbri-rpc/src/http_upstream.rs` 将 `400/401/403/422` 归入 `SUCCESS_WITH_DATA` 真实扣款，`generate_bill` 只计 `response_status < 400`，`snapshot_customer_bill_items` 与物化视图 `mv_daily_revenue_cost` 又未按 `charge_status = 'charged'` 过滤，且账期使用闭区间 `[start, end]`；管理面对账右边界写死 `23:59:59.000` 漏算 999ms、忽略 `scope` 且混入非计费流量；L3 对账器无 `'running'` 超时回收 | [post_response.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-gateway/src/gateway/post_response.rs#L504-L513)、[http_upstream.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-rpc/src/http_upstream.rs#L47-L57)、[init.sql](file:///workspace/rust-proj/mazbri-max/init-dbs/src/sql/schemas/billing/init.sql#L554-L586)、[generate.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/billing/generate.rs#L35-L107)、[jobs.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/billing/jobs.rs#L215-L299)、[reconcile.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-billing/src/reconcile.rs#L201-L296) |
| **S06-6** | Medium | `mazbri-billing-service` 的 `record_usage` RPC 在事务外插入 `event_id = None` 的 `usage_records`，重试导致重复计费且 `worker.enabled = false` 时形成无人消费死队列 | [charge.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-billing-service/src/tarpc_service/charge.rs#L43-L157)、[server.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-billing-service/src/server.rs#L60-L97) |

---

## 2. 第一性原理本质归因

本步对应 **公理 A7（分布式账本单一事实源与单调围栏公理）**：
1. **精度同构不变量**：会计恒等式 `账户余额变动 == 交易流水前后差 == 分录金额 == 用量单价 × 次数` 成立的充要条件是全链路（定价校验、网关试算、tarpc 协议、队列载荷、分录表、余额主表、生成列与 PL/pgSQL 存储函数）采用一致的定点数标度 `DECIMAL(18, 6)`（微元，`10^-6`）。
2. **异步消费的 Lamport 单调围栏（Fencing Token）**：存在超时重领的分布式队列必须由数据库在每次认领时递增 `lease_seq`，并在最终扣款事务内以 `WHERE id = $1 AND status = 'processing' AND lease_seq = $2` 作为原子前置条件，辅以分录表对业务事件 ID 的唯一索引物理兜底，实现精确一次（Exactly-Once）入账。
3. **单一可计费谓词（Single Billability Predicate）**：一笔调用是否计费只能在产生事件的源头判定一次并固化为 `charge_status = 'charged' AND billable_amount > 0`；下游客户账单头、账单明细、经营看板物化视图与跨库对账只能聚合该固化事实，严禁各自按 HTTP 状态码二次猜测。

---

## 3. 详尽可落地解决方案

### 3.1 全链路定点数精度统一（`DECIMAL(18, 6)`）与入参强校验

1. **计费库基线 Schema 与存储函数同构更新（`mazbri-max/init-dbs`）**
   - 直接更新 [init.sql](file:///workspace/rust-proj/mazbri-max/init-dbs/src/sql/schemas/billing/init.sql)：
     - 将 `tenant_accounts` 的 `balance`、`credit_limit`、`available_balance`（`GENERATED ALWAYS AS (balance + credit_limit) STORED`）以及 `transactions` 的 `balance_before`、`balance_after` 全部定义为 `DECIMAL(18, 6)`；
     - 将 PL/pgSQL 存储函数 `process_recharge` 与 `process_refund` 内部的局部变量 `v_balance_before`、`v_balance_after` 及返回列精度同步提升为 `DECIMAL(18, 6)`，消除人工充值或退款时将账户已有微元小数位截断的隐患；
     - 在 `ledger_entries` 上定义部分唯一索引：
       ```sql
       CREATE UNIQUE INDEX uq_ledger_entries_charge_event
       ON mazbri_billing.ledger_entries (interaction_event_id)
       WHERE entry_type = 'CHARGE' AND interaction_event_id IS NOT NULL;
       ```
2. **明确区分内部 RPC 微元单位与管理端充值分单位，修复 `conversion.rs`**
   - **内部 tarpc 协议（微元 `decimals = 6`）**：在 [conversion.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-billing-service/src/tarpc_service/conversion.rs) 与 [handlers.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-billing-service/src/tarpc_service/handlers.rs) 中：
     - `from_amount` 改用底数 **`10`**：`10i128.checked_pow(u32::try_from(amount.decimals)?)`（限制 `0 <= decimals <= 6`，非法值返回 `Err(RpcError::Validation)`）；
     - `to_amount` 统一输出 `decimals = 6` 微元整数（`0.001` 元精确编码为 `value = 1000, decimals = 6`），移除静默归零的 `safe_to_amount`；
     - `check_balance` 统一按微元 `Decimal::new(estimated_cost_micros, 6)` 换算。
   - **管理端人工充退接口边界（人民币分 `10^-2`）**：[mod.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/handlers/product/mod.rs) 与前端 [account.vue](file:///workspace/rust-proj/mazbri/mazbri-web/src/views/mazbri/tenants/[code]/tabs/account.vue) 的 `RechargeRequest.amount` / `RefundRequest.amount` 保持整数“分”（`1 元 = 100 分`）不变，在 `mgmt` 仓储层经 `Decimal::new(amount_cents, 2)` 精确传入 `DECIMAL(18, 6)` 的 `process_recharge` / `process_refund`。
   - **修复 `record_usage` RPC 并删除死中间件**：要求必填非空 `event_id`（UUID），在单事务内幂等写入 `usage_records` 并入队或（当 `worker.enabled == false` 时）直接调用 `deduct_and_record` 同步扣费；彻底删除无人挂载的死文件 [middleware.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-billing/src/middleware.rs) 及其导出。
3. **产品定价与调账防负值门禁**
   - [model.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-product/src/model.rs) `TenantProductConfig::validate()`：强制校验 `unit_price` 无前后空白、`price >= Decimal::ZERO && price.scale() <= 6 && price <= Decimal::from(100_000)` 且 `currency == "CNY"`。
   - `BillingRepository::debit_balance` 与 `create_adjustment`：硬断言扣费 `amount >= Decimal::ZERO`；调账强制校验 `amount > Decimal::ZERO`，方向使用强枚举 `AdjustmentDirection::{Credit, Debit}`，借方调账校验调整后余额不低于 `-credit_limit`（超管冲正除外）。

### 3.2 异步扣费队列单调租约围栏（Exactly-Once 入账）

1. **队列围栏列与单事务原子完结（`usage_charge_queue` & `usage_outbox`）**
   - 在 `billing/init.sql` 中为 `usage_charge_queue` 与 `usage_outbox` 定义 `lease_seq BIGINT NOT NULL DEFAULT 0` 列。
   - `fetch_pending` 认领任务时执行 `SET status = 'processing', worker_node = $1, locked_at = NOW(), lease_seq = lease_seq + 1 ... RETURNING ..., lease_seq`。
   - 在 [transactions.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-billing/src/repository/transactions.rs) 的 `deduct_and_record` 与 `deduct_and_record_batch` **同一个 PostgreSQL 事务**内合并队列完结与余额扣减：
     1. 先执行围栏抢占：`UPDATE usage_charge_queue SET status = 'completed', processed_at = NOW() WHERE id = $id AND status = 'processing' AND lease_seq = $seq`；
     2. 仅对 `rows_affected == 1` 的条目累加扣款金额、更新 `usage_records`（带 `AND charge_status != 'charged'`）并向 `ledger_entries` 写入非空 `interaction_event_id`（取 `event_id`）；已被新租约 Worker 处理的条目自动跳过。
   - `mark_failed`、退避重试与死信转移全部强制附加 `WHERE id = $id AND status = 'processing' AND lease_seq = $seq`（`UsageOutboxWorker` 同步实施相同 `lease_seq` 围栏）。

### 3.3 重构网关 `BalancePrecheckCache`（单次预占 + 原子保 TTL + 单租户定向失效）

1. **消除双扣与非计费漏退**
   - 在 [balance.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-gateway/src/gateway/balance.rs) 与 [post_response.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-gateway/src/gateway/post_response.rs) 统一采用“请求前预占 `reserve(estimated)` ➔ 响应后差额结算 `settle(estimated, actual_billable)`”：
     - 非计费响应（`actual_billable == 0`，含上游 `4xx`/`5xx`、缓存或幂等重放）立即调用 `release(estimated)` 归还预占额度；
     - 计费响应且 `actual_billable == estimated` 时预占已精确扣除，响应后零额外扣减；仅在存在差额时调整 `actual_billable - estimated`。
2. **Lua 原子扣减保 TTL 与单租户定向 Pub/Sub 失效**
   - L2 Redis 余额键由回源路径以 `SET key <micros> EX 10` 初始化；网关预占与归还通过原子 Lua 脚本在键存在时执行 `DECRBY` / `INCRBY`（保持原有 10s TTL 不被续期，确保每 10 秒强制回源一次 PG 权威余额）。
   - 当 L1/L2 预检判定余额不足即将返回 `402` 且距上次 L3 回源超过 `1s` 时，触发带单飞锁的 L3 查库复核，确认真实欠费才返回 `402`。
   - 在 `mazbri-mgmt` 完成账户充值、退款、调账或授信额度变更后，立即执行 `DEL {prefix}:billing:bal:{tenant_id}` 并向轻量级 Redis Pub/Sub 频道 `{prefix}:billing:bal_inv` 发布该 `tenant_id`，各网关副本订阅后**仅定向剔除本地 L1 中该租户的单条余额记录**，避免全局广播清空其余正常租户缓存。

### 3.4 统一计费、出账、经营物化视图与对账口径（SSOT）

1. **修正网关与 `mazbri-rpc` 的 `4xx` 计费映射**
   - 将 [post_response.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-gateway/src/gateway/post_response.rs) 的 `billing_code_for_status` 默认通配分支及 [http_upstream.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-rpc/src/http_upstream.rs#L47-L57) `HttpUpstreamResponse::ok` 中未细分 `4xx` 的映射统一修正为已有的 `mazbri_rpc::code::BillingCode::BAD_REQUEST`（`1000`，`default_billable = false`）；仅 `200..=299` 且非 `from_cbpro_fallback` 默认计费。
2. **统一客户账单、经营物化视图与对账 SQL 谓词及左闭右开时间窗 `[start, end)`**
   - `generate_bill`、`snapshot_customer_bill_items`（同事务原子写入，并在 `billing/init.sql` 为 `(tenant_id, period_start, period_end) WHERE status != 'cancelled'` 建立部分唯一索引）、`get_bill_items`、`get_usage_summary`：统一过滤 **`charge_status = 'charged' AND billable_amount > 0`**，时间窗统一为 **`recorded_at >= $start AND recorded_at < $end`**。
   - 同步修正 [init.sql](file:///workspace/rust-proj/mazbri-max/init-dbs/src/sql/schemas/billing/init.sql#L554-L586) 中的日收支物化视图 `mv_daily_revenue_cost`：在聚合 `usage_records` 客户收入分支追加 `WHERE charge_status = 'charged' AND billable_amount > 0`，确保财务经营总览（`finance_ops.rs`）与客户账单金额 100% 吻合。
   - 修复管理面对账（[jobs.rs](file:///workspace/rust-proj/mazbri/mazbri-bin/mazbri-mgmt/src/services/billing/jobs.rs)）：`reconciliation_day_bounds(date)` 右边界改为次日 `00:00:00.000000 UTC` 配合 `< $end`，ClickHouse 网关侧汇总附加 `AND billable = 1`，并按入参 `scope_type` / `scope_id` 精确下推过滤条件。
   - 修复 L3 对账器（[reconcile.rs](file:///workspace/rust-proj/mazbri/mazbri-libs/mazbri-billing/src/reconcile.rs)）：为 `billing_reconcile_runs` 增加 15 分钟超时 `'running'` 回收；正向对账关联检查 `usage_outbox`，将已处于 `dead_letter` 的记录归类为待处置死信而非管道漏单，并校验 `clickhouse_table` 标识符白名单。

---

## 4. 二阶与三阶影响深度推演

| 影响阶次 | 潜在连锁反应场景 | 架构消解与防护设计 |
| :--- | :--- | :--- |
| **二阶影响 1：只改表列精度而遗漏 PL/pgSQL `process_recharge` / `process_refund` 局部变量导致微元截断** | 若 `tenant_accounts.balance` 存有 `10.123456` 元，管理员在后台充值 `1.00` 元时，若存储函数内部 `v_balance_before DECIMAL(18,4)` 未同步扩宽，`transactions` 流水将写入截断后的 `10.1235`，破坏审计流水闭合。 | 在 `billing/init.sql` 同一变更中同步将表列、生成列与全部 PL/pgSQL 函数局部变量统一为 `DECIMAL(18, 6)`，通过单测断言含 6 位小数余额下的充退流水零精度损失。 |
| **二阶影响 2：外部非标上游以 HTTP `400` 承载有效业务核验结果时的计费方式** | 将网关默认 `4xx` 修正为不向租户收费后，个别把“要素不一致”放在 HTTP `400` 返回的外部上游若需向租户计费，不能再靠默认通配漏扣。 | 网关层保持清晰的协议语义（HTTP `4xx` 默认不计费）；对非标外部上游通过连接器工作台 `ChannelEgressSpec` 响应映射规则将其显式转换为平台标准 `200` 业务结果，由通道配置显式声明计费意图。 |
| **三阶影响：采用单租户定向 Pub/Sub 失效替代全局缓存清空的扩展性收益** | 高频充值场景下若触发全局缓存版本递增，会导致全站数千租户的网关 L1 余额缓存同时失效击穿 `mazbri-billing-service`。 | 定向 `DEL` + 携带 `tenant_id` 的轻量 Pub/Sub 消息将失效范围严格收敛在单个变动租户内，其余租户热路径保持零抖动。 |

---

## 5. 验收门禁与实测清单

1. **账务不变量与并发回归测试**
   - 验证 `to_amount` 与 `from_amount` 对 `0.000001`、`0.001`、`100.00`、`99999999.999999` 往返转换恒等无损；验证管理台充值 `10000` 分（`100.00` 元）到已有余额 `0.000123` 元的账户后，`balance == 100.000123` 且 `transactions.balance_before == 0.000123`、`balance_after == 100.000123`。
   - 并发围栏测试：模拟 Worker A 认领队列项（`lease_seq = 1`）后挂起，触发 `reclaim_stale_processing` 由 Worker B 认领（`lease_seq = 2`）完成扣款，随后恢复 Worker A 继续执行扣款与 `mark_failed`：断言 `tenant_accounts.balance` 仅扣减 1 次，`ledger_entries` 仅 1 条记录，队列终态保持 `'completed'`。
   - 连续发送 200 笔 `0.000040` 元计费请求与 50 笔 `400` 异常请求，刷新 `mv_daily_revenue_cost` 并执行出账与对账：断言 `bills.total_amount == SUM(bill_items.total) == mv_daily_revenue_cost.revenue == 0.008000` 元，三源对账差异数为 `0`。
   - 模拟租户以 `5 QPS` 持续请求 25 秒（跨越两个 10s TTL），断言余额充足时全程零误报 `402`；余额耗尽触发 `402` 后在管理台充值，断言 1 秒内该租户请求恢复 `200`。
2. **部署与验证顺序**
   - 更新 `init-dbs` (`billing/init.sql`) 并编译 `mazbri-billing-service`、`mazbri-gateway`、`mazbri-mgmt`，执行 `redep rerun` 完成账务全链路闭环验证。
