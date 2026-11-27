# WBProxyCenter Rust 重构框架设计文档

> 版本：v1.0（设计冻结稿）
> 目标读者：负责实现本项目的 AI Agent（ChatGPT / Codex 等）与项目维护者
> 上游事实来源：本仓库 Go 版 v1.2.30 的 `internal/`、`web/`、`docs/` 与 `README.md`
> 参考实现：<https://github.com/router-for-me/CLIProxyAPI>（凭证目录形态）

---

## 目录

| 章 | 内容 |
| --- | --- |
| 0 | 怎么用这份文档（读法、MUST/SHOULD、三条总原则） |
| 1 | 项目定义（是什么、不是什么、用户故事） |
| 2 | 技术选型（Rust 栈、依赖白名单、明确禁止） |
| 3 | 仓库结构（workspace 分层、依赖方向） |
| 4 | 架构总览（进程模型、请求路径、关键不变量） |
| 5 | 存储设计（**access_account 目录**、Redis 键空间、写策略、不加密） |
| 6 | 账号域（健康状态机、错误分类、冷却、额度、刷新） |
| 7 | 网关域（选号、重试、并发、流式、出站构造、协议适配） |
| 8 | 媒体域（图片、视频、结算、下载、DNS） |
| 9 | 自动化域（11 个任务、计划、执行、保留期） |
| 10 | 代理域（节点、订阅、桥接侧车、全局出口） |
| 11 | 统计、计费与错误可观测性（**错误详情、筛选、人民币、双轨指标**） |
| 12 | 系统与运维（登录、设置、日志、自更新、备份、Docker） |
| 13 | 配置项全集（每一项的类型、默认值、范围） |
| 14 | API 规格（管理面 + 网关面全量端点） |
| 15 | 前端设计规范（**MD3、蓝色令牌、组件、动效、逐页布局**） |
| 16 | CI / CD（GitHub Actions、版本号、发版） |
| 17 | 测试策略（16 条必须有测试的行为） |
| 18 | 功能对照与验收（F01–F27、验收清单） |
| 19 | 里程碑（M0–M9） |
| 20 | 附录（业务码、术语、错误示范） |

---

## 0. 怎么用这份文档

### 0.1 读法

1. 先读第 1～4 章，建立整体图景：这个软件到底做什么、用哪些技术、代码怎么分。
2. 再读第 5 章（存储）与第 6 章（账号）。**账号与凭证是整个项目的底座**，写错这里后面全是返工。
3. 第 7～12 章是各业务域的行为契约，按域实现，一域一提交。
4. 第 13～14 章是配置与 API 的完整清单，实现时逐项打勾。
5. 第 15 章是前端视觉规范。**这一章里的每一个数值都不是建议，是要求**，不要自行"优化"。
6. 第 16～20 章是工程化、测试、迁移与验收。

### 0.2 本文件里 MUST / SHOULD / MAY 的含义

| 词 | 含义 |
| --- | --- |
| **MUST** | 必须照做。偏离即为缺陷，验收会失败。 |
| **SHOULD** | 应当照做。确有理由才可偏离，且必须在 `CHANGELOG.md` 写明理由。 |
| **MAY** | 可选，实现者自行判断。 |

### 0.3 三条不可协商的总原则

1. **行为等价优先。** 重构不改变对外行为。上游协议、错误码、状态机、重试语义、统计口径，全部照旧。不确定的地方回读 Go 源码，不要猜。
2. **不要加戏。** 文档没写的东西不要加。不要"顺手"引入消息队列、ORM、GraphQL、微服务、服务发现。
3. **数据不加密（管理员密码除外）。** 这是维护者的明确决定：除管理员密码做哈希外，其余落盘数据一律明文。见 5.6。

---

## 1. 项目定义

### 1.1 它是什么

WBProxyCenter 是一个**自托管的上游账号聚合网关**。它把若干 WorkBuddy 账号（国内版 `copilot.tencent.com` / 国际版 `www.workbuddy.ai`）组织成一个号池，对外暴露标准的 OpenAI / Anthropic 兼容 API，并附带一整套运营面板：账号池管理、代理管理、自动化任务、API 密钥、调用日志、用量统计、版本更新。

一句话：**用自己的账号，对外提供一个可用的 API 端点。**

### 1.2 它不是什么

- 不是官方产品，不隶属于 WorkBuddy / CodeBuddy / 腾讯。
- 不提供账号，不绕过上游封禁，不保证可用性。
- 不是 SaaS：默认单机自托管，`127.0.0.1:3040` 起手。
- 不是通用 LLM 路由中间件：所有上游语义都绑定 WorkBuddy 协议。

### 1.3 核心用户故事

| # | 用户 | 想要 | 系统必须提供 |
| --- | --- | --- | --- |
| U1 | 运营者 | 把 5～200 个账号导进来，统一对外提供 API | 文件夹式凭证库、批量导入、健康分组、失败自动换号 |
| U2 | 运营者 | 给不同下游发放不同权限的 Key | API Key 绑定号池、模型白名单、账号白名单、额度与并发 |
| U3 | 运营者 | 不让某个账号被榨干 | 每日 Token/积分护栏、保留积分、模型级熔断、并发上限 |
| U4 | 运营者 | 让账号每天自动签到、领福利、做成长任务 | 计划任务 + 执行记录 + 并发控制 + 精确到账号的结果 |
| U5 | 运营者 | 网络环境差，要挂代理 | HTTP/SOCKS5 代理、Clash 订阅导入、内置桥接、全局出口 |
| U6 | 运营者 | 知道钱花在哪了 | 调用日志、用量历史、Token/费用/积分/TPS/RPM 统计 |
| U7 | 运营者 | 换机器不用重装 | 单二进制 + 数据目录 + 站内自更新与回滚 |

### 1.4 与旧版的关系

旧版是 Go 1.26 + Vue 3，约 6 万行 Go、8 千行 Vue。新版是 Rust + Vue 3，**功能集合必须完全覆盖旧版**，见第 18 章的功能对照表。旧版的历史包袱（`state.enc` 单文件全量重写、加密层、巨型 `server` 包）在新版里全部丢弃。

---

## 2. 技术选型

### 2.1 语言与运行时

| 项 | 选定 | 说明 |
| --- | --- | --- |
| 语言 | Rust，edition 2021，MSRV 1.85 | edition 2024 可选，但 2021 的 crate 兼容性更稳 |
| 异步运行时 | tokio 1.x，`features = ["full"]` | 全异步，不许在 async 上下文里做阻塞调用 |
| HTTP 服务 | axum 0.8 + tower 0.5 + tower-http | axum 只做路由与提取器；业务逻辑不写在 handler 里 |
| 出站 HTTP | **自研 `wbpc-wire`**（hyper 1 + rustls 0.23） | 见 7.5，必须控制请求头顺序与请求体字节 |
| TLS | rustls 0.23 + webpki-roots | 不依赖系统 OpenSSL，跨平台一致 |
| 数据库 | Redis ≥ 7.0（单实例；哨兵可选） | 仅此一个存储，不引入 SQL |
| 序列化 | serde + serde_json（**开启 `preserve_order`**） | 字段顺序要保留，见 7.5 |
| 日志 | tracing + tracing-subscriber（json 或 pretty 二选一） | 结构化日志，禁止 `println!` |
| 时间 | chrono（带 `serde`） | 统一存 RFC3339 字符串 + Unix 毫秒双份 |
| ID | ulid | 单调、可排序、可读 |

### 2.2 依赖白名单

以下 crate 允许直接使用。**新增任何依赖都必须在 `CHANGELOG.md` 登记理由**，且不得添加功能重叠的库。

```toml
# Cargo.toml 片段（workspace 依赖）
tokio        = { version = "1", features = ["full"] }
axum         = { version = "0.8", features = ["macros", "multipart"] }
tower        = "0.5"
tower-http   = { version = "0.6", features = ["compression-gzip", "trace", "timeout", "limit"] }
hyper        = { version = "1", features = ["client", "http1"] }
hyper-util   = { version = "0.1", features = ["tokio"] }
http         = "1"
http-body-util = "0.1"
rustls       = "0.23"
tokio-rustls = "0.26"
webpki-roots = "0.26"
bytes        = "1"
serde        = { version = "1", features = ["derive"] }
serde_json   = { version = "1", features = ["preserve_order"] }
serde_yml    = { version = "0.0.12" }   # Clash YAML 解析
redis        = { version = "0.27", features = ["tokio-comp", "connection-manager", "script"] }
chrono       = { version = "0.4", features = ["serde"] }
ulid         = { version = "1", features = ["serde"] }
thiserror    = "2"
anyhow       = "1"
tracing      = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
bcrypt       = "0.15"
sha2         = "0.10"
hmac         = "0.12"
base64       = "0.22"
hex          = "0.4"
url          = "2"
percent-encoding = "2"
mime_guess   = "2"
futures      = "0.3"
async-trait  = "0.1"
rand         = "0.8"
scopeguard   = "1"
parking_lot  = "0.12"
indexmap     = { version = "2", features = ["serde"] }
notify       = "7"          # access_account 目录监听
fs2          = "0.4"        # 文件锁（Windows/Linux 一致）
tar          = "0.4"        # 更新包解压
flate2       = "1"          # gzip
zip          = "2"          # Windows zip 更新包
self-replace = "1"          # 跨平台自替换
tempfile     = "3"
glob         = "0.3"
```

测试专用：

```toml
[dev-dependencies]
wiremock     = "0.6"
insta        = "1"
rstest       = "0.23"
serial_test  = "3"
```

### 2.3 明确禁止

- ❌ ORM / SQL 库（本项目只有 Redis）
- ❌ `reqwest` 用于上游对话请求（会丢请求头顺序与字节保真；只允许用于"非协议关键"的下载，如代理订阅拉取）
- ❌ 任何前端 UI 框架（Vuetify / Element Plus / Naive UI / Tailwind / UnoCSS）
- ❌ 任何动画库（GSAP / motion / anime.js）——动效全部用 CSS
- ❌ 任何图表库（ECharts / Chart.js）——图表用原生 SVG 手写
- ❌ `unwrap()` / `expect()` / `panic!` 出现在 `src/`（测试与 `main` 启动期除外）
- ❌ 加密存储（见 5.6）
- ❌ 把整个状态 JSON 全量读写当成主存储（旧版的性能死结就在这）

---

## 3. 仓库结构

```
WBProxyCenter/
├─ AGENTS.md                  # Agent 行为纲领（每次开工必读）
├─ CHANGELOG.md               # 唯一更新日志（每次任务必须追加）
├─ REFACTOR-SPEC.md           # 本文件
├─ README.md                  # 面向用户的使用文档
├─ Cargo.toml                 # workspace
├─ rust-toolchain.toml
├─ Dockerfile
├─ docker-compose.yaml
├─ access_account/            # ★ 账号凭证真源（运行期生成，见 5.2）
│  ├─ .gitkeep
│  └─ <realm>-<nickname>-<shortid>.json
├─ data/                      # ★ 运行期数据（git 忽略）
│  ├─ inbox/                  # 上传暂存（批次导入）
│  ├─ staging/                # 媒体下载暂存
│  ├─ updates/                # 更新包与备份
│  └─ logs/                   # 可选的文件日志
├─ crates/
│  ├─ wbpc-core/              # 领域类型、错误、时间、ID、单位换算
│  ├─ wbpc-wire/              # 出站 HTTP/1.1 客户端（顺序保真）
│  ├─ wbpc-store/             # Redis 仓储 + access_account 文件库
│  ├─ wbpc-provider/          # WorkBuddy 上游协议
│  ├─ wbpc-gateway/           # 路由、选号、重试、流式、协议适配
│  ├─ wbpc-media/             # 图片/视频、任务轮询、下载暂存
│  ├─ wbpc-proxy/             # 代理节点、订阅解析、桥接托管
│  ├─ wbpc-automation/        # 计划任务与执行器
│  ├─ wbpc-api/               # 管理 API + 网关 HTTP 层
│  └─ wbpc-server/            # 二进制入口（main.rs）
├─ web/                       # Vue 3 + TS + Vite
│  ├─ src/
│  │  ├─ design/              # 设计令牌（tokens.css）、原语样式
│  │  ├─ components/          # 通用组件
│  │  ├─ pages/               # 七个页面
│  │  ├─ api/                 # 类型化 API 客户端
│  │  └─ stores/              # Pinia
│  └─ tests/                  # Playwright
├─ docs/
│  ├─ PROTOCOL.md             # 上游协议事实（沿用旧版，逐条核对后更新）
│  ├─ DASHBOARD-STATS.md      # 统计口径
│  └─ releases/               # 每个版本的发行说明
└─ .github/workflows/
   ├─ ci.yml
   └─ release.yml
```

### 3.1 依赖方向（单向，违反即编译不过）

```
wbpc-server → wbpc-api → wbpc-gateway → wbpc-provider → wbpc-wire
                     ↘        ↘             ↘
                      wbpc-media  wbpc-automation  wbpc-proxy
                                 ↘
                              wbpc-store → wbpc-core
```

规则：**`wbpc-core` 不依赖任何内部 crate**；`wbpc-store` 只依赖 `core`；`api` 不直接碰 Redis；`provider` 不知道 Redis 的存在。

---

## 4. 架构总览

### 4.1 进程模型

单进程、多任务：

```
main
 ├─ 启动自检（数据目录、Redis 连接、access_account 扫描）
 ├─ axum Server（管理面 /api + 网关面 /v1 /v2 同端口 3040）
 ├─ 后台服务组（tokio::spawn，统一 CancellationToken 管理）
 │   ├─ 账号探测循环        5s 一轮，最多 4 并发
 │   ├─ 模型目录刷新        可配置分钟级
 │   ├─ 额度刷新            可配置小时级
 │   ├─ 自动化调度器        30s 一轮
 │   ├─ 视频任务轮询        15s 一轮，最多 4 并发
 │   ├─ 迟到计费补账        30s→60s 退避，最多 8 次
 │   ├─ 更新检查            10 分钟一轮
 │   ├─ access_account 监听  文件事件 + 5s 兜底轮询
 │   └─ Redis 写入器         批量合并，见 5.4
 └─ 优雅停机（SIGINT / SIGTERM / Ctrl-C）
```

### 4.2 一次对话请求的完整路径

```
Client ──POST /v1/chat/completions──▶ axum
        ├─ 1. 认证：Bearer → sha256 → Redis 查 key_id（进程内 LRU 兜底）
        ├─ 2. 解析请求体（保留字段顺序）
        ├─ 3. 归一化：developer→system、tool_choice、stop、max_completion_tokens
        ├─ 4. 别名替换（若开启）
        ├─ 5. 选号（第 7.1 节）
        ├─ 6. 出站：wbpc-wire 构造请求（顺序头 + 原始字节）
        ├─ 7. 上游 200 → 流式转发；非 200 → 错误映射
        ├─ 8. 结算：usage 落 Redis、日志落 Redis、用量历史聚合
        └─ 9. 回写状态：冷却、健康、每日计数
```

### 4.3 一次失败的换号决策

```
候选列表（已排序）
  ↓ for attempt in 0..=retry_count
  ├─ 取下一个健康账号
  ├─ 发送（连接超时 / 响应头超时 / 流空闲超时 三层）
  ├─ 若首帧有效输出前失败 → 记录失败，继续下一个
  └─ 若已开始转发有效输出 → 不再换号，直接把错误发出去
```

### 4.4 关键不变量（写代码时随时自检）

1. **一份凭证同时只允许一个刷新在飞。** 用 Redis 锁 `wbp:lock:refresh:<account_id>`。刷新成功后必须立刻落盘新 refresh token——旧 token 已作废，丢失即账号永久失效。
2. **日志与用量是两套数据。** 清理调用日志不许清掉用量统计。
3. **结算只结算增量。** 视频任务按"本次累计 − 已结算"记账，重复轮询不重复计费。
4. **健康账号才接新请求。** 待探测、限流、回避、异常一律不接。
5. **已发出的流不能改 HTTP 状态码。** 只能发 SSE 错误帧。
6. **所有时间戳以 Unix 毫秒存 Redis，同时保留 RFC3339 字符串给前端。**

---

## 5. 存储设计

### 5.1 两条铁律

1. **账号凭证只有一个真源：`<项目根>/access_account/` 目录。**
   一个账号 = 一个 `.json` 文件。Redis 里**不存**账号凭证、不存账号配置、不存账号主记录。删账号 = 删文件；改账号 = 重写文件；导入账号 = 把文件放进目录；导出账号 = 从目录读出来打包。这与 CLIProxyAPI 的 `auths/` 完全同构。
2. **Redis 只存"可重建的东西"。**
   运行时状态（冷却、计数、缓存目录、日志、统计、密钥、设置、代理节点、任务）在 Redis。凭证类的东西在文件系统。丢 Redis 不会丢账号，只会丢历史统计。

### 5.2 `access_account` 目录规范

#### 5.2.1 目录位置

| 环境 | 路径 |
| --- | --- |
| 二进制直接运行 | `<可执行文件目录>/access_account/` |
| Docker | 由 `WBP_ACCESS_DIR` 覆盖，默认 `/data/access_account/` |
| 任意 | 环境变量 `WBP_ACCESS_DIR` 优先 |

启动时若目录不存在，**MUST** 自动创建（权限 0700）。目录内 `.gitkeep` 与以 `.` 开头的文件一律忽略。

#### 5.2.2 文件名

```
<realm>-<slug>-<shortid>.json
例：cn-小号A-01J8ZK3Q.json
    intl-main-01J8ZK4M.json
```

- `realm`：`cn` | `intl`
- `slug`：昵称或备注，仅保留 `[A-Za-z0-9\u4e00-\u9fa5_-]`，最长 24 字符，重名时自动加 `-2`、`-3`
- `shortid`：账号 ID（ULID）后 8 位，大写

**文件名不是身份。** 身份是文件里的 `id` 字段。改名不影响账号；两个文件里出现同一个 `id` 时，**后写入者胜出**，并在日志里告警。

#### 5.2.3 文件内容（v1 schema）

```json
{
  "schema": 1,
  "id": "01J8ZK3Q9W7M2R4T6B8V0X1C2D",
  "label": "小号A",
  "realm": "cn",
  "product": "workbuddy",
  "enabled": true,
  "tasks_enabled": true,

  "credentials": {
    "kind": "oauth",
    "access_token": "eyJhbGciOi...",
    "refresh_token": "rt_...",
    "device_token": "",
    "token_type": "Bearer",
    "expires_at": 1790000000000,
    "uid": "12345678",
    "email": "",
    "nickname": "小号A"
  },

  "upstream": {
    "base_url": "https://copilot.tencent.com",
    "billing_url": "https://www.codebuddy.cn",
    "origin": "https://www.codebuddy.cn"
  },

  "egress": {
    "mode": "follow",
    "proxy_id": "",
    "url": "",
    "sticky_id": ""
  },

  "limits": {
    "daily_token_limit": 0,
    "daily_credit_limit": 0,
    "model_daily_token_limit": 0,
    "reserve_credits": 0,
    "max_concurrent": 0
  },

  "models": {
    "disabled": [],
    "catalog_synced_at": "2026-11-27T10:00:00+08:00"
  },

  "note": "",
  "created_at": "2026-11-27T10:00:00+08:00",
  "updated_at": "2026-11-27T10:00:00+08:00"
}
```

字段约束：

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `schema` | int | 当前恒为 `1`。读到更高版本 **MUST** 拒绝加载并告警，不许猜。 |
| `id` | ULID | 唯一。导入时若缺失 **MUST** 生成。 |
| `realm` | `cn` \| `intl` | 决定上游域名、出站身份、号池。 |
| `credentials.kind` | `oauth` \| `apikey` \| `device` | `ck_` / `pt_` 静态 Key 记为 `apikey`，不尝试刷新。 |
| `expires_at` | 毫秒 | `0` 表示不过期。过期但有 `refresh_token` 仍算可用（下次使用时刷新）。 |
| `egress.mode` | `bind` \| `follow` \| `direct` | `bind` 用 `proxy_id` 或 `url`；`follow` 走全局设置；`direct` 强制直连。 |
| `limits.*` | number | `0` = 继承全局默认（见第 13 章）。 |

**未列出的字段 MUST 被忽略并保留原样写回**（前向兼容）。实现上：先 `serde_json::Value` 读，再结构化读，写回时合并未知字段。

#### 5.2.4 写入协议（原子）

任何写入 **MUST** 遵循：

```
1. 取文件锁 <name>.json.lock（fs2::FileExt::lock_exclusive）
2. 写 <name>.json.tmp（同目录，避免跨设备 rename）
3. file.sync_all()
4. rename(tmp → target)         # Windows 上先 rename target → .bak 再 rename tmp
5. 释放锁
```

- 读取方永远只会看到完整的旧文件或完整的新文件。
- 写失败 **MUST** 回滚内存状态并向前端报错，不许"内存改了但盘上没改"。
- 每次写入同时更新 `updated_at`。

#### 5.2.5 目录监听

两条通道并行，互为兜底：

1. `notify` 监听 `Create` / `Modify` / `Remove` / `Rename`，抖动 300ms 合并。
2. 每 5 秒扫描一次目录，比对 `(文件名, mtime, size)`，发现变化就重新加载。

加载流程：

```
扫描 → 逐个解析 → 校验 schema → 建索引 → 与内存快照 diff
     → 新增：加入账号池、入队首次探测
     → 变更：保留运行时状态（冷却/计数），替换凭证与配置
     → 删除：从池中移除，日志保留，正在飞的请求允许跑完
```

**外部手改文件必须生效。** 这是这个设计的核心价值：用户可以 `scp` 一个 json 上去，5 秒内账号就出现在面板里。

#### 5.2.6 导入 / 导出

**导入（三种入口，同一套解析器）**

