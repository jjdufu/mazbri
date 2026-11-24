# 上游认证失效与断线重连机制

`mazbri-bypass`（及 `mazbri-mgmt` 试运行引擎）通过共享的 `mazbri-upstream-runtime` 自动处理外部上游**令牌失效**与 **WebSocket 陈旧连接断开**。示例中的地址与凭证均为占位符。

## 1. 机制概览

单次上游调用最多经历「首次尝试 + 一次内部重试」，重试在运行时内部完成，对网关与租户透明：

| 场景 | 触发判定 | 内部动作 |
|---|---|---|
| 认证失效 | 响应命中认证失效规则（默认 HTTP `401`） | 对比并作废本次使用的缓存令牌（不误删并发刷新的新令牌）→ 重新换票 → 重放原请求一次 |
| WS 陈旧连接 | 复用的空闲连接在发送或等待回复前被对端关闭 | 丢弃该连接 → 新建连接重放一次（不作废令牌） |
| 借出前探活 | `ws_liveness_check_on_checkout = true` | 借出空闲连接前执行非阻塞检查，已关闭连接直接丢弃并计入 `stage=checkout` |

核心约束：

- 每个请求最多执行 1 次认证重试；重试后仍被拒绝则返回 `UPSTREAM_AUTH_REJECTED`，禁止循环换票。
- 认证重试与 WS 陈旧连接重试共用按「供应商 + 渠道 + 鉴权指纹」分桶的**重试预算**（`retry_budget_*`）。预算仅由每一代令牌的**首个被拒请求**扣除并触发作废；并发请求检测到旧令牌已被换新时直接复用新令牌重放，不重复消耗预算。
- 令牌缓存 TTL 优先级：供应商配置 `expiry_jsonpath` > 换票响应 `expires_in` > `token_default_ttl_secs`（显式配置了 `expiry_jsonpath` 但响应缺失该字段时不回退 `expires_in`）。距离到期不足 `token_refresh_skew_secs` 时提前视为过期。

## 2. 认证失效判定规则

规则（`invalidation`）按 **渠道级 > 供应商级 > 默认值** 顺序取首个生效项：

- 渠道（接口）级：`egress.auth.invalidation`
- 供应商级：`auth_flow.invalidation`
- 默认值：`http_status = [401]`

| 字段 | 语义 | 校验约束 |
|---|---|---|
| `http_status` | 视为令牌失效的 HTTP 状态码列表 | 取值 `400–599`，不可重复；显式传 `[]` 表示不按状态码判定 |
| `jsonrpc_codes` | JSON-RPC `error.code` 列表 | 仅 `jsonrpc` 适配器可用；不可重复 |
| `body_match` | `{ path, equals[] }` JSONPath 精确匹配列表 | `path` 须为合法 JSONPath 且不为 `$`；`equals` 非空 |
| `ws_close_before_reply` | 新建 WS 连接在收到回复前被关闭视为令牌失效 | 仅 `ws_unary` 适配器可用 |

`auth.profile = none` 时禁止配置 `invalidation` 规则。

示例 1——供应商级 JSON-RPC 错误码判定：

```json
{
  "auth_flow": {
    "enabled": true,
    "profile": "gettoken",
    "token_url": "https://upstream.example.com/token",
    "token_jsonpath": "$.token",
    "invalidation": { "http_status": [401], "jsonrpc_codes": [-32001] }
  }
}
```

示例 2——渠道级 HTTP 200 业务码判定：

```json
{
  "egress": {
    "auth": {
      "profile": "oauth_cc_form",
      "invalidation": {
        "http_status": [401, 403],
        "body_match": [{ "path": "$.code", "equals": ["TOKEN_EXPIRED", 40101] }]
      }
    }
  }
}
```

示例 3——WS 新连接断连判定：

```json
{ "invalidation": { "http_status": [], "ws_close_before_reply": true } }
```

## 3. 运行时配置参数

`mazbri-bypass` 读取 `[upstream]` 段，`mazbri-mgmt` 读取 `[connector_probe.runtime]` 段，字段同名同义：

