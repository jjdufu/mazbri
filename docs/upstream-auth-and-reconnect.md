# 上游认证失效与断线重连

本文说明 bypass（以及 mgmt 试运行引擎）在上游**令牌失效**和**WebSocket 连接失效**时的处理机制、配置方式、观测手段与计费口径。示例中的地址、凭证均为占位符，不含任何真实密钥。

## 1. 机制概览

一次上游调用最多经历「首次尝试 + 一次内部重试」，重试发生在 bypass 引擎内部，对网关和租户透明：

| 场景 | 判定 | 动作 |
|---|---|---|
| 认证失效 | 响应命中「认证失效判定」规则（默认 HTTP 401） | 作废**本次使用的**缓存令牌（比较后作废，不误删并发刷新出的新令牌）→ 重新换票 → 同一请求重试一次 |
| WS 陈旧连接 | 复用的空闲连接在发送或等待回复前被对端关闭 | 丢弃该连接 → 新建连接重试一次；不作废令牌 |
| 借出前探活 | `ws_liveness_check_on_checkout=true` 时对空闲连接做非阻塞检查 | 已关闭的连接直接丢弃，计入 `stage=checkout` |

约束：

- 每个请求最多一次认证重试；重试后仍被拒绝即返回 `UPSTREAM_AUTH_REJECTED`，不会循环换票。
- 认证重试与 WS 陈旧连接重试受**重试预算**（`retry_budget_*`）约束，预算耗尽时跳过重试并返回错误，避免上游整体失效时放大流量。预算**按供应商分桶**（对应 Envoy 按 cluster 的 retry budget）：一个上游重启只消耗它自己的额度，不影响其它上游同时恢复。
- 令牌缓存有效期优先取供应商配置的 `expiry_jsonpath`；未配置时读取换票响应的 `expires_in`；两者都没有时使用 `token_default_ttl_secs`。配置了 `expiry_jsonpath` 但响应中缺失该字段时**不会**回退到 `expires_in`。缓存在到期前 `token_refresh_skew_secs` 视为过期。

## 2. 认证失效判定规则

规则（`invalidation`）可以声明在两处，运行时按 **渠道 > 供应商 > 默认** 取第一个存在的：

- 渠道（接口）级：`egress.auth.invalidation`
- 供应商级：`auth_flow.invalidation`
- 默认：`http_status = [401]`

字段：

| 字段 | 含义 | 限制 |
|---|---|---|
| `http_status` | HTTP 状态码列表 | 400–599，不可重复；显式写 `[]` 表示不按状态码判定 |
| `jsonrpc_codes` | JSON-RPC `error.code` 列表 | 仅 `jsonrpc` 适配器；不可重复 |
| `body_match` | `{ path, equals[] }` 列表：JSONPath 取值与任一期望值严格相等 | `path` 必须是合法 JSONPath 且不能是 `$`；`equals` 不能为空 |
| `ws_close_before_reply` | WS **新建**连接在回复前被关闭视为令牌失效 | 仅 `ws_unary` 适配器 |

`auth.profile = none` 时不允许声明规则。保存时 mgmt 会做同样的校验，前端表单（供应商「认证」区与工作台「鉴权」页的「认证失效判定」分组）实时提示错误。

示例一：JSON-RPC 上游以 `-32001` 表示令牌过期（供应商级）：

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

示例二：HTTP 200 但响应体业务码表示过期（渠道级）：

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

示例三：WS 上游在令牌失效时直接关闭新连接：

```json
{ "invalidation": { "http_status": [], "ws_close_before_reply": true } }
```

## 3. 运行参数

bypass 读取 `[upstream]`，mgmt 试运行引擎读取 `[connector_probe.runtime]`，字段同名同义：

| 键 | 默认 | 说明 |
|---|---|---|
| `token_default_ttl_secs` | 3600 | 换票响应无过期字段时的缓存秒数 |
| `token_refresh_skew_secs` | 30 | 提前过期余量，必须小于上一项 |
| `auth_retry_enabled` | true | 认证失效后作废令牌、换票并重试一次 |
| `ws_stale_retry_enabled` | true | 复用连接回复前断开时换新连接重试一次 |
| `ws_liveness_check_on_checkout` | true | 借出空闲连接前非阻塞探活 |
| `retry_budget_ratio` | 0.2 | 窗口内重试数 / 首次请求数上限，取值 (0, 1] |
| `retry_budget_min_per_window` | 3 | 低流量时窗口内保底允许的重试次数 |
| `retry_budget_window_secs` | 10 | 预算统计窗口 |
| `ws_pool_idle_ttl_secs` 等 | 见配置模板 | WS 连接池参数（原有） |