| 入口 | 行为 |
| --- | --- |
| 面板拖拽 / 选择文件 | 上传到 `data/inbox/`，返回批次 ID，后台解析入库（写 `access_account/`） |
| 面板粘贴 JSON | 同上，不落 inbox，直接内存解析 |
| 直接往目录放文件 | 监听器发现即入库，无需任何 API |
| OAuth 登录 | 拿到凭证后直接生成文件 |

支持的输入形态（与旧版一致，一个都不能少）：

- `{ "accounts": [ ... ] }`
- `[ ... ]` 账号数组
- 单个账号对象
- 桌面端 `{ "auth": {...}, "account": {...} }`
- `credentials` / `items` 数组包装
- `accessToken` 与 `access_token` 两种命名
- 裸 token 字符串
- 本系统自己导出的文件（含 `schema` 字段，直接落盘不重新解析）

区域判定：调用者显式指定优先 > JWT `iss`/`domain` 推断 > 默认 `cn`。

**去重**：以 `credentials.refresh_token`（没有则 `access_token`）的 SHA-256 指纹判重。已存在则**更新**而不是新增，并保留原 `id`。

**导出**

| 范围 | 产物 |
| --- | --- |
| 所选账号 | 单个 JSON 文件（`{ "accounts": [...] }`） |
| 当前筛选结果 | 同上 |
| 全部 | 同上；超过 6 MB 自动拆分 `part-1.json`、`part-2.json`… |

导出内容 = `access_account` 文件内容 + 运行时快照（余额、状态、模型目录摘要），后者放在 `runtime` 字段里，重新导入时忽略。**出口代理的用户名密码明文写在里面**，前端导出前 **MUST** 提示这一点。

#### 5.2.7 为什么这样设计

- **备份 = 复制文件夹。** 不需要 mysqldump，不需要理解加密。
- **迁移 = 换机器复制文件夹。** Redis 清空也能跑，只是历史统计归零。
- **排障 = 打开 json 看。** 不用写解密脚本。
- **批量操作 = 脚本。** `jq` 一把梭。

### 5.3 Redis 键空间

#### 5.3.1 命名与类型约定

- 前缀统一 `wbp:`，层级 `wbp:<域>:<实体>[:<子键>]`。
- 所有结构化值 **MUST** 以 JSON 字符串存于 `hash` 字段或 `string` 值中。**不要**把对象拍平成散列字段——字段演进会失控。
- 所有时间戳字段名以 `_at` 结尾，值为 **Unix 毫秒**；同时保留 `<name>_iso` 的 RFC3339 字符串给前端展示。
- 所有计数用 `i64`；金额用 `f64`（积分本身是浮点，别用整数分）。

#### 5.3.2 元数据

| Key | 类型 | 内容 |
| --- | --- | --- |
| `wbp:meta:schema` | string | 整数 schema 版本，启动时校验并跑迁移 |
| `wbp:meta:boot` | string | 本次启动的 ULID，用于识别僵尸锁 |
| `wbp:meta:instance` | hash | 实例信息：版本、启动时间、监听地址 |

#### 5.3.3 设置

| Key | 类型 | 内容 |
| --- | --- | --- |
| `wbp:settings` | hash | 字段名 → JSON 值。**整体就一个 hash**，一次 `HGETALL` 读全量 |
| `wbp:settings:rev` | string | `INCR` 单调递增；进程内缓存据此失效 |

设置写入 **MUST** 走 Lua：`HSET` + `INCR rev` 原子执行。局部更新（只改一个字段）不得重写其他字段。

#### 5.3.4 账号运行时（不含凭证！）

| Key | 类型 | 内容 |
| --- | --- | --- |
| `wbp:acct:index` | set | 全部账号 ID |
| `wbp:acct:realm:<cn\|intl>` | set | 按号池索引 |
| `wbp:acct:health:<group>` | set | `healthy` / `limited` / `avoiding` / `invalid` / `pending` |
| `wbp:acct:<id>` | hash | 运行时快照（下表） |
| `wbp:acct:<id>:models` | string | 模型目录 JSON 数组，探测成功后写入 |
| `wbp:acct:<id>:cooldowns` | hash | 模型名 → `{"until_ms":…,"started_at_ms":…,"reason":"…","kind":"model"}` |

`wbp:acct:<id>` 字段：

```
health            string   healthy|limited|avoiding|invalid|pending
last_error        string   人话错误，直接展示
last_error_kind   string   credential|limit|upstream|avoidance|""
last_sync_at      i64      最近一次成功探测
cooldown_until    i64      账号级暂停到期（401 后的 60s 等）
avoidance_since   i64
avoidance_next_at i64
avoidance_streak  i32
avoidance_failures i32
restricted        bool     上游限制（需人工确认恢复）
restricted_reason string
balance           f64
quota_total       f64
quota_used        f64
quota_synced_at   i64
balance_refused_at i64     显式余额拒绝时间（压过缓存余额）
credit_packages   string   JSON 数组
```

**这里出现的所有字段都可以从上游重新算出来。** 删掉整片 `wbp:acct:*` 的唯一后果是：面板短暂显示"未探测"，然后 5 秒内自动补齐。

#### 5.3.5 API 密钥

| Key | 类型 | 内容 |
| --- | --- | --- |
| `wbp:key:index` | zset | score = 创建时间，按创建倒序展示 |
| `wbp:key:<id>` | hash | 全部字段，含 `secret` 明文（见 5.6） |
| `wbp:key:hash:<sha256>` | string | → key id，鉴权热路径只查这个 |

`wbp:key:<id>` 字段：`name` / `prefix` / `secret` / `realm` / `enabled` / `restrict_models` / `models`(JSON) / `allowed_accounts`(JSON) / `token_limit` / `daily_token_limit` / `concurrency_limit` / `image_rpm_limit` / `media_daily_credit_limit` / `created_at` / `requests` / `tokens_used`。

计数器类字段（`requests` / `tokens_used`）**MUST** 用 `HINCRBY` 增量更新，不许读-改-写。

#### 5.3.6 代理

| Key | 类型 | 内容 |
| --- | --- | --- |
| `wbp:proxy:index` | zset | score = 创建时间 |
| `wbp:proxy:<id>` | hash | `name`/`type`/`server`/`port`/`url`/`requires_bridge`/`raw`(JSON)/`check`(JSON) |
| `wbp:proxy:global` | string | 当前全局代理 ID，或 `__random__` |

#### 5.3.7 自动化

| Key | 类型 | 内容 |
| --- | --- | --- |
| `wbp:task:index` | zset | 计划列表 |
| `wbp:task:<id>` | hash | 计划本体（含 `progress` JSON） |
| `wbp:run:index` | zset | score = 执行时间 |
| `wbp:run:task:<task_id>` | zset | 该计划的执行记录 |
| `wbp:run:<id>` | hash | 单条执行记录 |
| `wbp:job:<id>` | hash | 运行中的一轮（含取消标记、进度） |
| `wbp:job:active` | set | 正在跑的任务 ID |

#### 5.3.8 调用日志

| Key | 类型 | 内容 |
| --- | --- | --- |
| `wbp:log:<id>` | hash | 完整一条日志 |
| `wbp:log:index` | zset | score = 请求时间（毫秒） |
| `wbp:log:model:<model>` | zset | 模型索引 |
| `wbp:log:key:<key_id>` | zset | 密钥索引 |
| `wbp:log:realm:<realm>` | zset | 号池索引 |
| `wbp:log:status:<cls>` | zset | `ok` / `client` / `upstream` / `error` |
| `wbp:log:day:<yyyymmdd>` | zset | 按天分桶，用于保留期裁剪 |

写入用 Lua 脚本一次完成：`HSET` 日志 + 5 个 `ZADD` + 计数 `HINCRBY`。

查询走"最近 N 天的时间窗 + 交集"：

```
查询：某模型 + 某密钥 + 最近 7 天
1. ZRANGEBYSCORE wbp:log:index  (now-7d) (now  LIMIT 0 2000
2. 对候选 id 批量 HMGET 过滤模型/密钥
3. 结果超过 2000 条时改为按索引集合做 ZINTER
```

**候选集上限 2000**，超过就走 `ZINTER`（Redis 6.2+ 支持 `ZINTER` 多 key）。不许全量 `HGETALL` 扫描。

#### 5.3.9 用量历史（与日志独立）

| Key | 类型 | 内容 |
| --- | --- | --- |
| `wbp:usage:day:<yyyymmdd>` | hash | 全局聚合 |
| `wbp:usage:day:<yyyymmdd>:model` | hash | 模型 → 聚合 JSON |
| `wbp:usage:day:<yyyymmdd>:acct` | hash | 账号 → 聚合 JSON |
| `wbp:usage:day:<yyyymmdd>:key` | hash | 密钥 → 聚合 JSON |
| `wbp:usage:days` | zset | 有数据的日期列表，用于保留期裁剪 |

聚合 JSON 字段：`requests`、`tokens_in`、`tokens_out`、`tokens_cached`、`credits`、`cost_usd`、`errors`、`duration_ms_sum`、`ttft_ms_sum`、`ttft_count`。

**保留 400 天**，超过按 `wbp:usage:days` 裁剪。清理调用日志**不影响**这里。

#### 5.3.10 媒体

| Key | 类型 | 内容 |
| --- | --- | --- |
| `wbp:video:<id>` | hash | 视频任务（含累计用量、结算状态） |
| `wbp:video:index` | zset | score = 创建时间 |
| `wbp:video:poll` | zset | score = 下次轮询时间（调度队列） |
| `wbp:media:price` | hash | 模型 → 媒体积分价学习结果 |

`wbp:video:poll` 是轮询器的工作队列：`ZRANGEBYSCORE ... 0 now LIMIT 0 20` 取出待轮询任务，处理完 `ZADD` 新分数。

#### 5.3.11 会话与限速

| Key | 类型 | 内容 | TTL |
| --- | --- | --- | --- |
| `wbp:session:<sha256>` | string | 会话标记 | 24h |
| `wbp:login:attempt:<ip>` | string | 登录失败计数 | 5min |
| `wbp:rl:key:<id>:day:<yyyymmdd>` | string | 密钥日 Token 计数 | 2d |
| `wbp:rl:key:<id>:rpm:<yyyymmddhhmm>` | string | 密钥每分钟图像数 | 2min |
| `wbp:rl:acct:<id>:day:<yyyymmdd>` | string | 账号日 Token 计数 | 2d |
| `wbp:lock:<name>` | string | 分布式锁（带持有者 token） | 30s |

限流计数 **MUST** 用 Lua 做"读+判断+加"，不许先 GET 再 SET——并发下会击穿。

#### 5.3.12 其他

| Key | 类型 | 内容 |
| --- | --- | --- |
| `wbp:hold:<id>` | string | 账号风控暂停截止毫秒 |
| `wbp:egress:backoff` | hash | 出口键 → `{"until_ms":…,"failures":…}` |
| `wbp:update:state` | hash | 更新检查结果、上次检查时间 |
| `wbp:update:release` | string | 已校验的最新发行 JSON |
| `wbp:bridge:state` | hash | 桥接开关、每个节点的本地端口 |
| `wbp:resp:<id>` | string | Responses 历史（TTL 24h） |
| `wbp:resp:key:<key_id>` | zset | 按 Key 隔离与裁剪 |

### 5.4 写策略

旧版把整个 `State` 序列化成一个大 JSON 每写一次全量落盘，长跑之后一次写要几百毫秒——这是它最大的性能债。新版分三档：

| 档 | 场景 | 做法 |
| --- | --- | --- |
| **强一致** | 改设置、建/改/删 Key、账号状态跃迁、任务结束 | 同步执行，返回前必须写成功 |
| **合并写** | 请求日志、用量计数、冷却写入 | 进内存队列，**每 200ms 或攒满 100 条**打一次 pipeline |
| **尽力写** | 心跳类、临时标记 | 允许丢，重启后重算 |

合并写的队列 **MUST** 在优雅停机时 flush，且最多等待 5 秒。

### 5.5 锁与并发

- 分布式锁用 `SET key token NX PX 30000`，释放用 Lua 比对 token —— 这是唯一正确的释放方式，禁止直接 `DEL`。
- 需要锁的场景只有四个：凭证刷新、自动化单账号执行、更新安装、目录重整。
- 单实例部署时锁仍在 Redis 里，不要退化成进程内 `Mutex`（否则将来加实例就会踩坑）。

### 5.6 关于"不加密"

**这是本项目的明确设计决定，不是疏漏。**

| 数据 | 处理 |
| --- | --- |
| 管理员密码 | bcrypt 哈希（cost 10），**唯一**做保护的数据 |
| 账号 access/refresh/device token | `access_account/*.json` 明文 |
| API Key 明文 | Redis 明文（同时存 sha256 便于鉴权） |
| 代理用户名密码 | Redis 明文 |
| 更新用 GitHub Token | Redis 明文 |
| 设置、日志、统计 | 明文 |

必须在 `README.md` 与首次启动页明确写一句话：

> **本项目不加密本地数据。`access_account/` 与 Redis 数据等同于凭证本身，请自行限制目录权限、做好备份、不要把数据目录暴露给他人。**

实现要求：

- `access_account/` 创建时权限 `0700`，文件 `0600`。
- Docker 下 `PUID/PGID` 决定属主。
- 面板"导出账号"按钮前必须弹一次确认，写明"文件包含明文凭证与代理密码"。
- **不许**再引入任何 AES / 密钥文件 / 加密信封。

### 5.7 迁移（旧版 → 新版）

提供一次性命令 `wbproxy migrate --from <旧数据目录> --to <新目录>`，流程：

```
1. 读旧 state.enc + encryption.key（旧版是 AES-256-GCM）
2. 解出完整 State
3. 账号 → 每个写一个 access_account/*.json
4. 代理 / 密钥 / 设置 / 任务 → 写 Redis
5. 日志与用量历史 → 分批写 Redis（每批 1000 条）
6. 调用旧版模型目录缓存 → 丢弃（会重新拉）
7. 输出迁移报告：成功/失败计数，失败条目落 data/migrate-failed.json
```

迁移工具**只在本版本存在**，不进入常态代码路径。旧数据目录 **只读**，绝不原地修改。

---

## 6. 账号域

### 6.1 账号的三种状态，别混

新手最容易把这三件事搅在一起。请严格区分：

| 名称 | 存哪 | 谁改 | 丢了会怎样 |
| --- | --- | --- | --- |
| **凭证状态**（token 是否有效） | `access_account/*.json` | 刷新流程 | 账号消失，需要重新导入 |
| **健康状态**（能不能接请求） | Redis `wbp:acct:<id>` | 网关运行时 | 重新探测即可 |
| **运营状态**（是否启用、限额） | `access_account/*.json` | 管理员 | 回到默认值 |

### 6.2 健康分组（五组，与旧版一致）

```
                     ┌──────────────────────────────┐
                     │  凭证被拒 / 无 token / 被限制  │──▶ invalid  异常
                     │  过期且无 refresh_token        │
                     └──────────────────────────────┘
                     ┌──────────────────────────────┐
                     │  积分耗尽 / 账号级暂停调度     │──▶ limited  限流
                     └──────────────────────────────┘
                     ┌──────────────────────────────┐
                     │  流错误 code 0（回避标记）     │──▶ avoiding 回避
                     └──────────────────────────────┘
                     ┌──────────────────────────────┐
                     │  上游 5xx / 连接失败 / 同步失败│──▶ invalid  异常
                     └──────────────────────────────┘
                     ┌──────────────────────────────┐
                     │  从未成功探测过                │──▶ pending  未探测
                     └──────────────────────────────┘
                     ┌──────────────────────────────┐
                     │  以上都不是                    │──▶ healthy  健康
                     └──────────────────────────────┘
```

判定顺序 **MUST** 严格按此优先级（先命中先返回）：

```
1. restricted == true                        → invalid
2. token 为空                                → invalid
3. last_error_kind == "credential"           → invalid
4. expires_at > 0 且已过期 且 无 refresh_token → invalid
5. 积分耗尽 或 账号级 cooldown 未到期         → limited
6. last_error_kind == "avoidance"            → avoiding
7. last_error_kind == "upstream"
   或 avoidance_since > 0                    → invalid
8. last_sync_at == 0                         → pending
9. 其他                                       → healthy
```

**只有 healthy 才接新请求。** 没有例外，不要写"以 pending 兜底"这种便利代码。

### 6.3 错误分类（决定记在谁头上）

网关必须区分"账号有问题"和"请求有问题"。分类错了会导致：一个客户端的坏请求把健康账号打成异常。

| 来源 | 条件 | 归到 | 是否暂停账号 |
| --- | --- | --- | --- |
| 凭证拒绝 | 上游 401 / HTTP 200 流内 401 | `credential` | 暂停 1 分钟并尝试刷新 |
| 上游封禁 | 403 / 业务码 11140 / 明确封禁文案 | `restricted` | 持久停用，需人工恢复 |
| 账号限流 | 429 / 14003，上游明确写出账号级 | cooldown | 按 Retry-After |
| 模型限流 | 429 / 14003，仅某个模型 | 模型冷却 | 只影响该模型 |
| 免费额度窗口 | 6004 + 有效上游重置时间，且模型在白名单 | 配额窗口 | 持久，需等窗口到期 |
| 连接失败 | 传输层错误、DNS、TLS | `upstream` | 按每账号策略 |
| 流错误 | `WorkBuddy stream failed (HTTP 200, code 0)` | `avoidance` | 按每账号策略 |
| 请求内容被拒 | 400 / 404 / 413 / 422，非限流非封禁 | **不记账号** | 否 |
| 模型级业务码 | 11102 / 11129 / 11134 / 11150 / 11158 | **不记账号** | 否 |
| 中国移动模型失败 | `cmcc:` 前缀且非 401/403/429 | **不记账号** | 否 |

模型级业务码清单是硬编码常量，**不许**改成"按 HTTP 状态猜"。

### 6.4 冷却与恢复

#### 6.4.1 模型限流退避

没有上游重置时间时按连续次数退避：

```
第 1 次：60s
第 2 次：120s
第 3 次：240s
第 4 次：480s
第 5+ 次：600s（封顶）
```

**连续判定**：上一个窗口结束后 **15 分钟内**再次限流才算连续；
- 期间有成功请求 → 计数归零
- 间隔超过 15 分钟 → 计数归零
- 同一窗口内并发返回的多个 429 **不叠加计数**（用「冷却开始时间」去重）

#### 6.4.2 每账号回避策略（两种，管理员二选一）

| 策略 | 行为 |
| --- | --- |
| **账号冷却**（默认） | 连接失败与流错误**混合累计**，达到阈值（默认 3，范围 1–100）才入组。成功后计数归零。若因连接失败入组：保留当前出口，按间隔（默认 10 分钟，范围 1–10080）定时探测模型目录与额度直至恢复。若仅因流错误入组：等下一次模型同步，不启动连接探测。 |
| **代理轮询** | 不用阈值。连接失败即入异常并暂停调度；到期后只给受影响的账号切到代理池里下一个可用节点，再探测。其他账号不动。跳过同地址、检测失败、桥接未启动的节点；没有别的节点就保留当前出口，**绝不回退直连**。 |

倒计时只代表"可以再试了"，**不代表已恢复**。恢复必须以探测结果为准。

#### 6.4.3 401 处理

```
收到 401（含流内 401）
  ├─ 立即把账号标记 credential，暂停 1 分钟
  ├─ 后台用 refresh_token 换新凭证
  │    ├─ 成功 → 落盘新 token → 解除暂停 → 计入健康
  │    └─ 失败 → 保持 credential → 面板提示"重新登录或导入"
  └─ 每个账号最多每 10 分钟刷新一次
```

### 6.5 额度护栏

三个层级，从宽到严：

```
全局默认（设置页）
   ↓ 账号上填了正数就覆盖
账号级
   ↓
单模型级（只在模型窗口/每日模型 Token 上）
```

| 护栏 | 语义 |
| --- | --- |
| `daily_token_limit` | 当日输入+输出 Token 上限，按站点时区午夜重置 |
| `model_daily_token_limit` | 单模型当日 Token 上限 |
| `daily_credit_limit` | 当日积分上限 |
| `reserve_credits` | 保留积分：余额 ≤ 该值即视为不可用（`x0` 免费模型除外） |
| `max_concurrent` | 该账号同时在途请求上限 |

**这是接单护栏，不是预付费精确配额。** 并发中的请求会让最终用量略超设定值，这是可接受的，不要为它加锁串行化。

### 6.6 凭证刷新

```
acquire lock wbp:lock:refresh:<id> (NX PX 30s)
  ├─ 再次确认 token 确实过期（双检）
  ├─ POST 上游 /v2/plugin/auth/token/refresh
  │    headers: X-Refresh-Token, X-Auth-Refresh-Source
  │    cn → workbuddy / intl → plugin
  ├─ 解析（兼容单层与双层 data 信封）
  ├─ 写 access_account/<file>.json（原子）
  ├─ 更新 Redis 运行时（expires_at）
  └─ release lock
```

**硬要求**：

1. 刷新必须串行。并发刷新会消费同一个 refresh token，第二次必然失败并把账号打成失效。
2. 刷新成功 **必须先把新 token 落盘再放锁**。顺序颠倒 = 崩溃即失号。
3. 静态 Key（`ck_` / `pt_`）**不尝试刷新**。
4. 刷新失败不得清空原 token，只标记 `last_error`。

---

## 7. 网关域

### 7.1 选号算法

一次请求的候选集，按顺序做减法：

