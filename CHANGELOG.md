# Changelog

## [0.3.9](https://github.com/jjdufu/mazbri/compare/v0.3.8...v0.3.9) (2026-11-09)


### Bug Fixes

* **mgmt,web:** 统一全站布局与表格工具栏规范、搜索筛选及 MFA 账户安全体验 ([#67](https://github.com/jjdufu/mazbri/issues/67)) ([0cc205e](https://github.com/jjdufu/mazbri/commit/0cc205ecc3c1ce636b16e8457bd2993f18a2172e))

## [0.3.8](https://github.com/jjdufu/mazbri/compare/v0.3.7...v0.3.8) (2026-11-07)


### Features

* **ops,web:** 运维中心与公共外壳规范化重构及真分页治理 ([#65](https://github.com/jjdufu/mazbri/issues/65)) ([2efc5f1](https://github.com/jjdufu/mazbri/commit/2efc5f1880bff97350c7ca6b3956eaf44eb67bd6))

## [0.3.7](https://github.com/jjdufu/mazbri/compare/v0.3.6...v0.3.7) (2026-11-07)


### Bug Fixes

* **web:** 精简管理端文案并修正运维页面说明展示 ([d5d276f](https://github.com/jjdufu/mazbri/commit/d5d276f72cfe8e61775bb5947f325392481a2b90))

## [0.3.6](https://github.com/jjdufu/mazbri/compare/v0.3.5...v0.3.6) (2026-11-06)


### Features

* **web:** 运维控制台按新信息架构发版 ([#61](https://github.com/jjdufu/mazbri/issues/61)) ([c772ef4](https://github.com/jjdufu/mazbri/commit/c772ef4871b7e86596d28677741c2f3970c332f9))

## [0.3.5](https://github.com/jjdufu/mazbri/compare/v0.3.4...v0.3.5) (2026-11-06)


### Features

* **crypto:** 增加 SM2 预计算表启动预热 ([0952be8](https://github.com/jjdufu/mazbri/commit/0952be8d4665e648733ecfbe3eb5db11dc41723f))


### Bug Fixes

* **gateway:** ready 前预热 SM2 并将请求绑定验签移出 tokio worker ([be919c9](https://github.com/jjdufu/mazbri/commit/be919c920b6ed1ed5671c1da49926218195cc685))
* **gateway:** SM2 冷启动预热并将验签移出 tokio worker ([d71fbf4](https://github.com/jjdufu/mazbri/commit/d71fbf4b0cb0ba2375b0bc4daeefb1078180964a))

## [0.3.4](https://github.com/jjdufu/mazbri/compare/v0.3.3...v0.3.4) (2026-11-06)


### Bug Fixes

* **billing:** 用量事件改为有界队列加后台批量写入，响应路径不再同步 produce ([9bf006c](https://github.com/jjdufu/mazbri/commit/9bf006c985b361b6368e08762d7abc5e55e860a4))
* **cache:** resolve_config 单元测试不再连接本地 Redis ([63b8d25](https://github.com/jjdufu/mazbri/commit/63b8d2503d58470a007d8661afc244e2fa0985f7))
* **gateway,cache:** 缓存关闭时跳过 RPC，配置查询加内存缓存和短超时 ([600d35d](https://github.com/jjdufu/mazbri/commit/600d35dde747cfb40029546ae874b7279ff95558))
* **observability:** upstream_ms 改为纯上游耗时并补充分段字段与真实 span ([7a77d0b](https://github.com/jjdufu/mazbri/commit/7a77d0bf56477759cde94d4b9bf026d853543fc7))
* **product,gateway:** 用 cache_configs 填充产品快照 cache_enabled ([6373cae](https://github.com/jjdufu/mazbri/commit/6373cae5ec628acf87761e63c17a918353ce91c7))
* **tls:** 内部 tarpc 连接在握手前开启 TCP_NODELAY ([26a7df8](https://github.com/jjdufu/mazbri/commit/26a7df892e6167a4951ce26d85955deff1a839d2))
* 降低内部 tarpc 时延并按 cache_configs 填充 cache_enabled ([3b6501b](https://github.com/jjdufu/mazbri/commit/3b6501bee0ed41774a5e43b9b5c063309f66af01))

## [0.3.3](https://github.com/jjdufu/mazbri/compare/v0.3.2...v0.3.3) (2026-11-05)


### Features

* **analytics:** 总览改为显式时间窗 + 统一组件健康清单 ([e69a737](https://github.com/jjdufu/mazbri/commit/e69a73702e485f22be5520ad57d96ca9fa41699d))
* **mgmt:** 总览/异常按时间窗透传，产品名录与平台规模替换旧接口 ([adadb01](https://github.com/jjdufu/mazbri/commit/adadb0125cef7ec3752effe4a36386cefd8be68e))
* **web:** 共享卡片组件与 zh/en 文案统一 ([0d22090](https://github.com/jjdufu/mazbri/commit/0d220908c954646c5a7c621f925d5690437b66a4))
* **web:** 总览数据层对齐新接口（显式时间窗、组件健康、产品名录） ([68ae67c](https://github.com/jjdufu/mazbri/commit/68ae67c92999ef05a111487d1f30a709f9f50fe7))
* **web:** 按方案 B 重做运营总览（全局筛选栏 + 四个分区） ([829972e](https://github.com/jjdufu/mazbri/commit/829972e6f7d0fb36e927d90478b314b0d58c7f87))
* **web:** 运营总览方案 B 重做、统一组件健康清单与并排卡片等高 ([2b295f8](https://github.com/jjdufu/mazbri/commit/2b295f82de4b5ccb17c97170c547055dce3fd383))


### Bug Fixes

* **analytics:** 产品排行按时间窗选分钟/小时聚合，仅返回当前窗有调用的产品 ([75b2ec1](https://github.com/jjdufu/mazbri/commit/75b2ec175ec87b470f2ea59ab3de02ab699a9353))
* **web:** 大屏复用总览口径与组件健康清单 ([8fb6e1a](https://github.com/jjdufu/mazbri/commit/8fb6e1a1fdbb418105a3c47c5b33cd785c831f4f))
* **web:** 并排卡片统一等高，消除短卡下方空白 ([3365a77](https://github.com/jjdufu/mazbri/commit/3365a77fc73fab751982a8b95010c529d45f1c4a))
* **web:** 证书详情状态说明、链路详情右栏改为粘性侧栏 ([c58d200](https://github.com/jjdufu/mazbri/commit/c58d2006e3db12b124851f5fe36bfc69a339a878))

## [0.3.2](https://github.com/jjdufu/mazbri/compare/v0.3.1...v0.3.2) (2026-11-05)


### Features

* **mgmt:** 账号安全 RBAC 收敛与控制面板设置页统一双列布局 ([c4e0218](https://github.com/jjdufu/mazbri/commit/c4e02181b1ae4f43e3af9aad3de8070f2bfa040d))


### Bug Fixes

* **mgmt,web:** 账号安全页 RBAC 收敛与控制面板入口 ([9ce1b70](https://github.com/jjdufu/mazbri/commit/9ce1b70760c243025a454b454f929812f7a92c88))
* **mgmt,web:** 隐藏 MFA 步进免认证时长展示 ([ebfeb04](https://github.com/jjdufu/mazbri/commit/ebfeb04d5d82689a7ea1d19fe042c6642bac7d6a))
* **web:** 合并「最大登录尝试」与「锁定阈值」，只保留生效的 password_policy 设置 ([831b6e8](https://github.com/jjdufu/mazbri/commit/831b6e8f681064b7bc05f0a4f635be17a2096eef))
* **web:** 类别状态只保留已验证/未验证 ([d8ceb29](https://github.com/jjdufu/mazbri/commit/d8ceb296efbd149e6953044d77200b267ec4a606))
* **web:** 账号安全页去掉 MFA 步进类别面板 ([65fcf68](https://github.com/jjdufu/mazbri/commit/65fcf682f0bf12acca8c536129eac36d6fb03884))
* **web:** 账号安全页按方案 A 全页单列重构 ([b1f4754](https://github.com/jjdufu/mazbri/commit/b1f475470c9f33f69b32925c13ee8c1f5dfbe8a0))

## [0.3.1](https://github.com/jjdufu/mazbri/compare/v0.3.0...v0.3.1) (2026-11-05)


### Bug Fixes

* **web:** 上游接入成功页去掉网关 curl 示例 ([41d7dd2](https://github.com/jjdufu/mazbri/commit/41d7dd2bd6c9e3d4487326cb54a1858875024193))
* **web:** 上游接入成功页去掉网关 curl 示例 ([f1305b6](https://github.com/jjdufu/mazbri/commit/f1305b67a432c3e0f6582f90a1d11733493e7ea1))

## [0.3.0](https://github.com/jjdufu/mazbri/compare/v0.2.20...v0.3.0) (2026-11-04)


### ⚠ BREAKING CHANGES

* **workbench:** 工作台 business/live 试调改走 mgmt→bypass 隔离管理探针 HTTP（默认 :8890 + 共享 mgmt_probe_token），不经 gateway 计费、不给 mgmt 发 mTLS 客户端证；require_gate 只认 probe_path=bypass_mgmt_probe。同分支另含 job_poll submit-only、ws_unary 连接池硬并发等破坏性变更，见 CHANGELOG Unreleased。

### Bug Fixes

* **upstream-runtime:** http_envelope 加密接线测补齐 wrap/unwrap ([5f59d55](https://github.com/jjdufu/mazbri/commit/5f59d55d921cded3213ad23e947a63b0709b960b))
* **upstream-runtime:** http_envelope 测 unwrap 对齐 capture 响应 $.v ([4826d2b](https://github.com/jjdufu/mazbri/commit/4826d2b939cee98d5068cedd15fd454372357419))
* **web:** drafts 导入/导出/删除角色展开为可变数组以过 TS ([0a3493f](https://github.com/jjdufu/mazbri/commit/0a3493f7e056ffcea1e1a762bf696a3cfe2a7df4))


### Code Refactoring

* **workbench:** 试调改走 bypass 隔离管理口+共享 token（破坏性） ([987aed4](https://github.com/jjdufu/mazbri/commit/987aed4fe31bee10760bc6a41a137a022e4da05f))

## [Unreleased]

### ⚠ BREAKING CHANGES

* **workbench/probe:** 工作台 business/live 试调改走 **mgmt→bypass 隔离管理探针 HTTP**（默认 `:8890`，共享 `mgmt_probe_token`；**不**经 gateway 计费，**不**给 mgmt 发 mTLS 客户端证）。租户数据面仍为 bypass `:8888` mTLS。`require_gate` **只认** `probe_path=bypass_mgmt_probe`；历史 `RuntimeEngine` / 旧 `bypass_grpc` 记录**作废**。删除默认 Send 走 `build_connector_runtime`（engine 仅留 SSRF/`auth_only`/`transform_preview`/microservice，**probe_path=NULL 永不进 gate**）。无双门控开关。须同步 init-dbs 门控索引/COMMENT；升级后须用新路径重跑门控。

* **upstream/ws_unary:** 废除每 invoke 一次 `connect_async→一帧→close` 作为唯一路径；引入按键复用连接池 + 全局/每键硬并发（`ws_max_concurrent` / `ws_max_concurrent_per_key` / `ws_pool_max_idle_per_key` / `ws_pool_idle_ttl_secs`）。超限**失败快**，禁止无限排队与「池满回退短连」。Binary 帧仍严格失败。旧配置缺字段走 default，但行为已变（有池）。适合中低 QPS；高 QPS 请用 HTTP 适配器。

* **upstream/job_poll:** invoke 改为 submit-only（或请求级一次性 `job_poll_mode=query`），**禁止**在网关/bypass 内 sleep+轮询至终态。响应默认 `{"job_id":...}`；终态由租户轮询/回调完成。删除 Spec 字段 `poll_interval_ms`/`status_jsonpath`/`result_jsonpath`/`message_jsonpath` 与 bypass `job_poll_min_interval_ms`（`deny_unknown_fields`，旧配置需迁移）。无同步兼容开关。


## [0.2.20](https://github.com/jjdufu/mazbri/compare/v0.2.19...v0.2.20) (2026-11-04)


### Bug Fixes

* **analytics:** 运维观测 PromQL/Overview/追踪服务与删容量任务 ([16501d6](https://github.com/jjdufu/mazbri/commit/16501d688c6bd68e77b98db80740b1d61014398b))
* **mgmt:** overview 字段门控保留 analytics 的 range/instant 窗口语义 ([7da6787](https://github.com/jjdufu/mazbri/commit/7da6787512fc99446982ec2541b159ce3f1fcf81))
* **mgmt:** Overview 跟窗透传、追踪服务列表、去掉可靠性压测/容量 API ([5519030](https://github.com/jjdufu/mazbri/commit/551903086471e31cf1d215546422420609d258aa))
* **ops-ui:** 运维观测 PromQL/Overview/追踪服务与删可靠性页 ([c6ced88](https://github.com/jjdufu/mazbri/commit/c6ced88d5e018faf8e8888064df0be16bb2cf7aa))
* **web:** 删可靠性页；Dashboard「此刻」+ 跟窗 KPI；追踪服务下拉 ([eb58a7e](https://github.com/jjdufu/mazbri/commit/eb58a7ee3d8f5f1ea6d7c5ea2848242ac2ccbeaf))
* **web:** 补回误删的 opsRoutes 导出并修好可靠性路由残留语法 ([9d8d3c6](https://github.com/jjdufu/mazbri/commit/9d8d3c6372d61edc2048d67a831261743425588f))

## [0.2.19](https://github.com/jjdufu/mazbri/compare/v0.2.18...v0.2.19) (2026-11-04)


### Features

* **crypto/core/sdk:** 请求绑定支持非对称签名（rsa-v1/ecdsa-v1/ed25519-v1/sm2-v1） ([00a40a7](https://github.com/jjdufu/mazbri/commit/00a40a722735b09dfb16a4bf62ec81ecd5ed453b))
* **gateway/mgmt:** 请求绑定非对称签名落地（gateway 校验 + mgmt 公钥注册） ([5c1f749](https://github.com/jjdufu/mazbri/commit/5c1f749569a87057a044ae347401060e96b1d5f6))
* improve-0.2.18（analytics 503、租户 uuid、非对称请求绑定、协议规范） ([ab3fdbc](https://github.com/jjdufu/mazbri/commit/ab3fdbc398c4fe76635aff444c09d642f1d1913e))
* **mgmt:** 租户响应返回 uuid，产品列表支持按渠道编码筛选 ([1b68fc7](https://github.com/jjdufu/mazbri/commit/1b68fc7e3dbc358a7808418d5a244f000cd56776))
* **protocol:** 对外租户接入协议规范 + 固定测试向量 + 多语言示例；传输层 legacy 分派、mgmt 读接口副作用修复 ([70b6ba3](https://github.com/jjdufu/mazbri/commit/70b6ba365ea9e57d9580d8f3d299a06fe1886734))


### Bug Fixes

* **mgmt:** mgmt 调 analytics 的错误统一为 503/502/4xx 透传，去掉假空数据兜底 ([f420086](https://github.com/jjdufu/mazbri/commit/f420086d5b16809ccbbde304b2cc0c95b9a062a3))

## [0.2.18](https://github.com/jjdufu/mazbri/compare/v0.2.17...v0.2.18) (2026-11-03)


### Bug Fixes

* **crypto:** 租户密钥按数据算法取前缀（SM4 用 16B） ([e8c8ac9](https://github.com/jjdufu/mazbri/commit/e8c8ac95ec591aff7ff77bcf656f09de40b1bad6))
* **upstream:** DES-CBC 走 legacy 实现；OAuth 表单模板凭据 URL 编码 ([3feffc4](https://github.com/jjdufu/mazbri/commit/3feffc46e03023154d6c337de358161df6dcebab))
* **upstream:** GET 负载摊进 query；SignOnly 对 BIZ 字段做 raw 签 ([049ebf0](https://github.com/jjdufu/mazbri/commit/049ebf0d1fc3e5f9521479fb1d944fe5be06505b))
* **upstream:** 上游对接通用修复（digest-as-sign/encrypt_only、sorted_kv 推导、GET→query、DES-CBC/OAuth 编码、SM4 密钥前缀） ([c4a54bc](https://github.com/jjdufu/mazbri/commit/c4a54bc714dc9f9e9f2f7df33aa40efdce123b05))
* **upstream:** 允许 allow_insecure 下 digest-as-sign 与 encrypt_only 裸分组密码 ([9a2fa37](https://github.com/jjdufu/mazbri/commit/9a2fa37ecedd2e5702d5df1dd32e94f4500f2f44))

## [0.2.17](https://github.com/jjdufu/mazbri/compare/v0.2.16...v0.2.17) (2026-11-03)


### Features

* **mgmt:** i18n 与展示格式统一（名称·小数·状态）（step12） ([b0750ac](https://github.com/jjdufu/mazbri/commit/b0750acdb62c6453bf578ea6b9b23373e2ca1a13))
* **mgmt:** 传输加密页权限与许可证态一致（step10） ([07eb2cc](https://github.com/jjdufu/mazbri/commit/07eb2cce531f3792d659ac9d0b3ec9b4186fca9c))
* **mgmt:** 列表分页完整性与禁止假全量拉取（step05） ([fc90e28](https://github.com/jjdufu/mazbri/commit/fc90e284008482ec0c5771c2257b0ff8229c97b9))
* **mgmt:** 前端 chunk 失效硬刷新提示（step13） ([52452c3](https://github.com/jjdufu/mazbri/commit/52452c3433def7ff4463d5a997098fe2e667bd17))
* **mgmt:** 告警种子过滤与时间墙钟化（step09） ([240a182](https://github.com/jjdufu/mazbri/commit/240a1825d37af9a6dd9d4b8221b2d3d911f60b2b))
* **mgmt:** 审计噪音治理——个人偏好退出安全审计（step08） ([08037ea](https://github.com/jjdufu/mazbri/commit/08037ea0b7233e2c46dfc5ca569c0e4532bd5bec))
* **mgmt:** 对齐 RBAC 四层，去掉假 actions 与静默回首页 ([23b00ca](https://github.com/jjdufu/mazbri/commit/23b00ca542a7c4cbbba1232104db4229d69a5757))
* **mgmt:** 布局设计系统与信息架构重整（step11） ([2d4d416](https://github.com/jjdufu/mazbri/commit/2d4d416b51bb7fe457033f30397cc3a0d4be11f8))
* **mgmt:** 管理后台破坏性改造（RBAC/金额SSOT/语义/分页/布局等 step02–13） ([622b535](https://github.com/jjdufu/mazbri/commit/622b535fc4788a6c79ea79d22d773824a793f4ae))
* **mgmt:** 经营金额 SSOT（step03）— cost_status/window/租户名，禁假 100% 毛利 ([1c098c9](https://github.com/jjdufu/mazbri/commit/1c098c9f5e4b8d02afbc2b54b71932032f26f962))
* **mgmt:** 计费漏斗真实转化与账单生命周期空态（step07） ([af96bf1](https://github.com/jjdufu/mazbri/commit/af96bf1233dfa0a49db036faf8d0e9e947c4578e))
* **mgmt:** 调用/错误语义 SSOT 与瞬时 KPI 空态（step04） ([5c84bf4](https://github.com/jjdufu/mazbri/commit/5c84bf401d5a71c6344cb59befd1ff192e14b7b4))
* **mgmt:** 运维图表/TopN/时间窗统一（step06） ([622e141](https://github.com/jjdufu/mazbri/commit/622e141fdbeccf8869f690472444f856bfb8d94d))

## [0.2.16](https://github.com/jjdufu/mazbri/compare/v0.2.15...v0.2.16) (2026-11-03)


### Features

* **crypto:** step-03 非对称/国密/遗留原语落地 ([a69c014](https://github.com/jjdufu/mazbri/commit/a69c014fea145eff2e06cb7117814154e161d198))
* **crypto:** step-04 落地 TenantTransport/Upstream Profile 与命名套件 ([065a1f4](https://github.com/jjdufu/mazbri/commit/065a1f45dd01cf4be24a5f6be726ccf8d9fd6570))
* **crypto:** step-06 破坏性改写 core 传输/信封/租户密钥走 mazbri-crypto ([2f8cdcd](https://github.com/jjdufu/mazbri/commit/2f8cdcd79dd0e071dd86156c30a00cb0be7f88d3))
* **crypto:** step-07 Gateway/SDK/OpenAPI 按 Profile 破坏性封包 ([af496e0](https://github.com/jjdufu/mazbri/commit/af496e0e7dd45d33126c612cc4315fcb9a627cde))
* **crypto:** step-08 mgmt/租户传输 UI 对齐 suite 与闭集算法 ([0c312bf](https://github.com/jjdufu/mazbri/commit/0c312bfefd890d88eda62945ea96cab5c4f20d27))
* **crypto:** step-09 ChannelEgressSpec.crypto 与校验（删除 HMAC-only） ([a2e34ee](https://github.com/jjdufu/mazbri/commit/a2e34ee0e45bc4f85395b42a85d61ffac8f0ef93))
* **crypto:** 实现对称/AEAD 与摘要/MAC 原语及向量测试（step-02） ([cc46235](https://github.com/jjdufu/mazbri/commit/cc462353e2d14202fefa680f3fe279896127d309))
* **crypto:** 平台软件加解密闭集与 Profile 专项（step-01～15） ([919fb48](https://github.com/jjdufu/mazbri/commit/919fb48d34d83237b59b506a53664d8da0f0af42))
* **crypto:** 新增 mazbri-crypto 骨架与算法注册表（step-01） ([7d8f929](https://github.com/jjdufu/mazbri/commit/7d8f929faff70baba7b08c52726208d7bd6c0f7a))


### Bug Fixes

* **web:** workbench 补齐 isSuperAdmin，修复 CryptoProfileEditor 类型检查 ([f91f092](https://github.com/jjdufu/mazbri/commit/f91f092f98d3a24d020e781a9deb8a56210025b7))

## [0.2.15](https://github.com/jjdufu/mazbri/compare/v0.2.14...v0.2.15) (2026-11-03)


### Features

* **account:** step-18 平台安全策略嵌进账号安全页 ([7fd269c](https://github.com/jjdufu/mazbri/commit/7fd269c3065627c2df9aa8571ce1cfb1705392e0))
* **account:** step-19 删除控制台安全设置路由并完善账号安全摘要 ([2c098b6](https://github.com/jjdufu/mazbri/commit/2c098b688f362e78ced825cb3e132c43d3a13e4e))
* **ops-ia:** step-01 落地运维 IA 共享前后端基建 ([7a18e2b](https://github.com/jjdufu/mazbri/commit/7a18e2bae05c0fde7e48c796ccc93dc39e15e95d))
* **ops-ia:** step-02 重写运维监控一级路由骨架 ([d9a4846](https://github.com/jjdufu/mazbri/commit/d9a48464edab949eedce59b50fa3bce53713dec6))
* **ops-ia:** step-03 合并基础设施为 /ops/infra 三 Tab ([a3498c5](https://github.com/jjdufu/mazbri/commit/a3498c57be63695149189af92c1b3b199311cdbf))
* **ops:** step-04 告警中心合并为 /ops/alerts 三 Tab ([08806ad](https://github.com/jjdufu/mazbri/commit/08806ad32044f471e49bf51d663b1f4c9e21bd48))
* **ops:** step-05 计费与对账合并为 /ops/billing 两 Tab ([288e0d0](https://github.com/jjdufu/mazbri/commit/288e0d0b255751ad1b2a9b14e0c571ae9efe9aab))
* **ops:** step-06 流量与质量合并为 /ops/traffic 三 Tab ([8fb601c](https://github.com/jjdufu/mazbri/commit/8fb601c5f81ce1f71a9a09b30ece1478f4dc26b2))
* **ops:** step-07 上游健康合并为 /ops/upstream 两 Tab ([1bcaa19](https://github.com/jjdufu/mazbri/commit/1bcaa19f087346659c6c0b37463f3f9a0aed9c2c))
* **ops:** step-08 流量治理合并为 /ops/flow 两 Tab ([6f5c3a7](https://github.com/jjdufu/mazbri/commit/6f5c3a76da784c158bb9b1d3be6ec7d07f936123))
* **ops:** step-09 服务概览合并为 /ops/services 两 Tab ([d7e8930](https://github.com/jjdufu/mazbri/commit/d7e89308f6c3e49add8c3f643aa4d104b603fe07))
* **ops:** step-10 可靠性仅容量；侧栏撤异常/SLO/产品/变更 ([bc90af4](https://github.com/jjdufu/mazbri/commit/bc90af43155fa1fdcb088d477943b5d1371c5953))
* **ops:** step-11 Dashboard 裁剪占位与低价值块 ([93b38c3](https://github.com/jjdufu/mazbri/commit/93b38c3c69bf3c0eb0bd5a625ac220d86568c6f7))
* **ops:** step-12 Dashboard 组件收缩与角色门控 ([a138f06](https://github.com/jjdufu/mazbri/commit/a138f0658c18bc70f4d206d00e07e4d002ecb13d))
* **ops:** step-13 Dashboard 并入产品 TopN；删除产品排行路由 ([4cac408](https://github.com/jjdufu/mazbri/commit/4cac4083e27574785ddabc56c484374b2ada8925))
* **ops:** step-14 Dashboard 并入近期异常；删除异常检测路由 ([909eebe](https://github.com/jjdufu/mazbri/commit/909eebe37658195085aa737ac522cec5cf88968a))
* **ops:** step-15 Dashboard 并入近 24h 变更；删除变更时间线路由 ([cfa6744](https://github.com/jjdufu/mazbri/commit/cfa674471175b179c11b331bd0e2888c6c84bfb9))
* **ops:** step-16 Dashboard 并入 SLO/证书徽章/底座红灯；删除 SLO 路由 ([49165f4](https://github.com/jjdufu/mazbri/commit/49165f4d2d2073413adeae157879d861e1ac24f8))
* **ops:** step-17 全库下钻/通知链统一到新 IA，清除兼容 redirect ([355b8f2](https://github.com/jjdufu/mazbri/commit/355b8f2ff8d052377e9c3fa1afbb9e6814e78194))
* **ops:** step-20 对齐 i18n/菜单文案并补齐大屏与总览互链 ([75c3c54](https://github.com/jjdufu/mazbri/commit/75c3c543da615d2dd0a2f4c20e43124c7040c782))
* **ops:** step-21 钉死 route_access 审计并统一 overview 瞬时时间窗 ([9a80440](https://github.com/jjdufu/mazbri/commit/9a80440d137bd5aa39214ba566273366482d9d3f))
* **ops:** 运维信息架构重构（侧栏收敛 / Tab 壳 / Dashboard 折叠） ([f227974](https://github.com/jjdufu/mazbri/commit/f227974038e091384e4d5a10304ad0ce687a3821))


### Bug Fixes

* **ci:** 补齐 Release 调用 ci.yml 的 actions/PR 读权限 ([3f8869b](https://github.com/jjdufu/mazbri/commit/3f8869bcc39d7e100383ea03c626d6d9705fec20))
* **ci:** 补齐 Release 调用 ci.yml 的 actions/PR 读权限 ([e8db9f6](https://github.com/jjdufu/mazbri/commit/e8db9f6da4320228e20848cc9a2e19ab7633958a))

## [0.2.14](https://github.com/jjdufu/mazbri/compare/v0.2.13...v0.2.14) (2026-11-02)


### Bug Fixes

* **web:** 静态公共路由跳过 validatePath，修复登录白屏 ([07874b5](https://github.com/jjdufu/mazbri/commit/07874b50c9b12ba47b205fd25f41190010ec68de))
* **web:** 静态公共路由跳过 validatePath，修复登录白屏 ([9f95aaa](https://github.com/jjdufu/mazbri/commit/9f95aaa4ca32aad237d7d14f5263096fd676c88c))

## [0.2.13](https://github.com/jjdufu/mazbri/compare/v0.2.12...v0.2.13) (2026-11-02)


### Features

* **mgmt:** 按角色收紧 stats/alerts 的 route_access ([4018f74](https://github.com/jjdufu/mazbri/commit/4018f746c453ee3e667d3a29a7033fe3b95b32a2))
* **ready:** 全服务上报 mazbri_rpc_ready 并与 /ready 同源 ([432c0a6](https://github.com/jjdufu/mazbri/commit/432c0a6ad2f40cad282a37a9c28354906dbcf68c))
* **web:** 侧栏运营优先并按角色裁剪运维菜单 ([c9b433e](https://github.com/jjdufu/mazbri/commit/c9b433e248656f752be337a078b9aa065e2c2e39))


### Bug Fixes

* **ci:** 补齐 traces 图表导入并拆分 admin 就绪检查 ([501c1d8](https://github.com/jjdufu/mazbri/commit/501c1d82169288ad319f9a1d8006ff0f076758b4))
* **mgmt-web:** 修复 OpsPage 首卡被纵向 flex 压扁截断 ([0c54f74](https://github.com/jjdufu/mazbri/commit/0c54f74016d321ffb03142bf5d22f1439924bc32))
* **mgmt-web:** 修复未登录 401 误报，并拆分运维监控与运营中心菜单 ([584eab9](https://github.com/jjdufu/mazbri/commit/584eab966deb35ff5e6740c7617a876cbd498cb3))
* **mgmt-web:** 修复运维页图表溢出与布局对齐 ([d0ad39a](https://github.com/jjdufu/mazbri/commit/d0ad39acc2cb47f76335e3c58017eb506cf43933))
* **mgmt-web:** 收尾运维页 Ops 壳与内联图表溢出 ([180ece5](https://github.com/jjdufu/mazbri/commit/180ece52fc9d9c41c436ba0361e59f32ef3c5dc5))
* **mgmt:** NotifyInput 反序列化强制要求 emails/webhooks 字段 ([b7e6825](https://github.com/jjdufu/mazbri/commit/b7e682520b8634a2c53b0427a15c4b2fe2aa2752))

## [0.2.12](https://github.com/jjdufu/mazbri/compare/v0.2.11...v0.2.12) (2026-11-01)


### Features

* **analytics:** 登记流控/缓存指标目录与配额、租户、响应缓存 OLAP 模板 ([7fd7d3e](https://github.com/jjdufu/mazbri/commit/7fd7d3e886299cc811e342c274b03e2c29d9f744))
* **mgmt:** 经营看板与对账日历 API，小时刷新收入成本物化视图 ([c86c5c7](https://github.com/jjdufu/mazbri/commit/c86c5c7962c26ceff6740b42f18145dd48b23c96))
* **ops:** 上线证书/License 页与变更时间线（步骤 22） ([0d4a3fb](https://github.com/jjdufu/mazbri/commit/0d4a3fb4211e68855a13f468123a6d14c6c98178))
* **ops:** 上线鉴权与安全事件页及 analytics 规则任务（步骤 25） ([5c2885f](https://github.com/jjdufu/mazbri/commit/5c2885f9f100eeaf38f2f5864df422cac79cce41))
* **ops:** 产品排行、容量预测与异常检测（步骤 26） ([1522cbc](https://github.com/jjdufu/mazbri/commit/1522cbc186f3bc16c2f50c649478cac3b2eff73d))
* **ops:** 埋点 gateway_quota_usage_ratio 并切换限流页与告警口径 ([de400c5](https://github.com/jjdufu/mazbri/commit/de400c5eeeebdfd5defd1727772926e5e26b36e3))
* **ops:** 步骤21 NOC 大屏与服务拓扑 ([f912536](https://github.com/jjdufu/mazbri/commit/f91253635b05c22e82ee2bcbc34463f709f5e5d6))
* **ops:** 步骤23 告警完善——静默/抑制/值班/通知渠道 ([5fc9e5b](https://github.com/jjdufu/mazbri/commit/5fc9e5b823d82f2085ac349847bc40d005327a4b))
* **ops:** 步骤24 SLO / 错误预算——定义、燃烧率与可靠性页 ([c72914e](https://github.com/jjdufu/mazbri/commit/c72914e7ab17e5614f70ab77a9251efe76da2b4d))
* **ops:** 运维控制台步骤 19–27 与后续修复正式合入 ([39c17d9](https://github.com/jjdufu/mazbri/commit/39c17d9ebfbd334e970300183e02847c8975bf5e))
* **ops:** 首页偏好与自定义看板（步骤 27） ([17ea75f](https://github.com/jjdufu/mazbri/commit/17ea75f08d1a04f3649a872ce205ac10c78ebfaa))
* **web:** 上线限流配额、缓存、对账中心、经营看板与租户 360 ([7416a58](https://github.com/jjdufu/mazbri/commit/7416a58f6de3ce59ed5496c8e8f6e0b1763ca412))
* 上线运维基础设施三页（管道/中间件/主机容器） ([689ca49](https://github.com/jjdufu/mazbri/commit/689ca49dc703594bedd75afc132fa694d8a80f4f))
* 登记运维基础设施指标目录与查询构造 ([967ca8a](https://github.com/jjdufu/mazbri/commit/967ca8a1df76c366f5663be0b5973039c5716f09))


### Bug Fixes

* **analytics:** CH 拓扑与时间过滤统一显式 UTC ([c23f270](https://github.com/jjdufu/mazbri/commit/c23f2700d83066df21c07dc0e3b0a647779759df))
* **analytics:** security_events OLAP 避免 formatDateTime 别名遮蔽 ts 列 ([a52a203](https://github.com/jjdufu/mazbri/commit/a52a203a0a3804c93d3f184edb1e4f2d4673f34c))
* **mgmt:** 为 pool_budget 文档中的 acquire_timeout 补反引号 ([0b3dcb2](https://github.com/jjdufu/mazbri/commit/0b3dcb28f904ff56971901c807b404ffa303ec36))
* **mgmt:** 修复 admin 登录 pool acquire 超时的根因 ([52ee7b9](https://github.com/jjdufu/mazbri/commit/52ee7b912279e379c69f17163341c115d9d28e93))
* **mgmt:** 登记运维可靠性与偏好 API 的路由访问控制 ([b28df83](https://github.com/jjdufu/mazbri/commit/b28df8387cb908cd31793b17da580874065459af))
* **ops:** 为 dev19-mock 注入限流 429 与响应缓存命中种子 ([7be3ece](https://github.com/jjdufu/mazbri/commit/7be3eced950fb5929fa9aee54389cea874463a04))
* **ops:** 修复基础设施页与调用拓扑的点测遗留问题 ([0caf729](https://github.com/jjdufu/mazbri/commit/0caf729325a31e28ac6ba88085c8f9395e4a6456))
* **web:** OLAP 响应经 unknown 再断言，消除 vue-tsc 类型重叠错误 ([0f0d7bf](https://github.com/jjdufu/mazbri/commit/0f0d7bf821b3b19c0436aa8410f494d047c29dba))
* **web:** 修复运维路由对象缺少闭合括号导致前端无法构建 ([4482d74](https://github.com/jjdufu/mazbri/commit/4482d7480474dfc9fc1c9be6f11a99089c9d02ee))

## [0.2.11](https://github.com/jjdufu/mazbri/compare/v0.2.10...v0.2.11) (2026-11-01)


### Features

* **alerting:** 邮件为默认告警通知渠道，评估轮次加 PG advisory lock（第 09 步跟进） ([b074355](https://github.com/jjdufu/mazbri/commit/b0743558c0695c76014114ec158ea82da9f86e15))
* **analytics,gateway,mgmt:** step 03 cost/stage columns and OLAP template API ([a907c01](https://github.com/jjdufu/mazbri/commit/a907c0104af00777f7cff579e1aabf7b74528701))
* **analytics:** 指标时序接口改为多序列返回，配合 Prometheus remote write 修复 P95/P99 ([1a8489b](https://github.com/jjdufu/mazbri/commit/1a8489b28fc34c8d62d3a1b7f5b10696db2a48d8))
* **metrics:** 11 个服务统一暴露 build_info、进程资源与 tokio 运行时指标（步骤 10） ([cc4ae48](https://github.com/jjdufu/mazbri/commit/cc4ae48f55650f464131e2ed172a506dab32e981))
* **metrics:** 采集 tarpc RPC 与依赖客户端 RED 指标（步骤 11） ([4e60d0a](https://github.com/jjdufu/mazbri/commit/4e60d0abce789967a67edd1db744c5bb081dfa58))
* **ops:** gateway RED, latency breakdown and error analysis pages (step 06) ([c274be2](https://github.com/jjdufu/mazbri/commit/c274be2ef7d4644ef9db5a7ba9f3cb8b15b2a72a))
* **ops:** global overview, service health matrix and detail (step 05) ([59699a5](https://github.com/jjdufu/mazbri/commit/59699a592cc8c432c5a1d6ab8e6fa79cff0d1898))
* **ops:** supplier SLA, breaker/retry and billing pipeline health pages (step 07) ([7f19884](https://github.com/jjdufu/mazbri/commit/7f198841d9e485c55da7d9a3a5671cbbfd48d27c))
* **ops:** 最小告警中心（第 09 步）——规则评估、事件生命周期、webhook 通知与管理台页面 ([42e639e](https://github.com/jjdufu/mazbri/commit/42e639e407ebbc64ed34785022d10b05d7d2d4b5))
* **ops:** 步骤 08 调用检索页（合并客户/上游调用监控，直方图 + 阶段 Tab） ([32a486d](https://github.com/jjdufu/mazbri/commit/32a486d95e28f19d3551396482115b5880226195))
* **ops:** 步骤 12 采集与业务埋点 ([c862a80](https://github.com/jjdufu/mazbri/commit/c862a8063c6fb007e4e3d6926943f68cf44d0dea))
* **otlp:** 全量采集、span 白名单、SpanLimits 与可观测的导出队列（步骤 17） ([939eb2c](https://github.com/jjdufu/mazbri/commit/939eb2c3f6f9bba6b382000d5cb3bf6238799c90))
* **ratelimit:** api_limits 只保留 {qps,burst} 对象格式 ([9652b88](https://github.com/jjdufu/mazbri/commit/9652b883c8764531e781564001ab10f82d4ab411))
* **trace:** W3C Trace Context 跨 HTTP / gRPC / tarpc 透传 ([677bbb4](https://github.com/jjdufu/mazbri/commit/677bbb40173de6f278d11dd7f415296fbc40bc09))
* **trace:** 关键路径 span 埋点与日志 trace_id/span_id 关联 ([2efb066](https://github.com/jjdufu/mazbri/commit/2efb066021e448023d1cc78f71b17fc0cd482381))
* **trace:** 控制台 Trace 检索、瀑布详情、服务依赖与各页跳转（步骤 18） ([6501414](https://github.com/jjdufu/mazbri/commit/650141471ff75b7e3e163f83f600b549c0ee70c3))
* **web:** role-based home, ops overview/screen shell, shared ops widgets (step 04) ([a180bc4](https://github.com/jjdufu/mazbri/commit/a180bc49a2a6c0bbd45ac4a759bda4d94d678abd))


### Bug Fixes

* **analytics:** reject non-unix-second timestamp filters with 400 ([15b0d26](https://github.com/jjdufu/mazbri/commit/15b0d2643860a0ba094c6105a2925f0b3cbd46bb))
* **gateway:** cbpro 预热改用 CBProServiceClient，移除误用的 HttpUpstream tarpc 池 ([cd86c3e](https://github.com/jjdufu/mazbri/commit/cd86c3e57d49a129065b57bd5f4218e69a9259da))
* **metrics:** 修正监控指标口径并按 event_target 过滤访问日志（步骤 02） ([7d2206b](https://github.com/jjdufu/mazbri/commit/7d2206b3a1f60ffa3c21b7dc9466b9800268f3ac))
* **metrics:** 未接入数据库的服务健康状态不再判为不健康 ([5c503ea](https://github.com/jjdufu/mazbri/commit/5c503eac3ddfd2fa58e31bedf989ae59912aa2db))
* **ops:** call-search trace links, time picker, histogram brush, deep-link aliases, unix input zone ([92b849b](https://github.com/jjdufu/mazbri/commit/92b849b094eb253bd8b95f452fc711a89af7ed17))
* **ops:** 调用检索参数、trace 计费流水数据源与总览 P99 曲线点击（步骤 18 补修） ([acf143a](https://github.com/jjdufu/mazbri/commit/acf143aae89d559b5c54eab80216b84271a48f1b))
* **otel:** export spans via Vector OTLP receiver (step 14) ([6408e20](https://github.com/jjdufu/mazbri/commit/6408e20ab70fca0ee2de53a77347991f218d4c1b))
* **step18c:** unix-second time filters; log gateway rejections to request_logs ([ddb5eee](https://github.com/jjdufu/mazbri/commit/ddb5eeef773727f0f809f821ed350c0a497cdde6))

## [0.2.10](https://github.com/jjdufu/mazbri/compare/v0.2.9...v0.2.10) (2026-10-29)


### Features

* **mfa:** 前端按六类提权拦截，并去掉顶栏倒计时 ([87a0b39](https://github.com/jjdufu/mazbri/commit/87a0b39dbd2eb96d42028c4d3e605355e5bd6290))
* **mfa:** 增加六类独立提权窗口的类型和 Redis 键 ([de90202](https://github.com/jjdufu/mazbri/commit/de902022d09e25e32452d7aa76359e7a2f3d230b))
* **mfa:** 把提权文案和审计展示改成六类各自计时 ([395d2ae](https://github.com/jjdufu/mazbri/commit/395d2ae16712493ce0b093d4717dabd80ea2ac2b))
* **mfa:** 敏感操作改为六类独立的 15 分钟窗口 ([99ac3ad](https://github.com/jjdufu/mazbri/commit/99ac3ad440f7744f66fd0cf1759140a475c5fb91))


### Bug Fixes

* **mfa:** 修六类提权验证里发现的缺口 ([9e835dd](https://github.com/jjdufu/mazbri/commit/9e835dda28a4205be431bb9546b7ac3dfeba82e2))
* **mfa:** 去掉已删除的全局 MFA 策略列依赖 ([c86a1e6](https://github.com/jjdufu/mazbri/commit/c86a1e6b14d883b958e57c394bed1f7e0765acc3))
* **mfa:** 绑定成功只启用开关，不再打开提权窗口 ([cfeabe2](https://github.com/jjdufu/mazbri/commit/cfeabe2fa92ff6323ef5f16e40dfa3a4b5af4edc))
* **mfa:** 补上 MfaStepUpDialog 的全局组件类型登记 ([7603731](https://github.com/jjdufu/mazbri/commit/7603731166a0bb7bd1d55d79378271b69690fa6e))

## [0.2.9](https://github.com/jjdufu/mazbri/compare/v0.2.8...v0.2.9) (2026-10-28)


### Bug Fixes

* **auth:** 阶段令牌写库失败可回滚，并隔离登录 401 ([89479db](https://github.com/jjdufu/mazbri/commit/89479dbb02c7623ab66396df06ae6c709e114766))
* **auth:** 阶段令牌写库失败可回滚，并隔离登录 401 ([43d47f2](https://github.com/jjdufu/mazbri/commit/43d47f2f32bc6536e21db22fd4d1e2603625f0b6))

## [0.2.8](https://github.com/jjdufu/mazbri/compare/v0.2.7...v0.2.8) (2026-10-28)


### Features

* **mfa:** 为提权窗口增加 bootstrap 时长并收紧锁定配置 ([4617b89](https://github.com/jjdufu/mazbri/commit/4617b89ac446a373081c5bf84f4298d6e3275851))
* **mfa:** 用会话提权窗口替换逐请求验证码 ([a4196d7](https://github.com/jjdufu/mazbri/commit/a4196d75e56ebd357b94cc510fbca6da6deb59d4))
* **mfa:** 用会话提权窗口替换逐请求验证码 ([4dea3d1](https://github.com/jjdufu/mazbri/commit/4dea3d143da9d2520e9bb48b928250b14856ef74))


### Bug Fixes

* **mfa:** 提权失败回滚窗口，启用关闭重签会话并补齐错误体 ([d4e7ac9](https://github.com/jjdufu/mazbri/commit/d4e7ac92c2537ca7d0a1c6a305af8e2ce0310b62))

## [0.2.7](https://github.com/jjdufu/mazbri/compare/v0.2.6...v0.2.7) (2026-10-27)


### Features

* **cert:** scoped service tokens, actor claims v2 (HMAC) and keyless recovery ([#20](https://github.com/jjdufu/mazbri/issues/20)) ([c000afa](https://github.com/jjdufu/mazbri/commit/c000afa9d92e9869ec1e9fa9c28234cd28a1bc9e))

## [0.2.6](https://github.com/jjdufu/mazbri/compare/v0.2.5...v0.2.6) (2026-10-26)


### Features

* **mgmt:** audit middleware, role model, MFA per-feature + unified cross-service audit ([#18](https://github.com/jjdufu/mazbri/issues/18)) ([99cee99](https://github.com/jjdufu/mazbri/commit/99cee9906ea3355c16c5258e6621f5d68ca88113))

## [0.2.5](https://github.com/jjdufu/mazbri/compare/v0.2.4...v0.2.5) (2026-10-26)


### Features

* **web:** flat sidebar, denser layout and live appearance preview ([#16](https://github.com/jjdufu/mazbri/issues/16)) ([98c0776](https://github.com/jjdufu/mazbri/commit/98c077688af361a0ea1be5984a0f8f0bb0c25658))

## [0.2.4](https://github.com/jjdufu/mazbri/compare/v0.2.3...v0.2.4) (2026-10-25)


### Features

* **ssrf:** add [ssrf].allow_private_network switch for bypass and mgmt ([#14](https://github.com/jjdufu/mazbri/issues/14)) ([88be850](https://github.com/jjdufu/mazbri/commit/88be85081f8ff950e6eae43772e77abef3ce28be))

## [0.2.3](https://github.com/jjdufu/mazbri/compare/v0.2.2...v0.2.3) (2026-10-24)


### Bug Fixes

* mTLS certs only for mTLS services; gateway /health decoupled from downstreams; probe 503 not logged as ERROR ([#11](https://github.com/jjdufu/mazbri/issues/11)) ([5e82d55](https://github.com/jjdufu/mazbri/commit/5e82d55adc2b59a076a5aa9c144289eb81b221bd))

## [0.2.2](https://github.com/jjdufu/mazbri/compare/v0.2.1...v0.2.2) (2026-10-24)


### Bug Fixes

* **analytics:** compute stats from request_logs FINAL, drop pre-aggregated MV reads ([#9](https://github.com/jjdufu/mazbri/issues/9)) ([ba2671f](https://github.com/jjdufu/mazbri/commit/ba2671f85f11dbbb0a5a8906ee6296a7ae40594f))

## [0.2.1](https://github.com/jjdufu/mazbri/compare/v0.2.0...v0.2.1) (2026-10-24)


### Features

* billing batch charge + gateway charge retry, L1/L3 reconciliation, perf fixes, ClickHouse log dedup ([#7](https://github.com/jjdufu/mazbri/issues/7)) ([1b88aa7](https://github.com/jjdufu/mazbri/commit/1b88aa7120a04d90a5b3b190f8610c9dba6dc4a6))

## [0.2.0](https://github.com/jjdufu/mazbri/compare/v0.1.0...v0.2.0) (2026-10-23)


### ⚠ BREAKING CHANGES

* remove Fury/payload codec (JSON only); delete execute/transform, billing_type, tenant status enum (→ enabled), BinaryEncoding; required egress fields and api:* scopes; strict supplier parsing; audit q-only + fixed export; reconciliation gateway-only; idempotency fail-closed on Redis error (503); deny_unknown_fields on owned inputs; pedantic lints clean with no allows.

### Features

* remove Fury codec and legacy compat layers ([#5](https://github.com/jjdufu/mazbri/issues/5)) ([2e25409](https://github.com/jjdufu/mazbri/commit/2e254099db8cbb3755b425bcc2ad253efc5535ae))

## [0.1.0](https://github.com/jjdufu/mazbri/compare/v0.0.1...v0.1.0) (2026-10-22)


### ⚠ BREAKING CHANGES

* service names are now prefixed (mgmt -> mazbri-mgmt, incl. HKDF context; admin DB must be rebuilt); RUNTIME_ENV, bootstrap port/ip, registration.port, presets, DatabaseConfig.url, unused tls fields and cbpro-ext `[clients.gateway]` are removed; every service requires `[cert_service] url` and configs reject unknown fields.

### Features

* unified service identity, single bootstrap parse, workspace pedantic lints ([#3](https://github.com/jjdufu/mazbri/issues/3)) ([31a19cb](https://github.com/jjdufu/mazbri/commit/31a19cb10db636f02a2f19375033cb9dc08c99c8))

## 0.0.1 (2026-10-20)


### Bug Fixes

* switch to path deps, clean strict clippy warnings, fix frontend type errors ([aed2808](https://github.com/jjdufu/mazbri/commit/aed28089ca430f33a545cf077ebc76ed558b1a96))