| 配置键 | 默认值 | 说明 |
|---|---|---|
| `token_default_ttl_secs` | `3600` | 换票响应无过期字段时的缓存秒数 |
| `token_refresh_skew_secs` | `30` | 提前过期余量秒数（须小于 `token_default_ttl_secs`） |
| `auth_retry_enabled` | `true` | 认证失效后作废令牌、换票并重试一次 |
| `ws_stale_retry_enabled` | `true` | 复用 WS 连接断开时换新连接重试一次 |
| `ws_liveness_check_on_checkout` | `true` | 借出空闲 WS 连接前执行非阻塞探活 |
| `retry_budget_ratio` | `0.2` | 统计窗口内「重试数 / 首次请求数」上限，取值 `(0, 1]` |
| `retry_budget_min_per_window` | `3` | 低流量时窗口内保底允许的重试次数 |
| `retry_budget_window_secs` | `10` | 重试预算滚动窗口秒数 |

```toml
[upstream]
auth_retry_enabled = true
ws_stale_retry_enabled = true
ws_liveness_check_on_checkout = true
retry_budget_ratio = 0.2
retry_budget_min_per_window = 3
retry_budget_window_secs = 10
token_default_ttl_secs = 3600
token_refresh_skew_secs = 30
```

## 4. 指标、日志与排障

| Prometheus 指标 | 标签 | 含义 |
|---|---|---|
| `bypass_upstream_auth_rejected_total` | `adapter`, `trigger`（`http_status` / `jsonrpc_code` / `body_match` / `ws_close`） | 命中认证失效判定次数 |
| `bypass_upstream_auth_retry_total` | `adapter`, `result`（`ok` / `rejected_again` / `failed` / `skipped_disabled` / `skipped_budget`） | 认证重试结果计数 |
| `bypass_auth_token_evict_total` | `result` | 缓存令牌作废次数 |
| `bypass_ws_pool_stale_total` | `stage`（`checkout` / `send`） | 检测到陈旧 WS 连接次数 |
| `bypass_ws_stale_retry_total` | `result` | WS 陈旧连接重试结果计数 |
| `bypass_retry_budget_exhausted_total` | `kind`（`auth` / `ws_stale`） | 重试预算耗尽跳过次数 |

链路透传与日志关联：

- Protobuf `InvokeResponse` 通过 `internal_retries`（字段 6）与 `retry_reasons`（字段 7，如 `auth_rejected:jsonrpc_code`、`stale_connection`）将内部重试明细回传网关。
- 网关累加 `gateway_retry_total{reason="upstream_internal"}`，并在请求访问日志中记录合并后的 `retry_reasons` 与 `retry_count`；认证重试最终失败时标记 `error_class=upstream_auth`。
- 运行时事件日志包含 `upstream.auth_rejected`（仅输出 8 位十六进制令牌指纹 `token_fp`）、`upstream.auth_retry`、`upstream.ws_stale_retry`；试运行阶段详情中重试轮阶段名带 `#retry` 后缀。

排障步骤：

1. 租户收到 `502` / `E5021` 时，检索网关访问日志 `error_class=upstream_auth` 定位具体渠道。
2. 查看 `bypass_upstream_auth_rejected_total{trigger}` 确认触发判定的规则类型。
3. 查看 `bypass_upstream_auth_retry_total{result}`：`rejected_again` 表示新换取的令牌仍被拒（凭据配置错误或上游鉴权服务故障）；`skipped_budget` 表示触发重试熔断保护。
4. 在免代码工作台对目标接口执行试运行，比对首轮阶段与 `#retry` 阶段的请求/响应摘要。

## 5. 计费与错误码口径

- **内部重试成功**：按单次正常成功请求计费，不因内部换票重放重复计费。
- **重试后仍认证失效（或跳过重试）**：网关返回 HTTP `502`（未开启渠道切换时错误码为 `E5021`，开启 `switch_on_bad_gateway` 且全部候选渠道耗尽时为 `E5020`），该次请求**不计费**，并计入产品熔断失败统计。

## 注意事项

- 非幂等请求命中认证失效或回复前断连时同样会触发单次重放；若上游在返回 `401` 前已产生副作用，需在渠道侧关闭重试或由上游保证幂等。
- 借出前非阻塞探活无法识别 TCP 半开连接，半开连接在发送失败时由 `ws_stale_retry` 兜底重连。
- 修改失效规则或 `[upstream]` 参数后需重载对应服务配置。