```
输入：key（含 realm / allowed_accounts / models / 限额）、请求 model（已做别名替换）、会话 ID

1. 号池过滤
   key.realm == "all" → 优先请求体/客户端偏好；否则按 intl→cn 回退设置（默认关闭）
   key.realm == "cn"  → 只取 realm=cn
   key.realm == "intl"→ 只取 realm=intl（回退开启时可追加 cn）

2. 账号过滤（全部满足才留下）
   ├─ enabled == true
   ├─ 文件存在且凭证非空
   ├─ health_group == "healthy"
   ├─ key.allowed_accounts 为空 或 包含该 id
   ├─ key.restrict_models 为 false 或 模型在 key.models 内
   ├─ 模型不在账号 disabled_models
   ├─ 模型不在账号模型冷却中
   ├─ 不在 restriction_hold 窗口（号池级风控暂停）
   ├─ 出口代理可用（桥接未启动时，隧道节点视为不可用）
   ├─ 出口不在 egress_backoff 退避窗口
   └─ 未超出账号并发 / 账号日 Token / 日积分 / 保留积分

3. 排序（稳定，可复现）
   若开启「剩余额度优先」：
      按 (剩余积分 − 保留积分) 降序；同分按最近使用时间升序
   否则：
      按最近使用时间升序（LRU 轮转），用 Redis ZSET 的 score 记录

4. 会话亲和覆盖
   请求带 conversation_id / 上次响应 id，且该会话绑定账号仍在候选集中
   → 把该账号提到第一位（不做跨池强制，绑定账号不可用就按正常顺序）

5. 取前 N 个作为候选队列（N = retry_count + 1，上限 8）
```

**关键约束**

- 候选为空时返回 HTTP 429，带 `Retry-After`、`X-RateLimit-Reset`、JSON `error.reset_at`。不许返回 500，也不许"随便挑一个试试"。
- 选择过程本身 **不写** 账号状态，只有真正发出去之后才更新 `last_used_at`。
- 候选人顺序在整轮重试内固定，不许中途重排。

### 7.2 重试语义

```
attempt = 0
loop:
  取候选[attempt]
  lease = 申请账号并发槽 + Key 并发槽
  resp = 发送请求
  match resp:
    ├─ 连接失败 / 响应头超时 / 上游 500·502·503·504 / 空回答
    │    且 尚未转发任何有效输出
    │    → 记一次失败，attempt += 1，继续
    ├─ 429 且 明确账号级
    │    → 暂停账号，attempt += 1，继续
    ├─ 401
    │    → 标记 credential，attempt += 1，继续（并触发后台刷新）
    ├─ 403 / 11140 / 封禁文案
    │    → 持久停用账号，**立即终止本次请求**，不换号
    ├─ 已开始转发有效输出后断流
    │    → 不换号，发错误帧，结束
    └─ 成功
         → 转发、结算、结束
```

**不占重试次数的情况**：候选本身没发就往上游（并发已满、冷却中、已停用）。

**30 秒避让**：同一账号+模型刚失败过，后续 30 秒内优先尝试别的账号；只有一个健康账号时仍然照用。

**不做重试的情况**：图像生成、视频提交。收到空正文时提示用户核对任务记录，不许自动重提（会重复扣费）。

### 7.3 并发控制

三层令牌，从外到内：

| 层 | 键 | 上限来源 |
| --- | --- | --- |
| 全局媒体提交 | `media-image` / `media-video` | 设置 `image_max_concurrency` / `video_max_concurrency`，0 = 不限 |
| Key 并发 | `wbp:rl:key:<id>:inflight` | 密钥 `concurrency_limit`，0 = 不限 |
| 账号并发 | `wbp:rl:acct:<id>:inflight` | 账号 `max_concurrent` 覆盖设置 `account_max_concurrent`，0 = 不限 |

- 用 RAII guard 实现：`Drop` 时释放并 `HINCRBY -1`，**进程崩溃也不会泄漏**（启动时清零所有 inflight 键）。
- 视频异步任务提交完成后就释放 HTTP 并发槽，官方那边继续跑不占本地槽位。

### 7.4 流式

#### 7.4.1 保活

```
客户端请求 stream=true
  ↓
立即写一帧： ": wbproxy connected\n\n"
  ↓
每 10 秒（可配 stream_heartbeat_seconds）写一帧： ": keepalive\n\n"
  ↓
上游数据到达 → 原样转发
```

- 注释帧不算模型回复，**不计入 TTFT，不计 Token**。
- 心跳只在完整 SSE 事件边界写入，不许插到半个事件中间。
- 首帧必须在等待上游**之前**发出——这是 sub2api 这类客户端不超时的关键。

#### 7.4.2 超时三件套

| 超时 | 默认 | 设置项 | 作用 |
| --- | --- | --- | --- |
| 连接超时 | 10s | 固定 | 建连 |
| 响应头超时 | 60s | `response_header_timeout_seconds` | 等首字节 |
| 流空闲超时 | 300s | `stream_idle_timeout_seconds`（0–1800，0 关闭） | 两帧之间 |
| 整请求超时 | 300s | `request_timeout` | 兜底总时限 |

#### 7.4.3 已发流后的错误

HTTP 状态码已经发出去了，改不了。只能：

```
event: error
data: {"error":{"type":"rate_limit_error","message":"…","reset_at":1234567890},"reset_at":…,"retry_after":…}
```

**不许**发 `[DONE]` 或 `response.completed` 来"假装成功"。

#### 7.4.4 非流式请求

客户端要 `stream:false`，但上游只有流式：

```
出站强制 stream=true + stream_options.include_usage=true
收齐：文本、reasoning、并行工具参数片段、finish_reason、usage
再合成 JSON 返回
```

合成规则：**只要出现任意一个有效内容块就算成功**；空正文 / 空 choices / 只有 role / 只有 usage / 只有结束标记 → 视为失败，按 7.2 重试。

### 7.5 出站请求构造（`wbpc-wire`）

这是整个重构里最容易做歪的地方。旧版为此自己写了一个 HTTP transport，原因有三：请求头顺序、请求体字节保真、以及对重定向的绝对控制。

#### 7.5.1 三条硬要求

1. **请求头按固定顺序逐条写出**，不许用 `HeaderMap` 迭代（顺序不保证）。
2. **原生 `/v2` 请求体原样转发**，一个字节都不改；兼容入口在必须改写时，用保留字段顺序的解析器（`serde_json` 的 `preserve_order`）重排后再序列化。
3. **禁止自动跟随重定向**——凭证会跟着 `Location` 泄漏。要跟必须手工跟，并且剥掉 `Authorization`。

#### 7.5.2 实现

```rust
pub struct WireRequest {
    pub method: Method,
    pub scheme: &'static str,   // https
    pub host: String,
    pub path: String,           // 含 query
    pub headers: Vec<(HeaderName, HeaderValue)>,  // 顺序即写出顺序
    pub body: Bytes,
}

pub struct WireClient { pool: ConnPool, tls: Arc<ClientConfig>, .. }

impl WireClient {
    pub async fn send(&self, req: WireRequest) -> Result<WireResponse, WireError>;
}
```

- 连接池按 `(scheme, host, port, proxy, identity)` 分桶，空闲 90 秒回收，最多 8 条/桶。
- TLS 用 rustls，**不做**证书例外（不允许 `danger_accept_invalid_certs` 出现在上游对话路径）。
- HTTP 版本固定 1.1。上游不支持 h2，别开 ALPN。
- `Connection: keep-alive`，`Accept-Encoding: identity`（除非开启 GZIP 压缩）。

#### 7.5.3 代理

代理在 wire 层以下解决：

| 类型 | 实现 |
| --- | --- |
| HTTP / HTTPS | CONNECT 隧道；HTTPS 代理先与代理建 TLS，再在隧道内建 TLS |
| SOCKS5 / SOCKS5H | 标准握手，支持用户名密码认证 |
| 桥接节点 | 本地 `127.0.0.1:<port>` 的 HTTP 代理 |

要求：**首跳按代理主机验证证书，隧道内按目标主机验证证书**（两个都验，不许跳过一个）。

### 7.6 协议适配

对外五个入口，对内一个上游 `/v2/chat/completions`：

| 入口 | 处理 |
| --- | --- |
| `POST /v1/chat/completions` | 归一化后转发；流式原样透传 |
| `POST /v2/chat/completions` | **原生入口**，字节保真，只做 `developer` 角色修正 |
| `POST /v1/responses` | 双向转换（请求 → chat，chat → 响应事件流） |
| `POST /v1/messages` | 同上（Anthropic 语义） |
| `POST /v1/images/*`、`/v1/videos*` | 走媒体管线（第 8 章） |

#### 7.6.1 请求归一化（只作用于兼容入口）

| 输入 | 处理 |
| --- | --- |
| `developer` 角色 | 改 `system`（上游 400 拒收） |
| 首条不是 system | 补一条最小中性 system |
| `tool_choice` 对象 | 转字符串形式 |
| `stop` 字符串 | 转数组 |
| 旧 `functions` | 转 `tools` |
| `max_completion_tokens` | 转 `max_tokens` |

#### 7.6.2 身份句净化

第三方 Agent（Codex / Claude Code）会在消息里带固定自述句，命中上游渠道指纹校验会返回 400 / 11128。需要：

- 只处理 **assistant 历史、reasoning、工具描述** 中的自述句，替换为中性句。
- **user / tool 正文原样保留**，不许动。
- 计费头 `x-anthropic-billing-hdr` 及其缩写形式的首部元数据要处理。
- 规则表放 `crates/wbpc-provider/src/sanitize.rs`，用 `insta` 快照测试锁死。

#### 7.6.3 响应转换

- Chat → 原样透传（流式逐帧）。
- Chat → Responses：`response.created` → `response.output_text.delta` → `response.completed`，工具调用转 `function_call` 事件。
- Chat → Messages：`message_start` → `content_block_start/delta/stop` → `message_delta` → `message_stop`。
- 只处理第一个 choice。多候选请用原生 Chat 入口。
- 被拒（content_filter）保留为可读文本；Responses 用 `incomplete` 结束。

### 7.7 错误映射

对外统一信封：

```json
{ "error": { "message": "人话", "type": "gateway_error", "code": "upstream_429" },
  "message": "人话" }
```

| 内部情况 | HTTP | type | 额外头 |
| --- | --- | --- | --- |
| 未登录管理面 | 401 | `auth_error` | — |
| Key 无效 | 401 | `auth_error` | — |
| Key 被禁用 | 403 | `auth_error` | — |
| 无候选账号 | 429 | `rate_limit_error` | `Retry-After`、`X-RateLimit-Reset` |
| 上游限流 | 429 | `rate_limit_error` | 同上 |
| 请求体过大 | 413 | `invalid_request_error` | — |
| 请求内容被上游拒 | 上游状态 | `upstream_error` | — |
| 上游 5xx | 502 | `upstream_error` | — |
| 连接失败 | 502 | `upstream_error` | — |
| 媒体模型不支持的操作 | 501 | `not_implemented` | — |

错误文案里的上游业务码 **必须**保留，并附一句"一般不是账号故障"，避免用户误以为是号池问题。

### 7.8 会话亲和

- 请求带 `conversation_id`，或带 `previous_response_id`。
- 优先调度到上一次同一会话所用的账号，减少缓存失效。
- 没有会话 ID 时，用 `sha256(account_uid + 第一条 user 文本)` 稳定生成一个，**同一个提示词反复请求要落到同一个账号**（`affinity_stable_key_enabled`，默认开）。
- 绑定只是"优先"，账号不可用时必须能落到别的账号，不许因此失败。

### 7.9 模型目录与可见性

#### 7.9.1 目录来源

- 主：`GET /v3/config`（`agents` 中 CLI 的模型 ID，保留精确 ID、metadata、`credits` 倍率、tags、`videoGeneration` 约束）。
- 兜底：目录空或不可用时试 `GET /v2/enterprises/personal/models`。
- **不许**把本地写死的模型列表冒充账号真实目录。

#### 7.9.2 `cmcc:` 模型的特殊规则

`cmcc:` 前缀（目录里 `vendor=cmcc`）是中国移动 TokenPlan 模型，扣「算力豆」而不是普通积分。

| 位置 | 是否列出 |
| --- | --- |
| `/v1/models` | ❌ 不列出 |
| `/v1/media/models` | ❌ 不列出 |
| 密钥页 CC Switch 导入的模型建议 | ❌ 不列出 |
| 号池管理的模型卡片 | ✅ 显示（便于单独处理） |
| 密钥的模型权限列表 | ✅ 显示 |
| 别名 / 日志筛选 | ✅ 显示 |

- 判定依据是 **ID 前缀 `cmcc:`**，不是"有没有 credits 倍率"。
- 客户端显式用完整 ID 调用时**原样转发**。
- 普通模型名**不许**被路由到 `cmcc:` 模型。
- 这类模型的失败按模型级处理（401/403/429 除外），不写账号 `last_error`；11145 无论出现在哪个模型都同样处理。

#### 7.9.3 别名

`model_aliases` 只替换请求里的 `model` 字段；响应里仍显示实际模型。别名以 `cmcc:` 开头时同样不对外列出。

#### 7.9.4 列表过滤

`/v1/models` 按 API Key 的号池、账号白名单、模型权限过滤；`filter_models_by_key=false` 时不过滤，但**实际调用权限不变**——过滤开关不是授权开关。

---

## 8. 媒体域

### 8.1 入口与上游映射

| 网关入口 | 上游 |
| --- | --- |
| `POST /v1/images/generations` | `POST /v2/images/generations` |
| `POST /v1/images/edits` | `POST /v2/images/edits` |
| `POST /v1/videos`（Sora 风格） | `POST /v2/videos/generations` |
| `POST /v2/videos/generations`（原生） | 同上，字节保真 |
| `GET /v1/videos/{id}` | `POST /v2/videos/tasks`，body `{"task_id":"…"}` |
| `GET /v1/videos/{id}/content` | 取产物 URL → 下载 → 回传 |
| `GET /v1/media/models` | 本地目录 |
| `GET /v1/media/tasks` | 本地记录 |

### 8.2 图片

- 按上游模型前缀构造请求：`hunyuan*` 带 `n:1`；`gemini*` 不带 `response_format`；其余 `b64_json`。
- 尺寸上限 4 MP，超了报参数错误。
- `edits` 接受 data URL 数组与 multipart 文件；**不读服务器本地路径**，**不代下载任意 URL**。
- WorkBuddy 没有 mask 字段 → mask 明确报错，不许静默忽略。
- 部分模型会把 JSON 误标成 `text/event-stream`：**必须按响应内容嗅探**，不能只看 Content-Type，否则会丢掉已付费的图。

### 8.3 视频

#### 8.3.1 提交

```
解析 seconds / size / aspect_ratio / negative_prompt / 首尾帧 / 参考素材
  ↓
按 videoGeneration 约束校验（时长、分辨率、方向、提示词长度、素材数量、
首尾帧与参考素材互斥、常开音频）
  ↓
参考素材：data URL → 申请预签名地址 → PUT 原始字节（不带账号鉴权）
  ↓
提交 /v2/videos/generations
  ↓
落 Redis：账号、Key、模型、规格、创建时间、累计用量、已结算用量
```

- 图生视频不发送 `aspect_ratio`。
- 不擅自去水印。
- 目录里没有的约束不猜，交给上游判断。

#### 8.3.2 轮询

```
每 15 秒一轮
  ZRANGEBYSCORE wbp:video:poll -inf now LIMIT 0 20   # 最久未轮询优先
  最多 4 个并发
  每个任务用「提交时的账号」查询 —— 永远不换号
```

- 查询结果 `completed` 但没有产物 → 保持 `in_progress`，不许把空结果报成完成。
- 账号已删除 → 标 `lost`，保留历史行，禁用下载。

#### 8.3.3 结算

```
新累计 = 上游报告的累计用量
本次记账 = max(0, 新累计 − 已结算累计)
只把「本次记账」写进日志与统计
```

这是全项目最容易重复扣费的地方，写出三遍也要记住：**只记增量**。

#### 8.3.4 迟到结算

```
任务进入终态但没有正积分
  → 启用一次有界补账窗口
  → 10 分钟内最多 8 次，首次 30s，之后 60s
  → 次数、截止时间、下次时间持久化，重启不重置
  → 拿到正积分或账号被删即停
```

超过窗口后，只有客户端主动查询才会继续补记。

### 8.4 下载

两种访问方式，共用同一条校验链路：

| 方式 | 鉴权 |
| --- | --- |
| `GET /v1/videos/{id}/content` | 本地 HMAC 签名（15 分钟）**或** Bearer API Key |
| 管理面板 | 登录会话 + 一次性令牌 |

下载流程：

```
1. 用任务原账号向上游查产物 URL
2. 校验 URL 主机解析到公网地址（拒绝私有/保留网段、拒绝重定向）
3. HTTPS 下载到 data/staging/<ulid>.part
4. 校验：声明长度必须与实际一致；未知长度最多读 4 GiB + 1 字节判定超限
5. 校验通过 → 原子 rename 成 <ulid>.mp4
6. 交给客户端；结束后删除
```

**DNS 处理**：`media_download_dns` 可选 `system` / `cloudflare` / `google`。

- 选定 DoH 时，通过**原账号代理**查询固定官方端点，用固定公网入口连接并校验官方域名证书。
- 两类查询共用 8 秒预算，单次响应 ≤ 64 KiB。
- 校验响应码、截断标记、别名链、返回地址；拒绝非公网结果。
- 把连接固定到已验证 IP。
- **不许自动回退**到系统 DNS（切换服务商要用户自己点）。

**403 分类**：最多读 8 KiB XML，只有完整命中固定 Code/Message 对才归类为 `asset_signature_expired` / `asset_clock_skew` / `asset_signature_mismatch` / `asset_access_denied`；否则保留 `upstream_status`。**不许**把上游正文原样透给前端。

### 8.5 面板下载的两阶段

```
POST /api/media/tasks/{id}/prepare   → 返回 { download_url, expires_at, size }
GET  /api/media/downloads/{id}       → 真正下载（一次性）
```

| 约束 | 值 |
| --- | --- |
| 令牌有效期 | 2 分钟内必须开始下载 |
| 消费次数 | 1 次 |
| 并发准备中文件 | 全局最多 4 个，每会话 2 个 |
| 超限 | 429 |
| 重复准备已就绪文件 | 复用同一地址 |
| 仍在准备/下载中 | 409 |
| 会话注销 / 服务关闭 / 传输结束 | 立即清理暂存文件 |

### 8.6 媒体积分价格学习

- 图片：按**成功返回的实际张数**学习 `creditPerImage`。
- 视频：按模型 × 实际出站分辨率 × 时长 × 是否有参考素材分档学习。
- 「参考素材」只指 `reference_images` / `reference_videos` / `reference_audios` **非空数组**；首帧尾帧不算。
- 未实测：图片兜底 6 积分/张；视频 `creditPerSecond = null`，兼容字段 `creditPerSecondCeiling = 26`。
- 错误、预览、无产物、无法确定分档的任务 **不参与学习**。
- 重复查询已结算任务 **不覆盖**更新的报价。

---

## 9. 自动化域

### 9.1 任务目录（十一个，缺一不可）

| kind | 名称 | 号池 | 默认时段 | 说明 |
| --- | --- | --- | --- | --- |
| `checkin` | 每日签到 | cn | 09:00 / 21:00 | 加油站签到 |
| `growth` | 成长任务 | cn | 手动 | 接取并同步真实进度 |
| `miniprogram` | 小程序任务 | cn | 手动 | 同步真实进度领奖 |
| `night` | 夜猫子任务 | cn | 01:00 | 仅 23:00–08:00 处理 |
| `travel` | 猫猫旅行 | cn | 09:00 / 21:00 | 领奖、开盒、再派出 |
| `buddy_open` | Buddy 盲盒 | cn | 手动 | 按可用能量开盒 |
| `streak` | 连登管家 | cn | 手动 | 补签、兑换、抽奖 |
| `activity` | 活跃状态同步 | all | 手动 | 只读取真实状态 |
| `daily_chat` | 每日活跃对话 | intl | 随机双时段 | 真实对话 |
| `daily_welfare` | 每日任务一键执行 | all | 03:00 | intl 对话 / cn 依次签到+旅行+连登+夜猫子 |
| `keepalive` | 凭证保活 | all | 22:00 | 剩余不足 48h 才刷新 |

另有 `model_refresh`（模型刷新）与 `quota_refresh`（额度刷新）属于维护类，不产生执行记录。

### 9.2 计划模型

```rust
pub struct Task {
    pub id: String,
    pub name: String,
    pub kind: String,            // 上表 kind
    pub realm: String,           // cn | intl | all
    pub preset: String,          // 预设播种时写入，用于识别
    pub enabled: bool,
    pub schedule: String,        // "manual" | "daily" | "interval"
    pub times: Vec<String>,      // ["09:00","21:00"]
    pub timezone: String,        // "Asia/Shanghai"
    pub interval_minutes: i32,
    pub options: serde_json::Value,
    pub last_run_at: i64,
    pub next_run_at: i64,
    pub status: String,          // idle | running | done | failed | stopped
    pub progress: Option<TaskProgress>,
}
```

### 9.3 预设播种

首次启动时把十一个预设写进 Redis，**全部暂停**，时间固定：

- 新装默认：北京时区 03:00，开关关闭。
- 已有自定义任务与执行记录 **不许覆盖**。
- 预设只播种一次（Redis 里有 `wbp:task:seeded` 标记）。

### 9.4 一轮执行

```
POST /api/tasks/{id}/run 或 定时触发
  ├─ 生成 run_id（ULID）
  ├─ 按计划的范围选账号（号池筛选 + 健康 + 限流 + 自动化参与开关）
  ├─ 并发 = min(计划并发, automation_max_concurrency)，默认 5
  ├─ 每账号串行，重试预算 automation_max_retries（默认 3，0 = 不重试）
  ├─ 单轮硬时限 30 分钟（进入账号执行阶段后计）
  ├─ 每完成一个账号落一条执行记录 + 更新进度
  └─ 结束：写最终状态，清 RunRealm
```

**终止**（`POST /api/tasks/{id}/stop`）：

- 只取消本轮。未开始的账号不再执行，等待中的请求取消。
- 已由上游完成的操作 **不撤销**，也不发补偿请求。
- 下次定时仍按原计划跑。

### 9.5 成功判定

**上游业务结果说了算。**