示例：

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

bypass 指标：

| 指标 | 标签 | 含义 |
|---|---|---|
| `bypass_upstream_auth_rejected_total` | `adapter`, `trigger`（`http_status` / `jsonrpc_code` / `body_match` / `ws_close`） | 命中认证失效判定 |
| `bypass_upstream_auth_retry_total` | `adapter`, `result`（`ok` / `rejected_again` / `failed` / `skipped_disabled` / `skipped_budget`） | 认证重试结果 |
| `bypass_auth_token_evict_total` | `result` | 令牌作废 |
| `bypass_ws_pool_stale_total` | `stage`（`checkout` / `send`） | 发现陈旧连接 |
| `bypass_ws_stale_retry_total` | `result` | 陈旧连接重试结果 |
| `bypass_retry_budget_exhausted_total` | `kind`（`auth` / `ws_stale`） | 预算耗尽跳过重试 |

网关：`gateway_retry_total{reason="upstream_internal"}` 按 bypass 透传的内部重试次数计数；访问日志 `retry_count = (渠道尝试序号 - 1) + internal_retries`；认证失效最终失败时 `error_class=upstream_auth`。

日志事件（`target` 为 runtime 引擎）：`upstream.auth_rejected`（含 8 位十六进制 `token_fp` 令牌指纹，不输出令牌本身）、`upstream.auth_retry`、`upstream.ws_stale_retry`；预算耗尽告警每 10 秒最多一条。

proto `InvokeResponse` 新增 `internal_retries`（6）与 `retry_reasons`（7，如 `auth_rejected:jsonrpc_code`、`stale_connection`）。工作台试运行结果栏在发生内部重试时显示「内部重试」次数和原因，阶段详情中重试轮阶段名带 `#retry` 后缀。

排障路径：

1. 租户收到 502 / `E5021` → 查网关访问日志 `error_class=upstream_auth` 定位渠道。
2. 查 bypass `bypass_upstream_auth_rejected_total` 的 `trigger`，确认是哪条规则命中。
3. 看 `bypass_upstream_auth_retry_total{result}`：`rejected_again` 说明新令牌也被拒（凭证错误或上游故障）；`skipped_budget` 说明预算耗尽；`skipped_disabled` 说明开关关闭。
4. WS 场景看 `bypass_ws_pool_stale_total{stage}` 与 `bypass_ws_stale_retry_total`；`checkout` 计数上升通常意味着上游重启或空闲断连。
5. 在工作台对同一接口试运行，比对阶段详情中的首轮与 `#retry` 轮。

## 5. 计费口径

当前实现采用**方案 A**：重试后仍认证失效 → 网关返回 **502**，错误码 `E5021`（`UPSTREAM_AUTH_REJECTED`），消息已脱敏；该请求**不计费**，按 `switch_on_bad_gateway` 决定是否切换渠道，并计入熔断失败。内部重试成功的请求按正常成功请求计费一次，不因内部重试重复计费。

可选替代方案（未实现，供评估）：

- **方案 B**：透传上游的 401/403 给租户并不计费——语义更贴近上游，但会让租户误以为是自身凭证问题。
- **方案 C**：按「上游已产生调用」计费——适合上游按次收费的场景，需要额外的对账口径。

## 6. 已知限制

- 每个请求最多一次内部重试；不在单请求内做多轮换票。
- 网关渠道开启 `switch_on_bad_gateway` 且候选渠道全部失败时，租户看到的是通用的 502 / `E5020`（「All available channels exhausted」），`E5021` 只出现在不切换渠道的场景；访问日志 `error_class` 与网关日志仍能看到 `UPSTREAM_AUTH_REJECTED`。
- 非幂等请求同样会被重试：认证失效与回复前断线通常意味着上游未处理请求，但无法绝对保证。
- 失败路径（bypass 返回失败）的交互事件与失败访问日志不含 `internal_retries`；`upstream.auth_rejected` 日志不含是否已作废字段。
- `ws_close_before_reply` 只对新建连接生效；复用连接被关闭一律按陈旧连接处理。
- 借出前探活为非阻塞检查，无法发现「半开」连接；这类连接在发送失败后由陈旧连接重试兜底。
- 规则、运行参数变更后需 bypass / mgmt 重新加载配置；proto 新字段要求 gateway、bypass、mgmt 同批发布与回滚。