| 情况 | 记录 |
| --- | --- |
| 上游确认完成 | `success` |
| 上游明确"已完成但未领奖" | `pending`（可重试） |
| 上游说还需要客户端操作 | `skipped` + 说明 |
| 未确认 | 不算成功 |
| 失败 | `failed` + 原因 |

**绝对禁止虚构使用记录来领积分。** 成长、小程序、夜猫子必须按官方真实进度领取；需要专家/模板/其他客户端时，记录里写清楚"还需要做什么"。

### 9.6 去重与共享标记

- 国际版「每日福利」与「每日活跃」共享当天成功标记，避免重复发起对话。
- 每日标记按 **上海时间** 跨日重置。
- 福利领取标记 **长期保留**。
- 补签成功后要重新读连登状态再兑换，读失败就保留失败原因，**不许按过期状态领奖**。

### 9.7 保留期

- 执行记录：1–365 天，或 0 不清理。新装默认 30 天。
- 修改保留期保存后 **立即** 删除已过期记录。
- 清空执行记录 **不影响** 计划与每日完成标记。

---

## 10. 代理域

### 10.1 节点模型

```rust
pub struct Node {
    pub id: String,
    pub name: String,
    pub kind: String,        // http | https | socks5 | ss | ssr | vmess | vless | trojan | hysteria2 | tuic | wireguard | snell | anytls …
    pub server: String,
    pub port: u16,
    pub url: String,         // http(s)/socks5 可直接使用时填写
    pub requires_bridge: bool,
    pub raw: serde_json::Value,  // 原始节点参数，桥接配置生成用
}
```

### 10.2 导入

支持四种来源，统一解析成 `Vec<Node>` + `Vec<Warning>`：

1. 单个分享链接（`http://`、`https://`、`socks5://`、`vmess://`、`vless://`、`trojan://`、`ss://`、`hysteria2://`、`tuic://`、`anytls://`、`ssr://`）
2. Clash / Mihomo YAML（`proxies:` 数组）
3. Base64 编码的订阅内容
4. 订阅 URL（HTTP 拉取，20 秒总超时，大小上限 8 MiB）

要求：

- 去重按 `(kind, server, port, 关键参数)` 的指纹。
- 逐节点报错，一个坏节点不影响其他节点。
- 导入完成后自动关闭弹窗，警告留在页面上。
- 订阅 URL **不得**内嵌用户名密码。

### 10.3 桥接（重要取舍）

旧版把 Mihomo 编进二进制，支持 SS/SSR/VMess/VLESS/Trojan/Hysteria/Hysteria2/TUIC/WireGuard/Snell/AnyTLS/SOCKS5 等一整套协议。

**Rust 里不要重写这些协议。** 那不是重构，那是重造一个 mihomo，工作量与风险都不合理。

**选定方案：桥接侧车（sidecar supervisor）**

```
wbpc-proxy::bridge
  ├─ 定位 mihomo 可执行文件（优先随包分发，其次 PATH，其次设置里指定路径）
  ├─ 生成配置 data/bridge/config.yaml
  │    ├─ mixed-port: 0（不对外）
  │    ├─ listeners: 每个桥接节点一个 127.0.0.1:<固定端口>
  │    └─ proxies/proxy-groups 由 Node.raw 生成
  ├─ 以子进程启动，stdout/stderr 落 data/bridge/bridge.log（滚动）
  ├─ 端口分配：从 20000 起，落到 Redis wbp:bridge:ports，重启后复用
  ├─ 健康检查：每 10 秒探测各端口 TCP 连通
  ├─ 节点变更 → 热重载（SIGHUP 或 RESTful API），失败则整体重启
  ├─ 退出/崩溃 → 指数退避重启，最多 5 次后标记桥接不可用
  └─ 停止桥接 → 绑定隧道节点的账号立即拒绝出站，不回退直连
```

理由：

- 行为等价（用的还是 mihomo 内核，参数解析结果一致）。
- 体积换复杂度：多带一个二进制，少写两万行协议代码。
- 许可：mihomo 是 GPL-3.0，本项目整体也已经按 GPL-3.0 发行，沿用即可。第三方声明放 `THIRD_PARTY_NOTICES.md`。

`requires_bridge=false` 的普通 HTTP/SOCKS5 节点不经过桥接，直接用。

### 10.4 全局出口

- `global_proxy_enabled` 打开后，没有单独绑定代理的账号统一走：
  - 指定节点，或
  - 「随机化跟随」：每个账号第一次用时随机分配，之后固定（`sticky_id` 存进账号文件）。
- 分配时优先选**承载账号最少**的节点。
- 删除节点只影响用它的账号，自动重分配。
- 找不到可用代理 → 账号暂停调度，**绝不回退直连**。
- 被选为全局代理的节点不能直接删，要先换。
- 账号自己绑定的代理 **优先于** 全局设置。

---

## 11. 统计、计费与错误可观测性

### 11.1 两套数据，别混

| 数据 | 保留 | 用途 | 清理影响 |
| --- | --- | --- | --- |
| **调用日志**（每条请求明细） | 天数 + 条数 + 体积 | 排查、导出 CSV、视频任务列表、错误分析 | 清掉就没了 |
| **用量历史**（按天/模型/账号/密钥聚合） | 400 天 | 仪表盘趋势与排行 | 清日志**不影响** |

### 11.2 指标定义（口径不许改）

| 指标 | 定义 |
| --- | --- |
| **请求数** | 已结算的网关调用条数。重试不额外计数。 |
| **输入 / 输出 / 缓存 Token** | 上游 `usage` 实际报告的数值，不按字符估算。 |
| **RPM** | 最近 60 秒**开始时间**落在窗口内的请求数（含在途）。 |
| **TPM** | 最近 60 秒上游实际报告的输入+输出 Token；流式按上报秒累计增量，重复累计值不重复计。 |
| **TPS** | 成功文本请求的输出 Token 总量 ÷ 这些请求的生成耗时总和。流式扣掉首字等待。样本不足显示"—"，**不许写 0**。 |
| **费用** | 按价格表算的**参考估算**，不是上游账单。展示币种见 11.8。未知模型不估价。 |
| **积分消耗** | 只累加上游实际返回的扣分。拿不到就显示"未确认"，**不用余额差推算**。 |
| **缓存节省** | 同一价格表下「不用缓存的费用 − 实际费用」。旧记录没有价格快照就不追溯。 |
| **成功率** | 成功请求 ÷ 有明确结果的请求。 |

### 11.3 积分与货币

- 积分**没有**统一兑换率 → 余额的货币价值显示"未知"，不要拿某个模型的比值套整个池子。
- 大数用「万 / 亿」显示，悬停看完整值。
- 余额计入规则：按号池当前的「异常」分类**排除**该账号快照；账号恢复后重新纳入。
- 所选期间的消耗不因账号后来变异常而被删除。

### 11.4 仪表盘刷新节奏

| 场景 | 间隔 |
| --- | --- |
| 正在查看的组合 | 30 秒 |
| 闲置组合 | 3 分钟 |
| 页面隐藏 | 停止轮询 |
| 实时 KPI（RPM/TPM/并发） | 15 秒，串行 |

后端做快照缓存，多个标签页不重复计算。

---

### 11.5 错误详情模型（比旧版详细得多）

旧版日志里只有一个 `error` 字符串加一个 `upstream_code` 整数，排查时等于没有信息。新版每条失败日志 **MUST** 存一个结构化的 `error_detail`。

#### 11.5.1 结构

```rust
pub struct ErrorDetail {
    pub kind: String,           // 见 11.5.2，闭集
    pub stage: String,          // 见 11.5.3，闭集
    pub attribution: String,    // account | request | gateway | upstream | network
    pub counts_against_account: bool,

    pub http_status: Option<u16>,
    pub upstream_code: Option<i32>,
    pub upstream_message: String,   // 上游原文，截断 512 字节
    pub retryable: bool,
    pub retry_index: Option<u32>,
    pub reset_at: Option<i64>,
    pub retry_after_ms: Option<i64>,

    pub account_id: String,
    pub account_label: String,
    pub egress: String,             // direct | proxy:<id>:<name> | bridge:<id>:<port>

    pub fingerprint: String,        // 归一化指纹，见 11.6
    pub raw_excerpt: String,        // 响应体前 512 字节，已脱敏
    pub steps: Vec<ErrorStep>,      // 重试链
}

pub struct ErrorStep {
    pub attempt: u32,
    pub account_id: String,
    pub account_label: String,
    pub egress: String,
    pub kind: String,
    pub stage: String,
    pub http_status: Option<u16>,
    pub upstream_code: Option<i32>,
    pub message: String,
    pub elapsed_ms: i64,
    pub after_first_token: bool,    // 失败时是否已向客户端发过有效输出
    pub dns: Option<DnsInfo>,
}

pub struct DnsInfo {
    pub resolver: String,       // system | cloudflare | google
    pub host: String,
    pub addrs: Vec<String>,
    pub verdict: String,        // public | private | reserved | failed
    pub elapsed_ms: i64,
}
```

#### 11.5.2 `kind` 取值（闭集，不许自由发挥）

```
credential          凭证被拒（401 / 流内 401）
banned              账号被封禁或限制（403 / 11140）
rate_limit_account  账号级限流（429 / 14003）
rate_limit_model    模型级限流
quota_window        免费额度窗口（6004 + 有效重置时间）
balance             显式余额不足
content_rejected    请求内容被上游拒（400/404/413/422）
model_unavailable   模型级业务码（11102/11129/11134/11150/11158）
cmcc_unavailable    中国移动模型失败（11145 等）
transport           连接失败 / DNS / TLS / 连接被重置
timeout_connect     建连超时
timeout_header      等待响应头超时
timeout_idle        流空闲超时
stream_broken       流中途断开
empty_answer        上游 200 但无有效内容
media_download      产物下载失败
media_upstream      媒体接口业务失败
gateway_internal    网关自身错误
```

#### 11.5.3 `stage` 取值

```
auth  route  connect  send  header  body  stream  settle
media_submit  media_poll  media_download
```

#### 11.5.4 脱敏规则（硬要求）

`raw_excerpt` 与 `upstream_message` 落库前 **MUST** 过一遍脱敏：

| 模式 | 替换为 |
| --- | --- |
| `Bearer <token>`、`sk-...`、`eyJ...`（JWT） | `[redacted-token]` |
| URL 中的签名参数 `X-Signature=`、`X-Amz-*` | `[redacted-signature]` |
| Cookie 头 | `[redacted-cookie]` |
| 邮箱 | 保留首字符 + `***@domain` |
| 手机号 | `138****0000` |
| 代理 URL 里的 `user:pass@` | `***@` |

**不许**把上游响应正文、签名 URL、账号 token 原样写进日志。这是旧版明确做对的地方，不能退化。

### 11.6 错误指纹与聚合

```
fingerprint = sha256(kind, stage, http_status, upstream_code, normalize(message))
```

`normalize` 去掉数字、UUID、时间戳、主机名，只留模板串。

用途：

- 日志列表可"按指纹折叠"，一眼看出这 300 条是同一个错误。
- `wbp:errgroup:<fingerprint>` 保存：首次/最后出现时间、次数、影响的账号集合、最后一次完整 `ErrorDetail`。
- 仪表盘「错误排行」按指纹聚合，点击直接跳到已过滤的日志列表。

---

### 11.7 日志筛选（本次新增重点）

#### 11.7.1 筛选维度

| 维度 | 参数 | 说明 |
| --- | --- | --- |
| 时间 | `from` / `to` | 毫秒或 RFC3339 |
| 模型 | `model` | 精确或前缀 |
| 密钥 | `key_id` | |
| 号池 | `realm` | cn / intl / all |
| 结果 | `status` | ok / error / streaming |
| HTTP 状态 | `http_status` | 单值或 `4xx` 这类区间 |
| 业务码 | `upstream_code` | 如 11128 |
| **错误类型** | `error_kind` | 11.5.2 闭集，可多选 |
| **错误阶段** | `error_stage` | 11.5.3 闭集 |
| **归因** | `attribution` | account / request / gateway / upstream / network |
| **错误指纹** | `fingerprint` | 折叠同类 |
| **账号** | `account_id` | |
| **出口** | `egress` | 直连 / 某代理 |
| **重试次数** | `attempts_min` / `attempts_max` | |
| **半路断流** | `after_first_token` | true / false |
| 路径 | `path` | `/v1/chat/completions` 等 |
| 流式 | `stream` | true / false |
| 耗时 | `duration_min` / `duration_max` | 毫秒 |
| 首字 | `ttft_min` / `ttft_max` | 毫秒 |
| 关键词 | `q` | 搜 model / error.message / error.upstream_message |
| 客户端 | `client_ip` | 脱敏后保留网段 |

#### 11.7.2 组合语义

- 不同维度之间 **AND**
- 同一维度多值 **OR**
- 未传的维度不参与过滤

#### 11.7.3 查询实现规则

```
1. 带 error_kind / error_stage / fingerprint / account_id / egress 时，
   先查对应索引 ZSET
2. 与主时间窗 ZSET 求交（ZINTER，保留 MAX 分数）
3. 结果 ≤ 2000 条 → 批量 HMGET 补齐，内存里过滤剩余细条件
4. 结果 > 2000 条 → 先用索引把范围压到 2000 以内再补
5. 任何情况下禁止 SCAN / KEYS
```

#### 11.7.4 写日志的索引

一个 Lua 脚本内一次写完：

```
wbp:log:index                ZSET  时间
wbp:log:day:<yyyymmdd>       ZSET  时间（保留期裁剪）
wbp:log:key:<key_id>         ZSET
wbp:log:realm:<realm>        ZSET
wbp:log:model:<model>        ZSET
wbp:log:status:<ok|error>    ZSET
wbp:log:err:<kind>           ZSET
wbp:log:stage:<stage>        ZSET
wbp:log:acct:<account_id>    ZSET     ← 新增
wbp:log:egress:<key>         ZSET     ← 新增
wbp:log:fp:<fingerprint>     ZSET     ← 新增
wbp:log:path:<path>          ZSET     ← 新增
```

索引虽多，但都是 ZADD，一次 pipeline 成本可忽略；换来的是查询快一个数量级。

#### 11.7.5 CSV 导出列（顺序固定）

```
时间, 完成时间, 号池, 账号, 账号ID, 密钥, 密钥ID, 路径, 模型, 流式,
HTTP状态, 业务码, 错误类型, 错误阶段, 归因, 是否计入账号, 重试次数,
耗时ms, 首字ms, 输入Token, 输出Token, 缓存Token, 积分, 费用USD, 费用CNY,
失败响应, 上游消息, 出口
```

导出必须**流式**（边查边写），不许先拼成一个大字符串。

---

### 11.8 货币：默认人民币

#### 11.8.1 数据层

价格表本身是 **USD 基准**（上游 catalog 就是美元）。落库时同时存三份：

```
cost_usd   f64   按价格表算出的美元
cost_cny   f64   cost_usd × 结算时汇率
fx_rate    f64   结算时使用的 USD→CNY 汇率
```

**为什么三个都存**：汇率会变。只存 USD 再乘当前汇率，上个月的账单每个月都在变；只存 CNY 又无法追溯原始价。三个都存，历史永远稳定。

#### 11.8.2 展示层

| 设置 | 默认 | 说明 |
| --- | --- | --- |
| `pricing_currency_display` | `CNY` | `CNY` / `USD`，只影响展示 |
| `usd_cny_rate` | `7.10` | 手工汇率，4 位小数 |
| `usd_cny_rate_source` | `manual` | `manual` / `auto` |

规则：

- 默认展示 **¥**，符号在前：`¥12.34`
- 千分位逗号：`¥1,234.56`
- 超过 1 万换成 `¥1.23万`，悬停看完整值
- 切成 USD 显示 `$12.34`，同一条历史金额不变（因为两份都存了）
- `auto` 每天拉一次公开汇率，失败继续用手工值，**不许让价格页因此打不开**
- 价格编辑页 **MUST** 允许直接按人民币输入（内部换算回 USD 存基准），输入框带 `¥` 前缀

#### 11.8.3 文案

- 「费用」一律标注「参考估算」，并注明「按价格表换算，不等于账单」
- 积分旁边不显示货币符号（积分不是钱）

---

### 11.9 仪表盘：今日与总量并存

用户明确要求：**每日 Token、今日等价 API 费用、今日积分消耗必须能看到，而且不能因此看不到总量。** 三条指标都要双轨展示。

#### 11.9.1 三套口径

| 名称 | 定义 |
| --- | --- |
| **今日** | 站点时区当天 00:00 到现在 |
| **所选范围** | 顶部范围选择器：今日 / 7 天 / 30 天 / 自定义 |
| **累计** | 从有记录以来到现在，**不受范围选择器影响** |

**关键规则：范围选择器只影响「所选范围」这一组数字，永远不影响「今日」和「累计」。**

#### 11.9.2 KPI 卡片模板

```
┌──────────────────────────────────┐
│ 请求数                     ⓘ     │   标签 13px
│                                  │
│ 12,480                           │   主值 28px / 600 / tabular-nums
│                                  │
│ 今日 1,204 · 较昨日 +12.4%        │   副行 12px
└──────────────────────────────────┘
```

- **主值 = 所选范围总量。** 范围选「今日」时主值就是今日值，副行改成「累计 X」。
- 副行**永远**有内容，永远不隐藏。
- 卡片七张：请求数、Token、缓存命中、费用（¥）、积分消耗、失败率、TPS。
- 涨跌色只用于副行的箭头，数字本身保持 `on-surface`。

#### 11.9.3 「今日快照」条（新增）

在 KPI 网格上方固定一条，**任何范围选择下都显示**：

```
┌──────────────────────────────────────────────────────────────────────┐
│ 今日 · 2026-11-27                                                    │
│                                                                      │
│ Token  输入 128.4K  输出 42.1K  缓存 96.3K（命中 64.2%）              │
│ 等价 API  ¥18.42    积分消耗 312.5   请求 1,204   失败 3（0.25%）     │
└──────────────────────────────────────────────────────────────────────┘
```

- 背景 `surface-container`，圆角 16，无投影
- 数字 tabular-nums，标签用 `on-surface-variant`
- 每 15 秒随实时 KPI 刷新，数字做 200ms 滚动过渡（不是闪烁）

#### 11.9.4 趋势图

- 同时画两条：**所选范围**（实线，主色）与**近 30 天日均**（虚线，outline）
- 图例里同时给出「范围内合计」与「今日」两个数字
- 指标切换 chips：请求 / Token / 费用 / 积分 / 失败

#### 11.9.5 累计数据放哪

累计不塞进 KPI（会挤），单独放在「用量统计」页签顶部三行：

```
累计请求    1,284,903
累计 Token   18.42 亿（输入 12.1 亿 / 输出 6.32 亿 / 缓存 4.88 亿）
累计费用     ¥1.87 万 · 累计积分 42,118.6
```

并注明「累计自 2026-11-27 开始采集，此前数据不可恢复」——避免用户以为数字缺了。

#### 11.9.6 空状态

- 没数据就显示真实的 0 与「暂无记录」，**不许**填假数据
- 首次启动尚无快照：骨架屏；超过 3 秒才显示「正在计算」

---

## 12. 系统与运维

### 12.1 首次启动与登录

1. 未初始化时，首页直接显示「管理员密钥」和「确认管理员密钥」两个密码输入框。回环、内网、Docker 和反向代理使用相同流程，不需要用户名或验证码，启动日志不生成密钥。首次开站应限制访问，仅让管理员能到达未初始化实例。
2. 两次输入必须完全一致，密钥**至少 10 个字符、最多 72 个 UTF-8 字节**。只允许一次原子初始化，bcrypt 存哈希；成功后直接建立会话。已初始化实例不再提供设置表单，后续登录只输入一次管理员密钥。
3. 登录失败计数按 IP，5 分钟内超过 10 次返回 429
4. 会话：`wbp_session` Cookie，`HttpOnly` + `SameSite=Strict` + `Secure`（TLS 时），24 小时
5. 安全入口（`security_path`）：设了之后控制台只能从 `/<入口>` 打开；`/api` 与 `/v1` 不受影响。入口**不能代替密码**

### 12.2 设置写入语义

- 局部更新：只提交的字段才改，未提交的保持原值
- `null` 或类型错误 → **整包拒绝**，不许部分生效
- 显式 `false` / `0` 必须生效（不能当成"未提交"）
- 保存成功才提示成功；写失败要回滚内存

### 12.3 日志保留

三层限制同时生效，先到先裁剪：

```
log_retention_days   1–365（默认 30）
log_max_rows         0 或 5000–1000000（默认 20000）
log_max_mb           0–1024（默认 0 = 不限）
```

清理日志**不清**用量统计、**不清**视频任务。

### 12.4 自更新

```
检查（每 10 分钟，或手动）
  ├─ 读 Release 元数据（公有仓库直读；私有仓库用 GitHub Token）
  ├─ 只跟随正式发行（跳过 prerelease）
  └─ 结果单独存 Redis，不重写设置

应用
  ├─ 下载到 data/updates/<version>/（显示进度）
  ├─ 校验 SHA-256
  ├─ 解包，跑 --version 自检
  ├─ 备份当前程序为 .previous / .previous.2 …（数量可配，默认 1）
  ├─ 原地替换（Linux/macOS 进程号不变；Windows 同控制台重启）
  └─ 重启后保留登录状态

回退
  └─ 重新下载并校验最近三个旧版本，替换程序
```

**启动自检**：新版本连启动都失败 → 自动恢复上一个程序，并在版本页说明原因。已正常服务的版本**不会**被健康检查自动回退。

### 12.5 备份

| 要备份什么 | 说明 |
| --- | --- |
| `access_account/` | 凭证，**最重要** |
| Redis 数据 | `dump.rdb` / `appendonly.aof`（推荐 AOF `everysec`） |
| `data/updates/` | 可选，回退用 |

README 必须写清楚：

```sh
# 备份
redis-cli BGSAVE && cp -r /var/lib/redis/dump.rdb backup/
tar czf backup-$(date +%F).tgz access_account/ dump.rdb

# 恢复
停服 → 放回 access_account/ 与 dump.rdb → 启动
```

### 12.6 环境变量

| 变量 | 默认 | 说明 |
| --- | --- | --- |
| `WBP_ADDR` | `127.0.0.1:3040` | 监听地址 |
| `WBP_ACCESS_DIR` | `<程序目录>/access_account` | 凭证目录 |
| `WBP_DATA_DIR` | `<程序目录>/data` | 运行数据 |
| `WBP_REDIS_URL` | `redis://127.0.0.1:6379/0` | Redis 连接串 |
| `WBP_REDIS_PASSWORD` | 空 | Redis 密码 |
| `WBP_ADMIN_PASSWORD` | 空 | 仅首次初始化时生效 |
| `WBP_CONTAINER` | 空 | 镜像内设为 `1` |
| `WBP_LOG` | `info` | tracing 过滤串 |
| `WBP_MIHOMO_PATH` | 空 | 桥接内核路径，空则自动查找 |

### 12.7 Docker

```yaml
services:
  wbproxy:
    image: ghcr.io/mizaawa/wbproxycenter:v1.x.y
    container_name: wbproxy
    restart: unless-stopped
    ports: ["3040:3040"]
    environment:
      WBP_ADDR: 0.0.0.0:3040
      WBP_REDIS_URL: redis://redis:6379/0
      WBP_CONTAINER: "1"
    volumes:
      - wbproxy-data:/data
      - wbproxy-access:/data/access_account
    depends_on: [redis]

  redis:
    image: redis:7-alpine
    restart: unless-stopped
    command: ["redis-server", "--appendonly", "yes", "--appendfsync", "everysec"]
    volumes:
      - wbproxy-redis:/data
```

要求：

- 镜像内以非 root 运行（`USER 1000:1000`）
- 入口脚本先修正 `/data` 属主再降权
- **不要**用 `docker compose down -v`（会删卷），README 要写警告

---

## 13. 配置项全集

Redis `wbp:settings` 的字段。**一项都不许漏**，漏了功能就丢。

### 13.1 基础

| 键 | 类型 | 默认 | 范围 | 说明 |
| --- | --- | --- | --- | --- |
| `site_name` | string | `WBProxy Center` | ≤ 40 字 | 站点名，登录页也显示（公开） |
| `gateway_url` | string | 空 | — | 对外展示的网关地址，空则用当前 Host |
| `timezone` | string | `Asia/Shanghai` | IANA | 每日重置与统计口径的时区 |
| `theme` | string | `system` | system/light/dark | 控制台主题（MD3 明暗两套令牌） |

### 13.2 登录与安全

| 键 | 类型 | 默认 | 说明 |
| --- | --- | --- | --- |
| `password_hash` | string | 空 | bcrypt。**唯一做保护的数据** |
| `security_path` | string | 空 | 控制台入口路径 |
| `restriction_hold_seconds` | int | 1800 | 号池级风控暂停时长 |

### 13.3 请求转发

| 键 | 类型 | 默认 | 范围 | 说明 |
| --- | --- | --- | --- | --- |
| `request_timeout` | int | 300 | 5–3600 | 整请求秒数 |
| `response_header_timeout_seconds` | int | 60 | 1–600 | 等首字节 |
| `stream_idle_timeout_seconds` | int | 300 | 0–1800 | 0 关闭 |
| `stream_heartbeat_seconds` | int | 10 | 0–60 | SSE 心跳间隔，0 关闭 |
| `retry_count` | int | 2 | 0–10 | 换号重试次数 |
| `max_payload_mb` | int | 32 | 1–256 | 请求体上限 |
| `gzip_enabled` | bool | false | — | 上游请求压缩 + 响应压缩 |
| `affinity_stable_key_enabled` | bool | true | — | 无会话 ID 时按提示词稳定选号 |

### 13.4 号池与限额

| 键 | 类型 | 默认 | 范围 | 说明 |
| --- | --- | --- | --- | --- |
| `account_max_concurrent` | int | 0 | 0–100 | 每账号并发，0 不限 |
| `account_probe_max_concurrency` | int | 4 | 1–16 | 首次探测并发 |
| `automation_max_concurrency` | int | 5 | 1–10 | 自动化跨计划并发 |
| `automation_max_retries` | int | 3 | 0–10 | 单账号每轮重试，0 不重试 |
| `daily_token_limit` | i64 | 0 | ≥0 | 账号日 Token |
| `model_daily_token_limit` | i64 | 0 | ≥0 | 单模型日 Token |
| `daily_credit_limit` | f64 | 0 | ≥0 | 账号日积分 |
| `reserve_credits` | f64 | 0 | ≥0 | 保留积分 |
| `expiring_window_days` | int | 0 | 0–365 | 即将过期资源包提醒窗口 |
| `remaining_priority_enabled` | bool | false | — | 选号按剩余额度优先 |
| `pool_separation_enabled` | bool | false | — | 按 Key 号池隔离 |
| `realm_fallback_enabled` | bool | false | — | 国际不可用时回退国内 |
| `account_avoidance_strategy` | string | `cooldown` | cooldown/rotate | 每账号回避策略 |
| `account_avoidance_minutes` | int | 10 | 1–10080 | 回避间隔（分钟） |
| `account_avoidance_threshold` | int | 3 | 1–100 | 连续触发阈值 |
| `egress_backoff_enabled` | bool | false | — | 共享出口退避 |
| `realm_limits` | map | `{}` | — | 按号池覆盖上述限额 |

### 13.5 协议与兼容

| 键 | 类型 | 默认 | 说明 |
| --- | --- | --- | --- |
| `enable_anthropic_messages` | bool | true | 启用 `/v1/messages` |
| `reasoning_effort_map` | bool | true | reasoning_effort 映射 |
| `local_web_tools` | bool | false | 启用本地 `web_search` / `web_fetch` |
| `identity.*` | object | 见下 | 出站身份覆写 |
| `prompt_mode` | string | `passthrough` | `passthrough` / `inject` |
| `prompt_text` | string | 空 | `inject` 模式下注入的系统提示词 |
| `auto_switch_product` | bool | false | 自动切换产品身份 |
| `auto_switch_products` | []string | 全部 | 参与切换的产品 |
| `device_token_file` | string | 空 | 设备令牌文件路径 |
| `device_token` | string | 空 | 设备令牌（明文） |

`identity` 子对象：

| 键 | 默认 | 说明 |
| --- | --- | --- |
| `x_private_data` | `auto` | `auto` / `true` / `false` |
| `desktop_version_cn` | `5.8.3` | 国内桌面端版本 |
| `desktop_version_intl` | `5.9.4` | 国际桌面端版本 |
| `cli_version_cn` | `2.161.0` | 国内内置 CLI 版本 |
| `cli_version_intl` | `2.165.0` | 国际内置 CLI 版本 |
| `vscode_version` | 内置值 | VSCode 扩展版本 |
| `vscode_host_version` | 内置值 | VSCode 宿主版本 |
| `standalone_cli_version` | 内置值 | 独立 CLI 版本 |
| `sdk_compat_headers` | false | 是否附加 `X-Stainless-*` 等 SDK 头（只作用于对话） |

### 13.6 图像与视频

| 键 | 类型 | 默认 | 范围 | 说明 |
| --- | --- | --- | --- | --- |
| `image_max_concurrency` | int | 0 | 0–100 | 图片并发 |
| `video_max_concurrency` | int | 0 | 0–100 | 视频提交并发 |
| `video_max_seconds` | int | 0 | 0–3600 | 视频时长上限，0 不限 |
| `media_image_rpm_per_key` | int | 6 | 0–60 | 每 Key 每分钟图片数 |
| `media_daily_credit_limit_per_key` | f64 | 0 | ≥0 | 每 Key 每日媒体积分 |
| `media_download_dns` | string | `system` | system/cloudflare/google | 视频下载 DNS |

### 13.7 模型管理

| 键 | 类型 | 默认 | 说明 |
| --- | --- | --- | --- |
| `model_refresh_minutes` | int | 60 | 目录刷新间隔（0 关闭） |
| `model_aliases` | map | `{}` | 别名 → 实际模型 |
| `model_aliases_enabled` | bool | false | 启用别名 |
| `models_only_available` | bool | false | 列表只显示当前可用 |
| `filter_models_by_key` | bool | true | 列表按 Key 权限过滤 |

### 13.8 代理

| 键 | 类型 | 默认 | 说明 |
| --- | --- | --- | --- |
| `global_proxy_enabled` | bool | false | 所有凭证请求走代理 |
| `global_proxy_id` | string | 空 | 节点 ID，或 `__random__` |

### 13.9 缓存模拟 / 经营策略

| 键 | 类型 | 默认 | 说明 |
| --- | --- | --- | --- |
| `cache_simulation.enabled` | bool | false | 模拟缓存命中 |
| `cache_simulation.hit_rate` | f64 | 0 | 0–1 |
| `cache_simulation.min_tokens` | i64 | 0 | 低于该长度不模拟 |
| `cache_simulation.models` | []string | `[]` | 生效模型 |

### 13.10 价格与货币

| 键 | 类型 | 默认 | 说明 |
| --- | --- | --- | --- |
| `pricing_enabled` | bool | true | 启用参考价估算 |
| `pricing_variant_inherit` | bool | true | 变体模型继承主模型价格 |
| `pricing_refresh_minutes` | int | 1440 | 价格表刷新间隔 |
| `pricing_currency_display` | string | `CNY` | 展示币种：`CNY` / `USD` |
| `usd_cny_rate` | f64 | `7.10` | USD→CNY 汇率，范围 1–20，4 位小数 |
| `usd_cny_rate_source` | string | `manual` | `manual` / `auto`（每日自动拉取，失败回退手工值） |

价格表内部 **USD 基准**，落库同时保存 `cost_usd` / `cost_cny` / `fx_rate` 三份，见 11.8。

### 13.11 日志

| 键 | 类型 | 默认 | 范围 |
| --- | --- | --- | --- |
| `log_retention_days` | int | 30 | 1–365 |
| `log_max_rows` | int | 20000 | 0 / 5000–1000000 |
| `log_max_mb` | int | 0 | 0–1024 |
| `task_run_retention_days` | int | 30 | 0–365 |

### 13.12 更新

| 键 | 类型 | 默认 | 说明 |
| --- | --- | --- | --- |
| `update_check_enabled` | bool | true | 自动检查 |
| `auto_update` | bool | false | 自动安装 |
| `update_proxy_id` | string | 空 | 检查/下载走哪个代理 |
| `update_github_token` | string | 空 | 私有仓库令牌（明文） |
| `update_keep_backups` | int | 1 | 保留旧程序份数 0–5 |
| `release_repo` | string | `mizaawa/WBProxyCenter` | 发行仓库 |

### 13.13 额度巡检

| 键 | 类型 | 默认 | 说明 |
| --- | --- | --- | --- |
| `quota_scan_enabled` | bool | false | 启用额度窗口巡检 |
| `quota_scan_models` | []string | 内置四个 | 巡检模型 |
| `quota_scan_interval_hours` | int | 6 | 巡检间隔 |
| `quota_window_probe_enabled` | bool | false | 窗口探针 |
| `credits_refresh_hours` | f64 | 0 | 余额刷新间隔，0 关闭 |

---

## 14. API 规格

统一约定：

- 管理面 `/api/**` 走 Cookie 会话；网关面 `/v1/**`、`/v2/**` 走 `Authorization: Bearer <key>`。
- 管理面所有非 GET 请求校验 `Origin`，跨站 403。
- 响应统一 `Content-Type: application/json; charset=utf-8`，`Cache-Control: no-store`。
- 错误信封：`{ "error": { "message": "...", "type": "..." }, "message": "..." }`。
- 分页统一 `page`（1 起）+ `page_size`（1–200，默认 20），返回 `{ items, total, page, page_size }`。

### 14.1 认证

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/api/auth/status` | `{ authenticated, needs_setup, setup_code_required, version, site_name }`；`setup_code_required` 仅为兼容旧客户端保留，恒为 `false` |
| POST | `/api/auth/setup` | `{ password, password_confirm }`，两字段必填且相同；长度见 §12.1。不一致返回 400，已设置返回 409，成功时签发会话 Cookie |
| POST | `/api/auth/login` | `{ password, security_path? }` |
| POST | `/api/auth/logout` | — |
| GET | `/api/version` | `{ version }`（公开） |
| GET | `/health` | `{ status, version }`（公开） |

### 14.2 仪表盘

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/api/dashboard?range=24h\|7d\|30d&realm=all\|cn\|intl` | 快照：号池、积分、KPI、趋势、排行、Token 构成、缓存、成功率、延迟、24 小时分布 |
| GET | `/api/dashboard/live` | 最近 60 秒 RPM / TPM / 并发 / 待结算 |
| GET | `/api/dashboard/today` | **今日快照**：今日 Token（输入/输出/缓存）、今日等价 API（`amount` + `currency`）、今日积分消耗、今日请求数、今日失败数。与 `range` 无关，任何范围下都返回 |
| GET | `/api/dashboard/totals` | **累计**：累计请求、累计 Token（三类）、累计费用（双币种）、累计积分。不受 `range` 影响 |

响应里所有金额 **MUST** 同时给 `{ "amount": 18.42, "currency": "CNY", "usd": 2.594, "fx_rate": 7.10 }`，由前端按当前展示币种选择，避免前端自己算汇率。

### 14.3 账号

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/api/accounts?realm=&health=&q=&page=&page_size=` | 列表；`view=ids` 只返回 ID |
| GET | `/api/accounts/{id}` | 详情（含模型目录、资源包、限额生效值） |
| POST | `/api/accounts` | 手工新建（写 `access_account/`） |
| PATCH | `/api/accounts/{id}` | 改名、启用、限额、代理绑定、模型禁用 |
| DELETE | `/api/accounts/{id}` | 删单个（删文件） |
| DELETE | `/api/accounts?realm=&health=&q=` | 按筛选跨页删除 |
| POST | `/api/accounts/import` | 直接提交 JSON（≤ 8 MiB） |
| POST | `/api/accounts/uploads` | 大文件分批上传，返回 `{ batch_id }` |
| GET | `/api/accounts/uploads/{id}` | 批次回执：`{ uploaded, imported, failed, errors[] }` |
| POST | `/api/accounts/export` | `{ ids?: [], realm?, health?, q? }` → 文件流 |
| POST | `/api/accounts/batch` | `{ ids: [], action: enable\|disable\|refresh\|reset\|delete }` |
| POST | `/api/accounts/{id}/refresh` | 刷新目录与余额 |
| POST | `/api/accounts/refresh` | 批量刷新 |
| POST | `/api/accounts/{id}/models/test` | `{ model }` → 单模型链接测试（1 token 探测） |
| POST | `/api/accounts/oauth/start` | `{ realm, proxy_id? }` → `{ state, auth_url }` |
| POST | `/api/accounts/oauth/poll` | `{ state }` → 待授权 / 成功 / 失败 |

**单模型链接测试语义**（易错）：明确的 401（含 HTTP 200 流内 401）立即归入「异常 / 凭证失效」，清旧回避标记，不计回避次数；连接失败与无错误码流失败按每账号策略累计；成功清零未达标计数。手动测试**不刷新凭证、不切代理、不写请求日志**。图像/视频模型不提供该测试（会真实生成内容）。

### 14.4 密钥

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/api/keys` | 列表（`secret` 脱敏，只给前 8 位 + 后 4 位） |
| POST | `/api/keys` | 新建，返回完整 `secret` **一次** |
| PATCH | `/api/keys/{id}` | 改名、号池、模型权限、额度、并发、启用 |
| DELETE | `/api/keys/{id}` | — |
| POST | `/api/keys/{id}/secret` | 明文已存在则直接返回；旧数据补录时提交原文 |
| POST | `/api/keys/{id}/rotate` | 重新生成，旧 Key 立即失效 |

密钥字段见 5.3.5。`restrict_models=true` 时 `models` 非空才有意义。

### 14.5 代理

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/api/proxies` | 列表 + 连接检测结果 |
| POST | `/api/proxies` | 手工新建（HTTP/HTTPS/SOCKS5） |
| POST | `/api/proxies/import` | `{ source }` 单链接 / YAML / Base64 / 订阅 URL → `{ nodes, warnings, duplicates, format }` |
| POST | `/api/proxies/{id}/test` | 连接测试，返回延迟 |
| DELETE | `/api/proxies/{id}` | 被账号绑定时返回 409 并说明 |
| GET | `/api/proxies/bridge` | `{ running, nodes: [{ id, port, status, error }] }` |
| POST | `/api/proxies/bridge/start` | 启动桥接 |
| POST | `/api/proxies/bridge/stop` | 停止桥接 |
| GET | `/api/proxies/clash` | 导出 Mihomo 配置 |
| GET | `/api/proxies/global` | 全局出口状态与分配表 |
| POST | `/api/proxies/global/reshuffle` | 重新随机分配 |

### 14.6 自动化

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/api/tasks` | 计划列表 + 今日完成情况 |
| POST | `/api/tasks` | 新建计划 |
| PATCH | `/api/tasks/{id}` | 改时间、开关、选项 |
| DELETE | `/api/tasks/{id}` | — |
| POST | `/api/tasks/{id}/run` | 立即执行，返回 `run_id` |
| POST | `/api/tasks/{id}/stop` | 终止本轮 |
| POST | `/api/tasks/quick-run` | `{ kind, realm }` 汇总执行 |
| GET | `/api/tasks/runs?task_id=&account=&status=&page=` | 执行记录 |
| DELETE | `/api/tasks/runs` | 清空（需二次确认） |

### 14.7 日志与媒体

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/api/logs` | 调用日志，筛选参数见 11.7.1（时间、模型、密钥、号池、结果、HTTP 状态、业务码、错误类型、错误阶段、归因、错误指纹、账号、出口、重试次数、半路断流、路径、流式、耗时、首字、关键词、客户端） |
| GET | `/api/logs/facets` | 与当前筛选同条件的**分面计数**：每个 `error_kind` / `error_stage` / `attribution` / `http_status` 各多少条，用于渲染筛选侧栏 |
| GET | `/api/logs/errors` | 按指纹聚合的错误分组列表：`{ fingerprint, kind, stage, count, first_at, last_at, accounts[], last_detail }` |
| GET | `/api/logs/{id}` | 单条日志完整详情（含 `error_detail.steps` 全部重试链） |
| GET | `/api/logs/export?...` | CSV 流，列见 11.7.5 |
| DELETE | `/api/logs` | 清空（需二次确认） |
| GET | `/api/media/tasks?realm=&status=&search=&page=` | 视频任务列表 |
| GET | `/api/media/tasks/{id}/content?key_id=` | 直接下载 |
| POST | `/api/media/tasks/{id}/prepare?key_id=` | 准备下载 |
| GET | `/api/media/downloads/{id}` | 一次性下载 |
| GET | `/api/media/pricing` | 媒体积分参考价 |
| GET | `/api/pricing` / `PUT` / `POST /api/pricing/refresh` | 价格表（USD 基准；`PUT` 同时接受 `cost_cny` 输入并换算回 USD） |
| GET | `/api/models` | 全部可用模型（管理视图） |
| GET | `/api/currency` | 当前展示币种、汇率、来源、最后更新时间 |
| PUT | `/api/currency` | `{ display, rate, source }` 修改币种设置 |

### 14.8 系统

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET/PUT | `/api/settings` | 读 / 局部改 |
| GET | `/api/device-token/status` | 设备令牌状态 |
| POST | `/api/device-token/reload` | 重新加载 |
| GET | `/api/quota-probes/status` | 额度巡检状态 |
| GET | `/api/restriction-holds` | 号池风控暂停 |
| POST | `/api/restriction-holds/reset` | 手动清除 |
| GET | `/api/update/status` | 当前版本、检查结果、安装进度 |
| GET | `/api/update/backups` | 本地备份程序 |
| GET | `/api/update/releases` | 可回退版本列表 |
| GET | `/api/update/check` | 触发检查（服务端去重） |
| POST | `/api/update/apply` | `{ version }` 安装 |
| POST | `/api/update/rollback` | `{ version }` 回退 |

### 14.9 网关面

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| POST | `/v1/chat/completions` | OpenAI Chat 兼容 |
| POST | `/v2/chat/completions` | **原生**，字节保真 |
| POST | `/v1/responses` | Responses 兼容 |
| GET | `/v1/responses/{id}` | 取回本地保存的响应 |
| POST | `/v1/messages` | Anthropic Messages 兼容 |
| POST | `/v1/images/generations` | 生图 |
| POST | `/v1/images/edits` | 改图（JSON 或 multipart） |
| POST | `/v1/videos` | Sora 风格创建（JSON 或 multipart） |
| GET | `/v1/videos` | 任务列表（`limit` 1–100、`order` asc/desc、`after` 游标） |
| GET | `/v1/videos/{id}` | 任务状态 |
| GET | `/v1/videos/{id}/content?variant=video` | 下载产物 |
| POST | `/v2/videos/generations` | 原生提交 |
| POST | `/v2/videos/tasks` | 原生查询 |
| GET | `/v1/models` | 模型列表（按 Key 权限过滤） |
| GET | `/v1/media/models?realm=&tag=` | 媒体模型 |
| GET | `/v1/media/tasks` | 当前 Key 的视频任务 |
| GET | `/v1/usage` | 当前 Key 用量 |

`/v1/videos` 上不支持的方法（remix、删除、扩展）返回 501；`thumbnail`、`spritesheet` 返回参数错误。

---

## 15. 前端设计规范

> 这一章写给"审美不稳定"的实现者。**每一个数值都直接抄，不要自己发挥。**

### 15.0 先记住四条

1. **克制。** 一屏只能用**一个**强调色（primary）。除了状态语义（成功/警告/错误）和图表分类色，不许出现第四种颜色。
2. **对齐。** 所有间距必须是 4 的倍数；主要区块间距 24px，卡片内边距 20px，控件间距 12px。**不许出现 13px、17px、23px。**
3. **层级靠表面色，不靠阴影。** 浅色模式下卡片没有投影，只有 1px 的 `outline-variant` 描边 + 稍亮的表面色。投影只留给浮层（对话框、菜单、Toast）。
4. **字重最多两种。** 标题 500，正文 400，数字强调 600。**不要 700、不要全部加粗、不要斜体。**

### 15.0.1 反面清单（做了就是不合格）

| ❌ 不要 | ✅ 应该 |
| --- | --- |
| 紫色渐变背景、玻璃拟态、霓虹发光 | 纯色 surface + 一层 tonal overlay |
| 每个卡片都带 12px 投影 | 描边卡片；只有浮层有投影 |
| 彩虹色图表（七种颜色随手挑） | 每个指标固定一个色相，跨页面一致 |
| 图标 + 文字都染成主色 | 只有**可操作**的元素用主色 |
| 大标题 32px 加粗 700 | 标题 22px / 28px，字重 500 |
| 输入框 1px 灰线 + 直角 | 填充式输入框，圆角 12，聚焦 2px 主色描边 |
| 表格隔行变色 | 悬停用 8% state layer，选中用 primary-container |
| 按钮三个以上并列不同颜色 | 一个填充按钮 + 其余文字按钮 |
| 动效弹跳、旋转、缩放全套 | 只有淡入、位移 8px、高度展开 |
| 加载时页面乱跳 | 骨架屏尺寸 = 实尺寸 |

### 15.1 颜色系统

MD3 语义角色。**只允许使用这些变量，禁止在组件里写十六进制色值。**

#### 15.1.1 浅色（默认）

```css
:root {
  --md-primary: #0062F3;
  --md-on-primary: #FFFFFF;
  --md-primary-container: #E1E0FF;
  --md-on-primary-container: #001C51;

  --md-secondary: #545C86;
  --md-on-secondary: #FFFFFF;
  --md-secondary-container: #DEE1FF;
  --md-on-secondary-container: #0E1E42;

  --md-tertiary: #00788C;
  --md-on-tertiary: #FFFFFF;
  --md-tertiary-container: #ABEDFF;
  --md-on-tertiary-container: #00242B;

  --md-error: #B32537;
  --md-on-error: #FFFFFF;
  --md-error-container: #FFDAD8;
  --md-on-error-container: #480009;

  --md-surface: #FCFCFF;
  --md-on-surface: #1F1F25;
  --md-on-surface-variant: #454556;
  --md-surface-dim: #DCDCE4;
  --md-surface-container-lowest: #FFFFFF;
  --md-surface-container-low: #F6F6FD;
  --md-surface-container: #F0F0F8;
  --md-surface-container-high: #EBEAF2;
  --md-surface-container-highest: #E5E5EC;

  --md-outline: #767588;
  --md-outline-variant: #C5C5D9;
  --md-inverse-surface: #303036;
  --md-inverse-on-surface: #F0F0F8;
  --md-inverse-primary: #C1C1FF;
}
```

#### 15.1.2 深色

```css
[data-theme="dark"] {
  --md-primary: #C1C1FF;
  --md-on-primary: #002B74;
  --md-primary-container: #003FA3;
  --md-on-primary-container: #E1E0FF;

  --md-secondary: #BDC4F4;
  --md-on-secondary: #232E55;
  --md-secondary-container: #3B446D;
  --md-on-secondary-container: #DEE1FF;

  --md-tertiary: #00D9FD;
  --md-on-tertiary: #003640;
  --md-tertiary-container: #004E5C;
  --md-on-tertiary-container: #ABEDFF;

  --md-error: #FFB3B0;
  --md-on-error: #680016;
  --md-error-container: #920023;
  --md-on-error-container: #FFDAD8;

  --md-surface: #131318;
  --md-on-surface: #E2E2E9;
  --md-on-surface-variant: #C5C5D9;
  --md-surface-dim: #131318;
  --md-surface-container-lowest: #0E0D14;
  --md-surface-container-low: #1B1B20;
  --md-surface-container: #1F1F25;
  --md-surface-container-high: #2A292F;
  --md-surface-container-highest: #34343A;

  --md-outline: #908FA2;
  --md-outline-variant: #454556;
  --md-inverse-surface: #E2E2E9;
  --md-inverse-on-surface: #303036;
  --md-inverse-primary: #0062F3;
}
```

已核对的对比度（WCAG AA）：

| 前景 / 背景 | 浅色 | 深色 |
| --- | --- | --- |
| on-primary / primary | 5.20 | 7.72 |
| on-primary-container / primary-container | 12.69 | 7.28 |
| on-surface / surface | 16.01 | 14.36 |
| on-surface-variant / surface | 9.16 | 10.90 |
| outline / surface | 4.39 | 5.85 |

#### 15.1.3 状态与业务色（固定，跨页一致）

```css
:root {
  --status-ok: #14804A;        --status-ok-soft: #E3F6EC;
  --status-warn: #8A5A00;      --status-warn-soft: #FFF1D6;
  --status-danger: var(--md-error);  --status-danger-soft: var(--md-error-container);
  --status-idle: #5B5B6B;      --status-idle-soft: #EEEEF4;
  --status-avoid: #6B3FA0;     --status-avoid-soft: #F0E9FB;   /* 回避，紫色 */

  --realm-cn: #C4561D;         --realm-cn-soft: #FFF0E6;       /* 国内，橙 */
  --realm-intl: #5B3FD4;       --realm-intl-soft: #F1EDFF;     /* 国际，紫 */
}
```

图表色（**每个指标固定一个，跨页面不许换**）：

```css
:root {
  --chart-request:  #0062F3;   /* 请求数 */
  --chart-tokens:   #7C3AED;   /* 输出 Token */
  --chart-tokens-in:#0891B2;   /* 输入 Token */
  --chart-cache:    #14804A;   /* 缓存 / 可用积分 */
  --chart-cost:     #B45309;   /* 费用 */
  --chart-credit:   #C2410C;   /* 已消耗积分 */
  --chart-error:    #B32537;   /* 失败 */
  --chart-latency:  #6D28D9;   /* 延迟 */
}
```

#### 15.1.4 状态层（state layer）

MD3 的交互反馈靠叠加半透明层，不靠改颜色：

```css
--state-hover:    color-mix(in srgb, currentColor 8%, transparent);
--state-focus:    color-mix(in srgb, currentColor 10%, transparent);
--state-pressed:  color-mix(in srgb, currentColor 10%, transparent);
--state-dragged:  color-mix(in srgb, currentColor 16%, transparent);
```

规则：**可点击的东西必须有 hover 与 pressed；不可点击的东西不许有 hover。**

### 15.2 字体与排版

```css
:root {
  --font-display: "Outfit Variable", "HarmonyOS Sans SC", "MiSans", "PingFang SC",
                  "Microsoft YaHei UI", "Noto Sans SC", sans-serif;
  --font-text:    "HarmonyOS Sans SC", "MiSans", "PingFang SC",
                  "Microsoft YaHei UI", "Noto Sans SC", system-ui, sans-serif;
  --font-mono:    "JetBrains Mono", "Cascadia Code", "SF Mono", Menlo, monospace;
}
```

字号阶梯（**只有这 12 个，不许自创**）：

| Token | 字号 / 行高 / 字距 | 字重 | 用途 |
| --- | --- | --- | --- |
| `display-l` | 45 / 52 / -0.25 | 500 | 只有登录页的品牌字 |
| `display-m` | 36 / 44 / 0 | 500 | 空状态大标题 |
| `headline-l` | 28 / 36 / 0 | 500 | 页面标题 |
| `headline-m` | 24 / 32 / 0 | 500 | 区块大标题 |
| `title-l` | 22 / 28 / 0 | 500 | 卡片组标题 |
| `title-m` | 16 / 24 / 0.15 | 500 | 卡片标题 |
| `title-s` | 14 / 20 / 0.1 | 500 | 列表分组标题 |
| `body-l` | 16 / 24 / 0.5 | 400 | 正文（大屏） |
| `body-m` | 14 / 20 / 0.25 | 400 | 正文默认 |
| `body-s` | 12 / 16 / 0.4 | 400 | 辅助文字 |
| `label-l` | 14 / 20 / 0.1 | 500 | 按钮、标签页 |
| `label-m` | 12 / 16 / 0.5 | 500 | 徽标、Chip |
| `metric-xl` | 34 / 40 / -0.5 | 600 | KPI 主值 |
| `metric-l` | 28 / 34 / -0.25 | 600 | 卡片主数字 |
| `metric-m` | 20 / 26 / 0 | 600 | 次级数字 |

数字规则：

- 所有数值 **MUST** 加 `font-variant-numeric: tabular-nums`，否则表格列会抖。
- 金额与 Token 用 `--font-display`（Outfit 的数字更好看），正文用 `--font-text`。
- ID、模型名、路径用 `--font-mono`，字号降一档。

行宽限制：正文段落 `max-width: 68ch`。超过就换行，别拉成一条长线。

### 15.3 形状、层级、间距

#### 15.3.1 圆角阶梯（只许用这六个）

```css
--shape-xs:   4px;    /* 徽标、小 chip */
--shape-sm:   8px;    /* 输入框、按钮 */
--shape-md:  12px;    /* 卡片、列表项 */
--shape-lg:  16px;    /* 大卡片、面板 */
--shape-xl:  28px;    /* 对话框 */
--shape-full: 999px;  /* 胶囊、头像、开关 */
```

规则：**在同一屏里，最大的容器圆角必须大于等于其内部元素。** 不要出现外面 8px、里面 16px 的倒挂。

#### 15.3.2 层级（MD3 elevation）

浅色模式下**不用投影表达层级**，用表面色：

| 层级 | 表面 | 用在哪 |
| --- | --- | --- |
| 0 | `surface` | 页面底色 |
| 1 | `surface-container-low` | 页面内的分区带 |
| 2 | `surface-container` | 卡片 |
| 3 | `surface-container-high` | 卡片内的子卡片、表头 |
| 4 | `surface-container-highest` | 输入框、悬浮工具条 |

投影**只给浮层**：

```css
--elev-1: 0 1px 2px rgb(0 0 0 / .06), 0 1px 3px 1px rgb(0 0 0 / .08);   /* 菜单 */
--elev-2: 0 2px 6px 2px rgb(0 0 0 / .08), 0 1px 2px rgb(0 0 0 / .10);   /* 悬浮卡 */
--elev-3: 0 4px 8px 3px rgb(0 0 0 / .10), 0 1px 3px rgb(0 0 0 / .12);   /* 对话框 */
```

深色模式下把黑色换成 `rgb(0 0 0 / .5)` 且不透明度乘 1.5。

#### 15.3.3 间距

```css
--space-1:  4px;   --space-2:  8px;   --space-3: 12px;
--space-4: 16px;   --space-5: 20px;   --space-6: 24px;
--space-8: 32px;   --space-10: 40px;  --space-12: 48px;
```

固定用法（照抄）：

| 位置 | 值 |
| --- | --- |
| 页面左右内边距 | 32px（≤1199px 时 24px，≤760px 时 16px） |
| 页面顶部内边距 | 24px |
| 页面标题到内容 | 24px |
| 卡片内边距 | 20px |
| 卡片标题到内容 | 16px |
| 卡片之间的间距 | 16px |
| 表单行之间 | 16px |
| 表单标签到控件 | 8px |
| 按钮内边距 | 水平 20px、垂直 10px |
| 表格行高 | 56px（紧凑模式 44px） |
| 表格表头高 | 48px |

**永远不要出现 `padding: 13px`、`margin: 18px`、`gap: 10px` 这类数字。**

#### 15.3.4 栅格

- 内容最大宽度 `1480px`，居中。
- 12 列栅格，列间距 16px。
- 断点：`600` / `905` / `1240` / `1440`（MD3 的 compact / medium / expanded / large）。

KPI 卡片：≥1440 四列，≥1240 三列，≥905 两列，<905 一列。
仪表盘主区：≥1240 两列（7:5），<1240 单列。

### 15.4 组件规范

#### 15.4.1 按钮

| 类型 | 外观 | 用途 | 每屏最多 |
| --- | --- | --- | --- |
| Filled | `primary` 底 + `on-primary` 字，圆角 `--shape-full` | 主操作 | 1 个 |
| Tonal | `secondary-container` 底 | 次主操作 | 2 个 |
| Outlined | 1px `outline` 边 | 中性操作 | 不限 |
| Text | 无底无边，`primary` 字 | 行内操作 | 不限 |
| Icon | 40×40 圆形，只有图标 | 工具条 | 不限 |
| FAB | 56×56，`primary-container` 底，右下角，`--elev-3` | 页面级新建 | 每页 1 个 |

状态：`hover` 叠 8%，`pressed` 叠 10%，`disabled` 用 38% 不透明度且**不响应 hover**。
加载中：图标换成 16px 旋转环，文字保留，**宽度不许跳**。

#### 15.4.2 输入框

MD3 **填充式**（不是描边式）：

```
背景 surface-container-highest，圆角 --shape-sm 8px（顶部圆角，底部 0）
底部 1px on-surface-variant
聚焦时底部 2px primary，标签浮动到左上
错误时底部 2px error + 下方 12px 错误文案
```

高度 56px。标签永远浮在框内，**不要**把 label 放在框上方再加 placeholder（那是 2015 年的做法）。

数字输入：右对齐，带单位后缀（`Token` / `积分` / `次/分钟` / `秒`）。

#### 15.4.3 下拉选择

不用原生 `<select>`。自绘：`surface-container-high` 底，点击后弹出 `surface-container` 面板 + `--elev-1`，圆角 `--shape-md`，选项高 48px，选中项左侧 3px `primary` 竖条 + `primary-container` 背景。

#### 15.4.4 开关 / 勾选框 / 单选

- 开关：52×32 轨道，`outline` → `primary`，滑块 24px，位移 20px，**180ms** `--ease-emphasized`。
- 勾选框：18×18，圆角 2px，选中时 `primary` 填充 + 白色对勾。
- 单选：20×20 圆，选中时中间 10px 实心点。

#### 15.4.5 卡片

```
背景 surface-container
圆角 --shape-md (12px)
内边距 20px
无投影
1px outline-variant 描边（可选，只有需要和同色背景区分时才加）
```

卡片标题行：左标题 `title-m`，右侧放操作（图标按钮或文字按钮），标题与操作之间 `space-between`。
**不许**每张卡片都加阴影、不许卡片套卡片超过两层。

#### 15.4.6 表格

```
表头：surface-container-high，48px 高，label-m，on-surface-variant
行：56px 高，底部 1px outline-variant（只有行之间有线，表头下方线加粗到 1px on-surface-variant）
悬停：state-hover 8%
选中：primary-container 背景，左侧 3px primary 竖条
```

规则：

- 数字列右对齐，文本列左对齐，操作列固定右侧。
- 列宽用 `<colgroup>` 固定，不许自动撑开导致翻页时跳动。
- 超过 8 列时，前两列（账号/名称）固定左侧，操作列固定右侧，中间横向滚动。
- 空状态占满表格区域，居中显示图标 + 一句话 + 一个操作按钮。
- 加载中用骨架行，行高与真实行一致。

#### 15.4.7 Chip / 徽标

```
高度 24px（小）/ 32px（标准）
圆角 --shape-full
label-m
```

状态 chip 配色（**必须**用语义变量，不许临时调色）：

| 状态 | 背景 | 文字 |
| --- | --- | --- |
| 健康 | `--status-ok-soft` | `--status-ok` |
| 限流 | `--status-warn-soft` | `--status-warn` |
| 异常 | `--status-danger-soft` | `--status-danger` |
| 回避 | `--status-avoid-soft` | `--status-avoid` |
| 未探测 | `--status-idle-soft` | `--status-idle` |
| 国内版 | `--realm-cn-soft` | `--realm-cn` |
| 国际版 | `--realm-intl-soft` | `--realm-intl` |

#### 15.4.8 对话框

```
遮罩 rgb(0 0 0 / .32)
面板 surface-container-high，圆角 --shape-xl (28px)，--elev-3
最大宽度 560px（普通）/ 880px（宽）/ 1200px（全屏表格类）
内边距 24px
标题 title-l，右上角关闭图标
底部操作栏右对齐，间距 8px
```

打开动画：遮罩 150ms 淡入；面板 200ms 从 `scale(.96)` + `translateY(8px)` 到正常，缓动 `--ease-emphasized-decelerate`。
关闭：150ms 反向。
**不许**从屏幕边缘飞入，**不许**弹跳。

#### 15.4.9 Toast / Snackbar

```
底部居中（手机上在底部安全区上方 16px）
surface-inverse 背景，on-inverse-surface 文字
圆角 --shape-sm
最小高度 48px，左内边距 16，右内边距 8
最多同时 3 条，超出排队
4 秒自动消失，悬停暂停
```

类型：成功 / 失败 / 提示。失败时左侧加 4px `error` 竖条，**不要**整条变红。

#### 15.4.10 标签页

二级切换用 Segmented（胶囊），三级用 Tabs（下划线）。

```
Segmented：高 40px，圆角 full，选中项 primary-container 底 + on-primary-container 字，
           指示块用 200ms --ease-emphasized 位移
Tabs：高 48px，选中项底部 3px primary 指示条，文字 primary
```

#### 15.4.11 图表（原生 SVG，不许用图表库）

- 折线：2px 宽，圆头圆角连接；面积填充用同色 12% 不透明度。
- 面积图最多两层叠加，超过就换成堆叠条。
- 网格线：1px `outline-variant`，**只画水平线**，60% 不透明度。
- 轴标签：`body-s`，`on-surface-variant`。
- 数据点不在线上画圆点（除非悬停）。
- 悬停：垂直参考线 + 浮层卡片（`--elev-2`），显示该点所有系列的值。
- 空数据：显示"暂无数据"文字，**不要**画一条假线。
- 动画：路径 `stroke-dasharray` 绘制 600ms `--ease-emphasized-decelerate`，**只在首屏播放一次**，刷新不重播。

#### 15.4.12 代码 / ID 展示

模型名、密钥前缀、UUID 用 `--font-mono` + `surface-container-high` 背景 + `--shape-xs` 圆角 + 4px 6px 内边距。
超长时中间省略（`abc1…9xyz`），悬停显示完整值，点击复制。

#### 15.4.13 骨架屏

```
背景 surface-container-high
圆角同真实元素
20px 高度单位
shimmer 动画：linear-gradient 120deg，1.4s 无限，background-position 从 -200% 到 200%
```

**尺寸必须等于真实元素**，否则加载完会跳。
`prefers-reduced-motion` 时改用 1.2s 的透明度呼吸（0.6 ↔ 1）。

### 15.5 动效（纯 CSS）

#### 15.5.1 令牌

```css
:root {
  /* MD3 缓动曲线 */
  --ease-standard:            cubic-bezier(0.2, 0, 0, 1);
  --ease-standard-decelerate: cubic-bezier(0, 0, 0, 1);
  --ease-standard-accelerate: cubic-bezier(0.3, 0, 1, 1);
  --ease-emphasized:          cubic-bezier(0.2, 0, 0, 1);
  --ease-emphasized-decelerate: cubic-bezier(0.05, 0.7, 0.1, 1);
  --ease-emphasized-accelerate: cubic-bezier(0.3, 0, 0.8, 0.15);
  --ease-linear:             linear;

  /* 时长 */
  --dur-short1: 50ms;   --dur-short2: 100ms;  --dur-short3: 150ms;  --dur-short4: 200ms;
  --dur-med1: 250ms;    --dur-med2: 300ms;    --dur-med3: 350ms;    --dur-med4: 400ms;
  --dur-long1: 450ms;   --dur-long2: 500ms;   --dur-long3: 550ms;   --dur-long4: 600ms;
}
```

**用哪条曲线**：

| 场景 | 曲线 | 时长 |
| --- | --- | --- |
| 元素出现（淡入、展开） | `--ease-emphasized-decelerate` | 300ms |
| 元素消失（淡出、收起） | `--ease-emphasized-accelerate` | 200ms |
| 位置移动（指示块、滑块） | `--ease-emphasized` | 200ms |
| 颜色 / 背景变化 | `--ease-standard` | 150ms |
| 数值滚动 | `--ease-standard` | 200ms |
| 加载旋转 | `--ease-linear` | 900ms |

#### 15.5.2 动画清单（只许用这些）

```css
/* 1. 页面进入：整页淡入 + 上移 8px */
@keyframes page-in {
  from { opacity: 0; transform: translateY(8px); }
  to   { opacity: 1; transform: none; }
}
.page-enter { animation: page-in var(--dur-med2) var(--ease-emphasized-decelerate); }

/* 2. 卡片列表错峰进入（最多延迟 6 个，每个 +40ms） */
.stagger > * { animation: page-in var(--dur-med2) var(--ease-emphasized-decelerate) both; }
.stagger > *:nth-child(1) { animation-delay:   0ms; }
.stagger > *:nth-child(2) { animation-delay:  40ms; }
.stagger > *:nth-child(3) { animation-delay:  80ms; }
.stagger > *:nth-child(4) { animation-delay: 120ms; }
.stagger > *:nth-child(5) { animation-delay: 160ms; }
.stagger > *:nth-child(6) { animation-delay: 200ms; }
.stagger > *:nth-child(n+7) { animation-delay: 240ms; }

/* 3. 高度展开 / 收起 */
@keyframes expand { from { grid-template-rows: 0fr; opacity: 0; } to { grid-template-rows: 1fr; opacity: 1; } }
.collapse-enter { display: grid; animation: expand var(--dur-med1) var(--ease-emphasized-decelerate); }

/* 4. 骨架屏微光 */
@keyframes shimmer { from { background-position: -200% 0; } to { background-position: 200% 0; } }
.skeleton {
  background: linear-gradient(120deg, var(--md-surface-container-high) 35%,
              var(--md-surface-container-highest) 50%, var(--md-surface-container-high) 65%);
  background-size: 200% 100%;
  animation: shimmer 1.4s var(--ease-linear) infinite;
}

/* 5. 加载环 */
@keyframes spin { to { transform: rotate(1turn); } }
.spinner { animation: spin 900ms var(--ease-linear) infinite; }

/* 6. 数字滚动（只对整数部分做位移动画） */
@keyframes tick-up   { from { transform: translateY(60%); opacity: 0; } to { transform: none; opacity: 1; } }
.num-tick { animation: tick-up var(--dur-short4) var(--ease-standard); }

/* 7. 行删除：淡出 + 高度收拢 */
@keyframes row-leave { to { opacity: 0; transform: translateX(-8px); } }
.row-leaving { animation: row-leave var(--dur-short3) var(--ease-standard-accelerate) forwards; }

/* 8. 状态点脉冲（仅用于"正在处理"，最多 3 个同时在动） */
@keyframes pulse-ring {
  0%   { transform: scale(.8); opacity: .8; }
  100% { transform: scale(2.2); opacity: 0; }
}
.status-pulse::after {
  content: ""; position: absolute; inset: 0; border-radius: 50%;
  background: currentColor; animation: pulse-ring 1.6s var(--ease-standard) infinite;
}

/* 9. 图表路径绘制 */
@keyframes draw { to { stroke-dashoffset: 0; } }
```

#### 15.5.3 硬规则

1. **只对 `transform` 和 `opacity` 做动画。** 不许动画 `width` / `height` / `top` / `left` / `box-shadow`（会把帧率拖死）。高度变化用 `grid-template-rows: 0fr → 1fr`。
2. **同一个元素最多一个动画。** 不要叠加。
3. **进入用 decelerate，退出用 accelerate。** 反过来会显得笨重。
4. **`prefers-reduced-motion: reduce` 时**：所有非必要动画降为 1ms，骨架屏改透明度呼吸，图表不播放绘制动画。
5. **不许无限循环的装饰性动画**（旋转 LOGO、漂浮图形、背景粒子）。
6. **页面切换不超过 300ms。** 超过就会觉得卡。
7. 列表超过 50 行时 **关闭** 错峰进入动画，否则滚动会掉帧。

#### 15.5.4 状态过渡

- 数字变化：200ms 滚动，不对整个数字块做淡入淡出。
- 颜色变化：150ms。**不许**从红渐变到绿（会变成棕色），用瞬切 + 200ms 的透明度过渡。
- 展开详情：高度 + 内容淡入，300ms。
- 筛选结果替换：先 100ms 淡出旧行，再 200ms 淡入新行；**不要**让表格整体闪烁。

#### 15.5.5 焦点环

```css
:focus-visible {
  outline: 2px solid var(--md-primary);
  outline-offset: 2px;
  border-radius: inherit;
}
```

用 `:focus-visible`，不要用 `:focus`（鼠标点击也会描边，很丑）。**不许** `outline: none` 而不给替代。

### 15.6 页面布局

#### 15.6.0 外壳

```
┌────────────┬──────────────────────────────────────────────────────────────┐
│            │  ┌────────────────────────────────────────────────────────┐  │
│  ▌品牌      │  │ 页面标题 28/500            [筛选] [筛选] [+ 主要操作]  │  │  64px
│            │  └────────────────────────────────────────────────────────┘  │
│  ● 仪表盘   │                                                              │
│  ○ 号池     │       内容区（最大 1480px，左右 32px）                        │
│  ○ 自动化   │                                                              │
│  ○ 代理     │                                                              │
│  ○ 密钥     │                                                              │
│  ○ 日志     │                                                              │
│  ○ 设置     │                                                              │
│            │                                                              │
│  ──────    │                                                              │
│  版本 1.2.30│                                                              │
│  退出       │                                                              │
└────────────┴──────────────────────────────────────────────────────────────┘
   252px 固定
```

- 侧边栏：`surface-container-low` 背景，宽度 252px，固定不滚动（内部滚动）。
- 导航项：高 48px，圆角 `--shape-full`，左内边距 16px，图标 20px + 12px 间距 + 文字 `label-l`。
- 选中态：`primary-container` 背景 + `on-primary-container` 文字 + 左侧 3px? **不**，MD3 是整块圆角背景，**不要**再加竖条。
- 选中指示块用绝对定位的 `rail-pill`，`transform: translateY()` + `height` 过渡 200ms。**只有一个元素在动。**
- 悬停：state-hover 8%。
- 底部：版本号 `body-s` + 退出按钮。

响应式：
- ≥1240px：常驻侧边栏
- 905–1240px：收窄到 88px，只显示图标，悬停出 tooltip
- <905px：隐藏，顶部汉堡打开抽屉（遮罩 + 从左滑入 250ms）

#### 15.6.1 仪表盘

```
页面标题区
┌────────────────────────────────────────────────────────────────┐
│ 仪表盘                                      [号池 ▾] [范围 ▾] [刷新] │
│ 账号、用量和消耗，一眼看清。                                       │
└────────────────────────────────────────────────────────────────┘

今日快照条（11.9.3，任何范围都显示）
┌────────────────────────────────────────────────────────────────┐
│ 今日 · 2026-11-27                                                │
│ Token 输入 128.4K 输出 42.1K 缓存 96.3K（命中 64.2%）             │
│ 等价 API ¥18.42   积分 312.5   请求 1,204   失败 3（0.25%）       │
└────────────────────────────────────────────────────────────────┘

[总览] [用量统计]                                   数据更新于 12:04:31
┌───────────────────────┬───────────────────────┐
│ 号池状态               │ 积分与价值             │
│ ┌─────┬─────┬─────┐   │ 总额  剩余  已用       │
│ │ 国内 │ 国际 │ 全部 │   │ 1.2万  9,800  2,200   │
│ └─────┴─────┴─────┘   │ ¥ 未知（无兑换率）      │
│ 健康 42 限流 3 异常 2  │ 即将过期 ▸ 3 个资源包   │
└───────────────────────┴───────────────────────┘

KPI 七卡（每卡：主值 = 所选范围，副行 = 今日 + 环比）
┌──────────┬──────────┬──────────┬──────────┐
│ 请求数    │ Token    │ 缓存命中  │ 费用      │
│ 12,480   │ 5.42M    │ 64.2%    │ ¥18.42   │
│ 今日1,204 │ 今日 0.5M │ 今日 65%  │ 今日 ¥1.8 │
└──────────┴──────────┴──────────┴──────────┘
┌──────────┬──────────┬──────────┐
│ 积分消耗  │ 失败率    │ TPS      │
│ 3,118.6  │ 0.42%    │ 42.3     │
│ 今日 312  │ 今日 0.25%│ 今日 44  │
└──────────┴──────────┴──────────┘

趋势（占满宽度，高 320px）
┌──────────────────────────────────────────────────────────────┐
│ 用量趋势            [请求][Token][费用][积分][失败]            │
│ 范围内 12,480 · 今日 1,204                                    │
│   ╱╲╱╲╱╲  实线=所选范围  虚线=近 30 天日均                    │
└──────────────────────────────────────────────────────────────┘

账号排行（左 7）           模型排行（右 5）
┌───────────────────┬──────────────────┐
│ 账号排行           │ 模型排行          │
│ 1 小号A  3,120 请求│ 1 glm-5    42%   │
└───────────────────┴──────────────────┘
```

**「用量统计」页签**追加三块：Token 构成（输入/输出/缓存 堆叠条）、响应质量（成功率/首字/延迟分布）、24 小时热力（按小时 × 号池）。

关键约束：

- 卡片高度对齐，允许内容少的卡片底部留白。
- 主数字 `metric-xl`，单位用 `body-m` 附在数字右侧、基线对齐，颜色 `on-surface-variant`。
- 空数据时数字显示 `—`，不是 `0`，并在卡片下方写"暂无记录"。
- 失败率涨是坏事：副行箭头向上时用 `--status-danger`；请求数涨是好事：用 `--status-ok`。

#### 15.6.2 号池管理

```
┌──────────────────────────────────────────────────────────────────────┐
│ 号池管理                                        [导入凭证] [+ 添加账号] │
│ 共 47 个账号 · 健康 42 · 限流 3 · 异常 2                              │
└──────────────────────────────────────────────────────────────────────┘

筛选栏（粘贴在标题下方，不是浮动卡片）
┌──────────────────────────────────────────────────────────────────────┐
│ [全部 47][健康 42][限流 3][回避 0][异常 2][未探测 0]  [搜索______] [状态▾]│
└──────────────────────────────────────────────────────────────────────┘

批量操作条（仅有选中时出现，从上方推入 200ms）
┌──────────────────────────────────────────────────────────────────────┐
│ 已选 3 个  [启用][停用][刷新][重置状态][导出][删除]        取消选择    │
└──────────────────────────────────────────────────────────────────────┘

┌─┬──────────────┬─────────────┬────────┬────────┬──────────┬────┬────┐
│☐│ 账号          │ 状态·每日任务 │ 可用积分│ 模型    │ 出口代理  │启用│ ⋮ │
├─┼──────────────┼─────────────┼────────┼────────┼──────────┼────┼────┤
│☐│ 小号A         │ ● 健康  3/4 │ 1,240  │ 12 个  │ 香港01 ·全局│ ● │ ⋮ │
│ │ cn · 138****00│  ▣▣▣▢       │        │        │          │    │    │
└─┴──────────────┴─────────────┴────────┴────────┴──────────┴────┴────┘
```

- 状态列：圆点（8px）+ 状态文字 + 每日任务进度（四个小方块，完成填实）。
- 积分列：数字右对齐 + 一条 4px 高的进度条（已用/总额）。
- 模型列：显示数量，悬停弹出模型 chip 列表。
- 行操作：`⋮` 打开菜单（详情 / 编辑 / 测试 / 刷新 / 删除）。
- 详情抽屉：右侧滑入 480px，`surface-container-low`，含模型卡片网格、限额、运行日志。

移动端（<905px）：表格换成两行卡片列表：

```
┌──────────────────────────────────────┐
│ 小号A            ● 健康      [ ⋮ ]   │
│ cn · 积分 1,240 · 模型 12 · 香港01    │
└──────────────────────────────────────┘
```

#### 15.6.3 自动化任务

```
┌────────────────────────────────────────────────────────────────────┐
│ 自动化任务                                  [一键执行] [+ 新建计划]  │
└────────────────────────────────────────────────────────────────────┘
[每日任务] [一次性福利] [账号维护] [执行记录]

┌ 计划卡 ─────────────────────────────────────────────────────────────┐
│ ▌每日签到  国内版        09:00 · 21:00      [开关]  [运行]  [终止]   │
│  已选 42 个账号 · 今日完成 38/42 · 下次 21:00                       │
│  ████████████████████░░░░  90%                                      │
└─────────────────────────────────────────────────────────────────────┘
```

- 计划卡：左侧 4px 色条表示状态（运行中 primary / 完成 ok / 失败 danger / 暂停 idle）。
- 时间选择：小时 + 分钟两个数字选择器，旁边「保存时间」按钮；**保存时间不会自动启用**，开关是独立的。
- 进度条：4px 高，圆角 full，`primary` 填充，涨落 400ms。
- 执行记录表格：时间 / 账号 / 计划 / 结果 / 说明 / 凭证确认时间。结果用 chip。
- 「终止」是危险操作，二次确认，文案写清"只取消本轮，已完成的操作不会撤销"。

#### 15.6.4 代理管理

```
┌────────────────────────────────────────────────────────────────────┐
│ 代理管理                       [导入订阅] [启动桥接] [导出配置] [+]  │
└────────────────────────────────────────────────────────────────────┘

┌ 全局出口 ───────────────────────────────────────────────────────────┐
│ 所有凭证请求通过代理  [开关]                                         │
│ 通用代理  [随机化跟随 ▾]                        [重新随机分配]        │
└─────────────────────────────────────────────────────────────────────┘

│ 名称        │ 类型   │ 地址              │ 延迟  │ 状态    │ 绑定 │ ⋮ │
│ 香港 01     │ SS     │ hk1.example:443   │ 68ms  │ ● 可用  │ 12   │ ⋮ │
│ 日本 02     │ VLESS  │ jp2.example:443   │ —     │ ● 需桥接│ 3    │ ⋮ │
```

- 「需桥接」chip 用 `tertiary-container`；桥接未启动时整行 60% 不透明度。
- 延迟测试中显示旋转小环 + 骨架数字。
- 导入结果：关闭弹窗后，页面顶部出现可折叠的警告条（不是 Toast，因为可能很长）。

#### 15.6.5 密钥管理

```
┌────────────────────────────────────────────────────────────────────┐
│ 密钥管理                                                        [+]  │
│ 网关地址  http://127.0.0.1:3040/v1                        [复制]     │
└────────────────────────────────────────────────────────────────────┘

┌ 密钥卡 ──────────────────────────────┐  ┌ 密钥卡 ─────────────────┐
│ Cherry Studio            ● 启用       │  │ Claude Code    ● 启用   │
│ sk-wbp-7f3a…c91d          [复制] [👁] │  │ sk-wbp-2b81…4e0a        │
│ 国内版 · 模型 3 个 · 今日 1.2K 请求    │  │ 全部号池 · 全模型        │
│ Token ████████░░░░ 78%                │  │ Token ██░░░░░░░░ 12%     │
│ [编辑] [轮换] [删除]                   │  │ [编辑] [轮换] [删除]     │
└───────────────────────────────────────┘  └─────────────────────────┘
```

- 密钥卡片两列（≥1240），一列（<1240）。
- 明文默认打码，点眼睛显示 10 秒后自动收回。
- 「轮换」二次确认，写明"旧密钥立即失效"。
- 新建密钥后卡片插入到第一位并播放一次 400ms 的高亮脉冲（`primary-container` → 透明）。
- 每张卡片有「导入到 CC Switch」按钮：用 `ccswitch://` deeplink 把当前密钥作为服务商导入本机 CC Switch。弹窗里可编辑服务商显示名、选择模型（默认只列非 `cmcc:` 的可用模型），并提示"需要本机已安装 CC Switch，导入后在应用中确认启用"。该功能**只生成链接**，不上传任何数据。

#### 15.6.6 调用日志（重点页面）

```
┌──────────────────────────────────────────────────────────────────────────┐
│ 调用日志                                          [导出 CSV] [清空]        │
└──────────────────────────────────────────────────────────────────────────┘

┌ 筛选面板（可折叠，默认展开一行快捷筛选）──────────────────────────────────┐
│ [时间 ▾ 近7天] [号池 ▾ 全部] [结果 ▾ 全部] [错误类型 ▾] [搜索______] [更多▾] │
└──────────────────────────────────────────────────────────────────────────┘
   展开后：
┌──────────────────────────────────────────────────────────────────────────┐
│ HTTP状态 [200] [4xx] [5xx]   业务码 [____]   阶段 ▾   归因 ▾              │
│ 账号 ▾   出口 ▾   重试次数 [0]-[10]   □ 只看半路断流                       │
│ 耗时 [__]ms — [__]ms   首字 [__]ms — [__]ms                               │
│ 已启用 4 个条件                                    [重置] [应用]           │
└──────────────────────────────────────────────────────────────────────────┘

┌ 错误聚合（仅当有失败时出现，可折叠）──────────────────────────────────────┐
│ 这 7 天共 128 次失败，归为 5 类：                                         │
│ ● upstream_code=11128  渠道指纹拦截  82 次 · 3 个账号 · 首次 11-21 09:12  │
│ ● credential           凭证失效      21 次 · 2 个账号 · 首次 11-23 14:02  │
│ ● timeout_header       等待超时      18 次 · 5 个账号 · 首次 11-24 03:41  │
│   点击任意一行 → 自动套用该指纹并过滤                                     │
└──────────────────────────────────────────────────────────────────────────┘

┌────┬──────────┬──────────────┬──────┬──────┬─────┬──────┬──────┬────────┐
│时间 │ 密钥      │ 模型          │状态  │耗时  │首字 │Token │费用  │ ⋮      │
├────┼──────────┼──────────────┼──────┼──────┼─────┼──────┼──────┼────────┤
│12:04│Cherry    │ glm-5        │ 200  │1.2s  │320ms│1.4K  │¥0.02 │ 展开   │
│12:03│Cherry    │ glm-5        │ 429  │—     │—    │—     │—     │ 展开   │
└────┴──────────┴──────────────┴──────┴──────┴─────┴──────┴──────┴────────┘
```

展开某一行的详情（行内下拉，300ms 高度展开）：

```
┌──────────────────────────────────────────────────────────────────────────┐
│ 请求 01J8ZK… · /v1/chat/completions · 流式 · 号池 cn · 账号 小号A          │
│                                                                          │
│ 错误 ● 上游限流                                            [查看原始响应] │
│   类型 rate_limit_model · 阶段 header · 归因 account · 计入账号 否         │
│   HTTP 429 · 业务码 14003 · 重试 2 次 · 重置时间 12:05:00                 │
│   出口 香港01 · 上游消息 "请求过于频繁，请稍后再试"                         │
│                                                                          │
│ 重试链                                                                   │
│   #0 小号A  香港01  429/14003  header  820ms   未发出首字                  │
│   #1 小号B  日本02  429/14003  header  610ms   未发出首字                  │
│   #2 小号C  直连    200        stream  1.2s   成功                        │
└──────────────────────────────────────────────────────────────────────────┘
```

设计规则：

- **筛选栏永远是页面的一部分**，不是弹窗。用户要能一边改筛选一边看结果。
- 错误类型用多选 chip，选中后 chip 变成对应的语义色（不是统一的蓝色）。
- 激活的筛选条件必须**可见且可单独移除**（每个条件一个小 chip 带 ×）。
- 表格里失败的行的状态列用 `error` 色的 chip，**不要**整行变红。
- 「错误聚合」条让人一眼知道问题集中在哪，这是排查效率的关键。
- 折叠行详情里，重试链用三列对齐（账号 / 出口 / 结果），一眼看清换了几个号。
- 分页：每页 20 / 50 / 100 可选，底部居中，显示"第 3 页 / 共 128 页 · 2,540 条"。

移动端：表格改为卡片列表，每张卡三行（时间+状态 / 模型+密钥 / 耗时+Token），点击展开详情。

#### 15.6.7 系统设置

```
┌───────────────┬──────────────────────────────────────────────────────────┐
│ 基础配置       │  基础配置                                                 │
│ 经营策略       │  ┌────────────────────────────────────────────────────┐  │
│ 登录与安全     │  │ 站点名称  [WBProxy Center            ]             │  │
│ ─────────     │  │ 网关地址  [http://127.0.0.1:3040/v1  ]             │  │
│ 请求转发       │  │ 时区      [Asia/Shanghai ▾]                        │  │
│ 代理管理       │  │ 主题      [跟随系统 ▾]                              │  │
│ 协议与兼容     │  └────────────────────────────────────────────────────┘  │
│ ─────────     │                                                          │
│ 账号与限额     │  ┌ 未保存的更改 ──────────────────────────────────────┐  │
│ 图像与视频     │  │  有 2 项更改   [放弃] [保存]                       │  │
│ 模型管理       │  └────────────────────────────────────────────────────┘  │
│ 价格与货币     │                                                          │
│ 日志与版本     │                                                          │
└───────────────┴──────────────────────────────────────────────────────────┘
```

- 左侧分类列表 216px，分组之间用 8px 空白 + 1px 分隔线分隔（不写分组标题，靠间距）。
- 分类项高 44px，选中 `primary-container`。
- 右侧内容区最大 880px。
- **保存条固定在底部**，有改动才出现（从下方滑入 200ms），显示"有 N 项更改"。
- 每个设置项：左标签 + 说明（`body-s`，`on-surface-variant`），右控件。行高 64px。
- 数值输入紧贴单位，单位文字在框内右侧。
- 危险操作（清空日志、清空记录、重置状态）单独放在区块底部，用 `error` 描边按钮 + 二次确认对话框。

「价格与货币」页签顶部：

```
┌──────────────────────────────────────────────────────────────────┐
│ 展示币种 [人民币 ¥ ▾]   汇率 1 USD = [7.10] CNY   [手工] [自动]    │
│ 价格表以美元为基准，切换展示币种不会改变历史记录金额。              │
└──────────────────────────────────────────────────────────────────┘
```

价格表格里同时显示 `$0.00015 / 1K` 与 `¥0.0011 / 1K`，编辑时按当前币种输入。

### 15.7 响应式

| 断点 | 名称 | 侧边栏 | 内容边距 | KPI 列 | 表格 |
| --- | --- | --- | --- | --- | --- |
| ≥1440 | large | 252px 常驻 | 32px | 4 | 完整表格 |
| 1240–1440 | expanded | 252px 常驻 | 32px | 4 | 完整表格 |
| 905–1240 | medium | 88px 图标 | 24px | 2 | 隐藏次要列 |
| 600–905 | compact | 抽屉 | 16px | 1 | 卡片列表 |
| <600 | compact | 抽屉 | 16px | 1 | 卡片列表 |

移动端专用规则：

- 底部安全区：`padding-bottom: calc(16px + env(safe-area-inset-bottom))`
- 表格类页面换卡片，**不要**横向滚动的宽表格。
- 对话框在手机上变全屏底部面板（从下滑入，圆角只有上方 28px）。
- 筛选面板在手机上变成底部抽屉，带"应用"按钮。
- 触控目标最小 44×44px。

### 15.8 无障碍

- 所有可交互元素必须有可见焦点环（`:focus-visible`）。
- 图标按钮必须有 `aria-label`。
- 表格用 `<th scope="col">`；可排序表头用 `aria-sort`。
- 对话框：打开时焦点进入，`Escape` 关闭，关闭后焦点回到触发元素，`aria-modal="true"`，打开期间 `inert` 背景。
- Toast 用 `role="status"` + `aria-live="polite"`；错误用 `aria-live="assertive"`。
- 颜色不是唯一信息载体：状态点必须配文字。
- 支持 `prefers-reduced-motion`。
- 对比度：正文 ≥ 4.5:1，大字号 ≥ 3:1，本规范给出的令牌已满足。

### 15.9 前端工程约束

#### 15.9.1 目录

```
web/src/
├─ main.ts
├─ App.vue
├─ design/
│  ├─ tokens.css          # 唯一允许出现十六进制色的地方
│  ├─ reset.css
│  ├─ typography.css
│  └─ motion.css
├─ components/            # 无业务语义的通用件
│  ├─ MdButton.vue  MdField.vue  MdSelect.vue  MdSwitch.vue
│  ├─ MdChip.vue    MdCard.vue   MdDialog.vue  MdToast.vue
│  ├─ MdTable.vue   MdSegmented.vue  MdTabs.vue  MdSkeleton.vue
│  └─ MdChart.vue   MdSparkline.vue  MdEmptyState.vue
├─ layouts/
│  └─ AppShell.vue  AuthLayout.vue
├─ pages/
│  ├─ Dashboard/  Accounts/  Automation/  Proxies/  Keys/  Logs/  Settings/
├─ api/
│  ├─ client.ts     # fetch 封装，统一错误信封
│  ├─ types.ts      # 由后端 OpenAPI 生成或手写，禁止 any
│  └─ endpoints/    # 按域拆分
├─ stores/          # Pinia：session / settings / toasts / filters
└─ composables/     # usePolling / usePagination / useFilters / useTheme
```

#### 15.9.2 硬规则

1. **组件里禁止写十六进制颜色值。** 只用 `var(--md-*)` / `var(--chart-*)` / `var(--status-*)`。CI 用正则扫描 `<style>` 块里的 `#[0-9a-fA-F]{3,8}`，发现即失败（`tokens.css` 除外）。
2. **禁止行内 `style` 写颜色和尺寸**，只允许写计算出来的动态值（如进度条 `width: {{pct}}%`）。
3. **禁止 `any`。** `tsconfig` 开 `strict: true`，`vue-tsc --noEmit` 必须零错误。
4. **一个页面一个目录**，页面内部组件不跨页面复用；要复用就提到 `components/`。
5. **不许引入 UI 框架**（见 2.3）。
6. **样式用 `<style scoped>`**，全局样式只放 `design/`。
7. 每个页面最多 **600 行**；超了就拆子组件。旧版 `Accounts.vue` 1043 行是反例。
8. 网络请求只在 `api/` 里发，组件不直接 `fetch`。
9. 轮询统一用 `usePolling`：页面隐藏自动暂停，恢复可见立即刷新一次。
10. 所有列表页必须有：加载骨架 / 空状态 / 错误状态 / 分页。四者缺一不可。

#### 15.9.3 状态管理

- `session`：登录态、版本、站点名。
- `settings`：全站设置（只读缓存，写入后失效重取）。
- `toasts`：全局提示队列。
- `filters`：把筛选条件同步到 URL query，刷新页面不丢。**日志页尤其重要。**

#### 15.9.4 构建

```json
{
  "scripts": {
    "dev": "vite",
    "build": "vue-tsc --noEmit && vite build",
    "preview": "vite preview",
    "test": "playwright test",
    "lint:colors": "node scripts/check-colors.mjs"
  }
}
```

- 产物 `web/dist`，构建时嵌入 Rust 二进制（用 `include_dir` 或 `rust-embed`）。
- 首屏 JS 目标 < 180 KB gzip。页面按路由懒加载，仪表盘可同步。
- 字体只加载 `Outfit` 一个可变字体，中文走系统字体，**不要**打包中文字体。

---

## 16. CI / CD

### 16.1 `ci.yml`

```yaml
name: CI
on:
  push:
    branches: [main]
  pull_request:
  workflow_call:
    inputs:
      ref: { type: string, default: '' }

permissions:
  contents: read

concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true

jobs:
  rust:
    runs-on: ubuntu-latest
    services:
      redis:
        image: redis:7-alpine
        ports: ['6379:6379']
        options: >-
          --health-cmd "redis-cli ping" --health-interval 5s
          --health-timeout 3s --health-retries 20
    steps:
      - uses: actions/checkout@v4
        with: { ref: "${{ inputs.ref || github.sha }}" }
      - uses: dtolnay/rust-toolchain@stable
        with: { components: rustfmt, clippy }
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all -- --check
      - run: cargo clippy --all-targets --all-features -- -D warnings
      - run: cargo test --workspace --all-features
        env:
          WBP_REDIS_URL: redis://127.0.0.1:6379/1
      - run: cargo build --release --locked

  windows:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --workspace --lib
      - run: cargo build --release --locked

  web:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with: { node-version: '22', cache: npm, cache-dependency-path: web/package-lock.json }
      - run: npm ci
        working-directory: web
      - run: npm run lint:colors
        working-directory: web
      - run: npm run build
        working-directory: web
      - run: npx playwright install --with-deps chromium
        working-directory: web
      - run: npm test -- --retries=0
        working-directory: web
```

要点：

- **`cargo clippy -D warnings` 必须过。** 这是防止代码腐化最有效的一条。
- Redis 用 service 容器，测试连 DB 1，每个测试用随机前缀，结束时清自己的键。
- Windows runner **只跑 lib 测试和构建**，不跑浏览器测试（旧版踩过：Windows runner 慢到超时）。
- 任何一步失败都不许 merge。

### 16.2 `release.yml`

```yaml
name: Release
on:
  push:
    tags: ['v*']
  workflow_dispatch:
    inputs:
      tag: { required: true, type: string }

permissions:
  contents: write
  packages: write

jobs:
  prepare:
    runs-on: ubuntu-latest
    outputs:
      tag: ${{ steps.v.outputs.tag }}
    steps:
      - uses: actions/checkout@v4
        with: { ref: "${{ inputs.tag || github.ref_name }}", fetch-depth: 0 }
      - id: v
        shell: bash
        run: |
          set -euo pipefail
          TAG="${{ inputs.tag || github.ref_name }}"
          [[ "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]
          test "$(git rev-parse HEAD)" = "$(git rev-parse "refs/tags/$TAG^{commit}")"
          VER=$(grep -oP '(?<=^version = ")[^"]+' Cargo.toml)
          test "$VER" = "${TAG#v}"
          test -s "docs/releases/$TAG.md"
          grep -q "$TAG" CHANGELOG.md
          echo "tag=$TAG" >> "$GITHUB_OUTPUT"

  test:
    needs: prepare
    uses: ./.github/workflows/ci.yml
    with: { ref: "${{ needs.prepare.outputs.tag }}" }

  build:
    needs: [prepare, test]
    strategy:
      matrix:
        include:
          - { os: ubuntu-latest,  target: x86_64-unknown-linux-gnu,   name: linux_amd64 }
          - { os: ubuntu-latest,  target: aarch64-unknown-linux-gnu,  name: linux_arm64,  cross: true }
          - { os: windows-latest, target: x86_64-pc-windows-msvc,     name: windows_amd64 }
          - { os: windows-latest, target: aarch64-pc-windows-msvc,    name: windows_arm64 }
          - { os: macos-13,       target: x86_64-apple-darwin,        name: darwin_amd64 }
          - { os: macos-14,       target: aarch64-apple-darwin,       name: darwin_arm64 }
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with: { node-version: '22', cache: npm, cache-dependency-path: web/package-lock.json }
      - run: npm ci && npm run build
        working-directory: web
      - uses: dtolnay/rust-toolchain@stable
        with: { targets: "${{ matrix.target }}" }
      - uses: Swatinem/rust-cache@v2
      - uses: taiki-e/upload-rust-binary-action@v1
        with:
          bin: wbproxycenter
          target: ${{ matrix.target }}
          archive: wbproxycenter_${{ needs.prepare.outputs.tag }}_${{ matrix.name }}
          include: README.md,LICENSE,THIRD_PARTY_NOTICES.md,THIRD_PARTY_LICENSES,docker-compose.yaml
          token: ${{ secrets.GITHUB_TOKEN }}
```

`publish` 作业：

1. 汇总所有产物，生成 `checksums.txt`（SHA-256）。
2. 用 `softprops/action-gh-release` 创建 Release，正文取 `docs/releases/<tag>.md`。
3. 用 `docker/setup-buildx-action` + `docker/build-push-action` 构建 `linux/amd64,linux/arm64` 镜像，推到 GHCR，标签 `vX.Y.Z` 与 `latest`。
4. 同时产出离线镜像包 `wbproxycenter_docker_<ver>_linux_{amd64,arm64}.tar.gz`。

### 16.3 版本号单一真源

- `Cargo.toml` 的 `[workspace.package] version` 是**唯一**版本号。
- 编译期用 `env!("CARGO_PKG_VERSION")` 注入。
- 发版脚本 `scripts/bump-version.sh X.Y.Z`：改 Cargo.toml、写 `docs/releases/vX.Y.Z.md` 模板、在 `CHANGELOG.md` 顶部加一节、提交、打 tag。
- 前端版本从构建环境读，**不要**在 `package.json` 里另外维护一份。

### 16.4 分支与提交

- 主分支 `main`，必须线性历史（squash merge）。
- 提交信息用 Conventional Commits：`feat:` / `fix:` / `perf:` / `refactor:` / `docs:` / `test:` / `chore:`。
- 只有文档/注释的提交在标题加 `[skip ci]`。
- 每个 PR 必须：CI 绿 + 更新 `CHANGELOG.md` + 有测试。

---

## 17. 测试策略

### 17.1 分层

| 层 | 工具 | 覆盖什么 | 目标 |
| --- | --- | --- | --- |
| 单元 | `cargo test` | 解析器、指纹、冷却计算、汇率换算、脱敏 | 每个纯函数 |
| 契约 | `wiremock` | 上游请求形状（头顺序、body 字节）、错误码映射 | provider 全量 |
| 集成 | `cargo test` + 真 Redis | 选号、重试、结算、索引查询 | gateway / store |
| 快照 | `insta` | Responses / Messages 转换、身份句净化 | 逐字节比对 |
| 前端 | Playwright | 七个页面的关键路径 | 每页 ≥ 3 条 |
| 端到端 | 脚本 | 启动 → 导入账号 → 建 Key → 对话 | 一条主链路 |

### 17.2 必须有的测试（缺一即验收失败）

1. **请求体字节保真**：原生 `/v2` 请求经过网关后与原始字节完全一致（含字段顺序）。
2. **请求头顺序**：出站头序列与预期数组逐项相等。
3. **凭证刷新串行**：两个并发刷新只有一个成功，另一个拿到新 token。
4. **刷新落盘顺序**：模拟落盘失败，内存状态必须回滚。
5. **重试不重复计费**：三次尝试只产生一条日志、一次用量。
6. **视频增量结算**：连续三次查询返回累计 10 / 20 / 20 积分，总共只记 20。
7. **健康分组优先级**：构造同时满足多条件的账号，断言按 6.2 的顺序归类。
8. **模型级业务码不污染账号**：11102 等错误不写 `last_error`。
9. **错误脱敏**：把 token、签名 URL、邮箱塞进上游响应，断言落库后已替换。
10. **错误指纹稳定**：同一条错误带不同时间戳/数字，指纹相同。
11. **筛选组合**：kind + stage + 时间窗 + 账号，结果集正确。
12. **货币双写**：同一条记录 `cost_cny = cost_usd × fx_rate`，切换展示币种不改历史。
13. **今日/累计不受 range 影响**：`range=7d` 时 `today` 与 `totals` 返回值不变。
14. **目录外部改动生效**：往 `access_account/` 写新文件，5 秒内出现在列表。
15. **文件原子写**：读取方在并发写入时不会读到半个 JSON。
16. **桥接停止后不回退直连**：隧道节点账号请求直接失败。

### 17.3 测试数据

- 上游一律用 `wiremock` 模拟，**不要把真实账号放进 CI**。
- Redis 用固定 DB（`/1`），每个测试用 `TEST::<ulid>::` 前缀，`Drop` 时清理。
- 时间相关逻辑注入 `Clock` trait，测试用可拨动的假时钟，**不要** `sleep`。

---

## 18. 功能对照与验收

### 18.1 功能对照表

| # | 旧版位置 | 功能 | 新版位置 | 验收 |
| --- | --- | --- | --- | --- |
| F01 | `provider/accounts.go` | 凭证解析（7 种形态） | `wbpc-provider::import` | 7 种输入的单元测试 |
| F02 | `provider/*` | OAuth 设备授权轮询 | `wbpc-provider::oauth` | 模拟 state/token 轮询 |
| F03 | `server/accounts.go` | 账号列表/详情/编辑 | `wbpc-api::accounts` | 面板可用 |
| F04 | `server/account_health.go` | 五组健康分类 | `wbpc-core::health` | 17.2-7 |
| F05 | `server/account_avoidance.go` | 两套回避策略 | `wbpc-gateway::avoidance` | 策略切换测试 |
| F06 | `server/rate_limit.go` | 模型冷却退避 | `wbpc-gateway::cooldown` | 分钟级序列测试 |
| F07 | `server/quota_probe.go` | 额度窗口巡检 | `wbpc-gateway::quota` | 窗口到期恢复 |
| F08 | `server/gateway.go` | 选号与重试 | `wbpc-gateway::router` | 17.2-5 |
| F09 | `server/stream.go` | SSE 保活与转换 | `wbpc-gateway::stream` | 快照测试 |
| F10 | `provider/wire.go` | 顺序头 + 字节保真 | `wbpc-wire` | 17.2-1/2 |
| F11 | `provider/upstream_json.go` | 保序 JSON 编解码 | `wbpc-wire::json` | 快照测试 |
| F12 | `provider/web_tools.go` | 本地 web_search / web_fetch | `wbpc-provider::webtools` | 私网拒绝测试 |
| F13 | `server/media_*.go` | 图片/视频管线 | `wbpc-media` | 17.2-6 |
| F14 | `server/media_download_preparation.go` | 两阶段下载 | `wbpc-media::staging` | 一次性令牌测试 |
| F15 | `provider/media_download_dns.go` | DoH 校验 | `wbpc-provider::doh` | 保留地址拒绝 |
| F16 | `automation/*` | 11 个任务 | `wbpc-automation` | 每个任务一个用例 |
| F17 | `server/task_schedule.go` | 计划与预设播种 | `wbpc-automation::schedule` | 预设仅播种一次 |
| F18 | `proxy/subscription.go` | 订阅解析 | `wbpc-proxy::parse` | 各协议样本 |
| F19 | `proxy/mihomo.go` | 桥接 | `wbpc-proxy::bridge` | 侧车启停 |
| F20 | `server/stats.go` | 仪表盘聚合 | `wbpc-api::dashboard` | 口径测试 |
| F21 | `server/usage_history.go` | 用量历史 400 天 | `wbpc-store::usage` | 裁剪测试 |
| F22 | `server/update*.go` | 自更新与回退 | `wbpc-server::update` | 端到端脚本 |
| F23 | `server/store_settings.go` | 设置局部更新 | `wbpc-store::settings` | null 拒绝测试 |
| F24 | `web/*` | 七个页面 | `web/src/pages` | Playwright |
| F25 | — | **错误详情 + 筛选** | `wbpc-core::errdetail` | 17.2-9/10/11 |
| F26 | — | **人民币展示** | `wbpc-core::money` | 17.2-12 |
| F27 | — | **今日 + 累计双轨** | `wbpc-api::dashboard` | 17.2-13 |
| F28 | `web/pages/keys` | CC Switch deeplink 导入 | `web/pages/Keys/ImportDialog` | 生成 `ccswitch://` 链接 |
| F29 | `provider` | `cmcc:` 模型默认不对外列出 | `wbpc-gateway::catalog` | 7.9.2 的单测 |
| F30 | `server/gateway.go` | 模型别名 | `wbpc-gateway::alias` | 别名不改响应模型名 |
| F31 | `server/settings.go` | 设置局部更新与原子拒绝 | `wbpc-store::settings` | null / 类型错误整包拒绝 |
| F32 | `server/cache_simulation.go` | 缓存模拟 | `wbpc-gateway::cache_sim` | 命中率可配 |
| F33 | `provider/identity.go` | 设备令牌加载 | `wbpc-provider::identity` | 文件与内联两种来源 |
| F34 | `server/record_policy.go` | 清空记录（二次确认） | `wbpc-api::records` | 清日志不清统计 |

### 18.2 验收清单

- [ ] `cargo clippy --all-targets -- -D warnings` 零告警
- [ ] `cargo test --workspace` 全绿，覆盖率 ≥ 60%（核心 crate ≥ 75%）
- [ ] `vue-tsc --noEmit` 零错误
- [ ] `npm run lint:colors` 通过（无硬编码色值）
- [ ] Playwright 全绿（Chromium）
- [ ] 手动走通：导入 3 个账号 → 建 Key → 对话（流式/非流式）→ 看日志 → 导出 CSV
- [ ] 手动走通：停 Redis 再启动，账号仍在、统计仍在
- [ ] 手动走通：删掉整个 Redis，账号仍在（文件真源验证）
- [ ] 手动走通：站内更新到下一版本并回退
- [ ] 文档：README、PROTOCOL、DASHBOARD-STATS 已更新
- [ ] `CHANGELOG.md` 有本次所有变更

---

## 19. 里程碑

按里程碑推进，**一个里程碑一次提交，不许并行铺开**。每个里程碑结束后必须能在本地跑起来。

| 里程碑 | 交付物 | 完成判据 |
| --- | --- | --- |
| **M0 骨架** | workspace、9 个 crate、配置加载、tracing、Redis 连接、健康检查、Dockerfile | `cargo run` 起得来，`/health` 返回 ok |
| **M1 凭证库** | `access_account` 读写、监听、导入导出、账号 CRUD API | 往目录扔文件，面板 5 秒内出现 |
| **M2 上游协议** | `wbpc-wire`、身份头、OAuth、刷新、模型目录、额度 | 能对一个真账号拉目录 |
| **M3 网关对话** | 选号、重试、流式、Chat/Responses/Messages 转换 | `curl` 能跑通三种协议 |
| **M4 媒体** | 图片、视频提交/轮询/结算、两阶段下载 | 提交一个视频任务并完成 |
| **M5 自动化** | 11 个任务、计划、执行记录、并发控制 | 每日签到跑通 |
| **M6 代理** | 节点 CRUD、订阅解析、桥接侧车、全局出口 | 隧道节点可用 |
| **M7 密钥与日志** | Key CRUD、鉴权、日志、错误详情、筛选、导出 | 17.2-9/10/11 通过 |
| **M8 统计** | 用量历史、仪表盘、双轨展示、人民币 | 17.2-12/13 通过 |
| **M9 更新与发布** | 自更新、回退、CI/CD、Release | 走通一次发版 |

时间建议：M0–M3 是主干，占一半工作量；M4/M5 各占 15%；其余占 20%。

---

## 20. 附录

### 20.1 上游业务码速查

| 码 | 含义 | 处理 |
| --- | --- | --- |
| `11101` | 不支持非流式 | 兼容入口强制 stream |
| `11102` | 找不到模型服务 | 模型级，不记账号 |
| `11128` | 渠道指纹校验拦截 | 净化 + 提示 |
| `11129` | 工具参数无效 | 模型级，不记账号 |
| `11134` | 模型提供方暂时不可用 | 模型级，不记账号 |
| `11140` | 账号被封禁 | 持久停用 |
| `11145` | 移动算力豆耗尽 | 模型级提示 |
| `11150` | 不支持该推理强度 | 模型级，不记账号 |
| `11158` | 模型对当前账号类型不可用 | 模型级，不记账号 |
| `11217` | OAuth 等待授权 | 继续轮询 |
| `14003` | 限流 | 按范围冷却 |
| `6004` | 免费额度窗口 | 白名单内持久化 |

### 20.2 术语

| 词 | 含义 |
| --- | --- |
| 号池 / realm | 上游区域，`cn` 国内版 / `intl` 国际版 |
| 账号 | `access_account/` 里的一个 json 文件 |
| 出口 / egress | 上游请求走哪个网络路径（直连 / 代理 / 桥接） |
| 回避 | 特定流错误触发的隔离分组，与"异常"不同 |
| 冷却 | 限时不可调度 |
| 窗口 | 上游给的额度重置时间点 |
| 结算 | 把一次请求的用量与费用写入统计 |
| 接单护栏 | 请求发出前的准入判断，不是预付费配额 |

### 20.3 常见错误示范（写代码时对照）

| 错误做法 | 正确做法 |
| --- | --- |
| 把账号也存进 Redis | 账号只在 `access_account/`，Redis 只存可重建的状态 |
| 刷新 token 后先放锁再落盘 | 先落盘再放锁 |
| 用 `HeaderMap` 迭代发请求 | 用有序 `Vec<(HeaderName, HeaderValue)>` |
| 用 `serde_json::to_value` 再 `to_string` | 用 `preserve_order` 的 `Value`，或直接透传字节 |
| 每次请求全量写一个大 JSON | 分档写入，见 5.4 |
| 用 `SCAN` 找日志 | 用预建 ZSET 索引 |
| 用当前汇率换算历史账单 | 结算时同时存 USD / CNY / 汇率 |
| 前端自己算汇率 | 后端返回三种金额，前端只选 |
| 失败率高的页面整片变红 | 只有失败行用语义 chip |
| 在组件里写 `#0062F3` | 用 `var(--md-primary)` |

### 20.4 文档维护

- 本文件是**设计冻结稿**。实现期发现设计缺陷，允许修订，但必须：
  1. 在 `CHANGELOG.md` 记录"规范修订：第 X 节，原因为何"
  2. 同步更新受影响的章节
- 不许只改代码不改文档，也不许只改文档不改代码。
- 每次发版前通读一遍第 17、18 章，逐项打勾。
