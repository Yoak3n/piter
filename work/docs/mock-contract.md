---
type: 文档
status: 🚧 草稿（契约基线，待后端实现校准）
created: 2026-08-10
---

# Work 模块契约（mock 基线 v0.1）

> 来源：[[开发计划/0.3.0/工作空间与产物管理]] + [[开发计划/0.3.0/工作视图与下载]] + [[开发计划/0.3.0/移动端App]] + [[开发计划/0.3.0/Flutter工程规划]]
> 用途：**Flutter mock 数据 + 后端实现的共同基线**——Flutter 端先按本契约做 mock 驱动全部 UI/交互/状态流；后端按本契约实现 work API + WS 事件，就绪后 Flutter 只换数据源。
> 状态：🚧 草稿——字段名/结构以后端实现为准；本文件由后端落地时校准。
> ⚠️ 鉴权依赖：**工作空间接口须挂局域网鉴权**（[[开发计划/0.2.0/局域网鉴权]]，已落地）；Web/LAN 形态未授权请求 → 401 `{"error":"lan_auth_required"}`。

---

## 1. 通用约定

### 1.1 基础路径与鉴权

- REST：`/api/workspaces/*`（与现有 `/api/sessions` 等并列）
- WS：`/chat-ws`（chat）、`/work-ws`（work）——见 §3.1
- 鉴权：LAN 非 loopback 请求走 PIN/token（0.2.0 已有）；loopback 豁免
- 错误格式（统一）：`{"success": false, "error": "<code>", "message": "<human>"}`（对齐现有 `/api/*` 约定）

### 1.2 类型定义（JSON 形态）

```jsonc
// Workspace（= projects 表 type='workspace' 的行）
{
  "id": "ws_ab12cd",              // projects.id
  "name": "我的工作空间",
  "cwd": "E:/data/piter/workspaces/ws_ab12cd/",   // real_dir
  "createdAt": 1723200000000,     // epoch ms
  "updatedAt": 1723200000000,
  "fileCount": 12,
  "sizeBytes": 3428000,
  "mode": "ask"                   // 写边界模式：ask | allow | deny（默认 ask）
}

// FileEntry（目录树节点）
{
  "path": "src/main.rs",          // 相对 real_dir
  "type": "file",                 // file | dir
  "size": 1024,
  "mtime": 1723200000000,
  "isDeliverable": false          // 手动标记过则为 true
}

// Artifact（产物条目，按 turn 分组）
{
  "id": "art_01",
  "workspaceId": "ws_ab12cd",
  "sessionId": "session-uuid",
  "turnId": 7,                    // 消息 seq / turn 序号
  "path": "output/report.md",
  "op": "new",                    // new | modified | deleted
  "size": 2048,
  "source": "snapshot",           // snapshot | live
  "deliverable": true,            // output/ 内 ∪ save_artifact ∪ 手动标记
  "createdAt": 1723200000000
}
```

---

## 2. REST 契约

### 2.1 工作空间 CRUD

| 方法 | 路径 | 请求体 | 响应 |
|---|---|---|---|
| `GET` | `/api/workspaces` | — | `{"workspaces": [Workspace]}` |
| `POST` | `/api/workspaces` | `{"name": str}` | `{"workspace": Workspace}`（创建 real_dir + 基线快照） |
| `DELETE` | `/api/workspaces/:id` | — | `{"success": true}`（级联删 real_dir + artifacts + 快照） |
| `GET` | `/api/workspaces/:id` | — | `{"workspace": Workspace}`（含文件统计） |

### 2.2 文件

| 方法 | 路径 | 请求体 | 响应 |
|---|---|---|---|
| `GET` | `/api/workspaces/:id/files` | — | `{"files": [FileEntry], "basePath": "…/real_dir/"}`（目录树，扁平含 rel path） |
| `POST` | `/api/workspaces/:id/upload` | multipart `files[]`（单/批量） | `{"uploaded": ["a.txt"], "rejected": [{"path":"b.txt","reason":"output_path_excluded"}]}` |
| `POST` | `/api/workspaces/:id/mark-deliverable` | `{"path": str, "deliverable": bool}` | `{"success": true, "entry": FileEntry}` |

上传校验：单文件 ≤50MB；`output/` 路径拒绝；`..`/绝对路径清洗拒绝。

### 2.3 产物

| 方法 | 路径 | 请求体 | 响应 |
|---|---|---|---|
| `GET` | `/api/workspaces/:id/artifacts` | `?sinceTurn=` | `{"turns": [{"turnId": 7, "createdAt": ms, "items": [Artifact]}]}`（按 turn 分组，新→旧） |
| `GET` | `/api/workspaces/:id/deliverables` | — | `{"items": [Artifact]}`（仅 deliverable=true，output/ ∪ 标记） |

### 2.4 下载

| 方法 | 路径 | 说明 |
|---|---|---|
| `GET` | `/api/workspaces/:id/download?path=<rel>` | 单文件流式；路径锚定 real_dir（防 `..` 穿越）；`Content-Disposition: attachment` |
| `POST` | `/api/workspaces/:id/zip` | body `{"paths": [rel...]}` 或 `{"all": true}` → 服务端临时打包 → 返回 `{"downloadUrl": "/api/workspaces/:id/zip-file/<token>"}`（或直接流式响应，后端定） |

安全：仅工作空间内文件可下载（用户侧边界）。

---

## 3. WS 契约

### 3.1 连接模型（端点定前端，0.3.0 定案）

- **端点与客户端类型对应**（gateway 连接注册表按 path 判定）：
  - `/chat-ws` → chat（Vue chat / App chat WebView）
  - `/work-ws` → work（工作空间视图/App；初始握手不发 chat 会话列表）
  - `/ws`、`/ui-ws` → ui（历史/管理兼容，不冒充业务客户端）
- chat 与 work 复用同一 WS 消息协议（`gateway_command` 等）；差异在**命令与事件内容**（work 会话带 `cwd=workspace`）+ 初始握手（work 无 `sessions_list`）

### 3.2 客户端 → 服务端命令

复用 `broker_command` 透传 + 新增 work 专属 gateway 命令：

```jsonc
// 会话级（透传 pi，与 chat 相同）
{ "type": "broker_command", "instanceId": "…", "payload": { "type": "prompt", "message": "…" } }
{ "type": "broker_command", "instanceId": "…", "payload": { "type": "new_session", "cwd": "<workspace real_dir>", "name": "…" } }

// work 专属 gateway 命令（新增，走 gateway_command）
{ "type": "gateway_command", "requestId": "r1", "command": "create_workspace_session", "data": { "workspaceId": "ws_ab12cd" } }
// → 响应 gateway_response: { "requestId": "r1", "success": true, "data": { "instanceId": "…", "sessionFile": "…" } }
```

### 3.3 服务端 → 客户端事件（新增，随现有事件流推送）

```jsonc
// 本轮产物摘要（turn_end 后推送，驱动产物区刷新）
{
  "type": "turn_artifacts",
  "instanceId": "…",
  "workspaceId": "ws_ab12cd",
  "turnId": 7,
  "items": [
    { "path": "output/report.md", "op": "new", "size": 2048, "deliverable": true },
    { "path": "src/lib.rs", "op": "modified", "size": 9001, "deliverable": false }
  ]
}

// 写阻断请求（agent 尝试写工作空间外，ask 模式）
{
  "type": "write_block",
  "instanceId": "…",
  "workspaceId": "ws_ab12cd",
  "path": "E:/other/project/x.txt",
  "reason": "写入位置应在工作空间内（cwd=…）；如确实需要请批准",
  "requestId": "wb_01"
}

// 写阻断批准结果（用户侧应答）
// 客户端 → 服务端：
{ "type": "gateway_command", "requestId": "wb_01", "command": "approve_write", "data": { "allow": true, "remember": true } }
// → gateway 写 .pi/approvals.json（文件轮询通道），扩展下次阻断前命中即放行
```

### 3.4 现有事件复用（work 会话同样收到）

- `message_start / message_update / message_end`（流式）
- `tool_execution_start / update / end`（工具块渲染；edit 工具带 `details.patch` → **unified diff 渲染**，Flutter 端最大自研点）
- `session_snapshot`、`sessions_list`、`agent_end`、`turn_end`

---

## 4. mDNS 服务发现

- 服务类型：`_piter._tcp`
- TXT 记录（字段名待后端定稿，基线如下）：

| TXT key | 值 | 说明 |
|---|---|---|
| `port` | `"31421"` | 实际端口（固定默认 31421 + 冲突回退随机，QR/卡片同源） |
| `proto` | `"1"` | 协议版本（能力探测用） |
| `name` | `"Yoa 的 Piter"` | 实例名（可读标识，列表显示） |

- 客户端：浏览 `_piter._tcp` → 解析 IP + TXT → 一键连接；**兜底**：扫码（QR）+ 手动 IP:端口
- 禁组播场景（访客 WiFi / 蜂窝 / 企业网）→ mDNS 不可用，降级提示清晰

## 5. QR 内容格式

- 复用现有 `/api/lan-qr` 的 qr_data 形态（含 `brokerWs=ws://IP:PORT/ws`），**不掺 PIN**（PIN 走 admin 另行告知）
- App 扫码 → 解析出 `baseUrl = http://IP:PORT` + `brokerWs` → 若服务端开鉴权 → 弹 PIN 输入 → 连接

## 6. PIN 鉴权 token 形态（影响 dio 拦截器）

- **现状（0.2.0 已落地）**：`Set-Cookie: piter_lan_token=…; HttpOnly; Path=/; Max-Age=2592000`
- **Flutter 侧结论**：dio 启用 `withCredentials: true`（cookie jar）——Web 端浏览器自动携带；App 端 IOWebSocketChannel/WebSocket 需显式带 Cookie 头（HTTP 握手时）
- WS 鉴权：浏览器同源自动携带；App 端握手头带 `Cookie: piter_lan_token=…`

---

## 7. 待后端确认（开放项）

> 状态：2026-08-12 后端已全部定稿并落地（见 §8.5）。

| # | 项 | 定稿 |
|---|---|---|
| 1 | `/api/workspaces/:id/zip` 返回形态 | **直接流式 `application/zip`**（`Content-Disposition: attachment`） |
| 2 | 上传批量上限 | 单文件 ≤50MB；批量不设上限（逐文件拒绝报告） |
| 3 | `turn_artifacts` 推送时机 | turn_end 事件处理后立即推送（快照 diff 结果），与 turn_end 同序 |
| 4 | `approve_write` 的 `remember` 语义 | 一律写入 `.pi/approvals.json` 白名单（持久）；`allow=false` 仅应答不改状态 |
| 5 | mDNS TXT 字段名 | `port` / `proto` / `name`（已落地，见 §8.4） |

---

## 8. 实现备注（Flutter mock 阶段追加，不改原契约）

> 由 Flutter 客户端实现时发现的契约与实现偏差记录（2026-08-12）。

### 8.1 默认端口：定案 31421（高位端口）

- 契约 §4 mDNS TXT `port` 基线为 `"31421"`（表格内）但 §4 说明文字曾写"固定默认 1421"，两处冲突——以 **31421 为准**；
- 后端：`crates/pi_server/src/gateway/server.rs` `DEFAULT_HTTP_PORT = 31421`（busy 时回退随机端口）；
- **0.3.0 定案（P1）**：服务端保持高位端口 **31421**；Flutter 侧手动添加默认端口、Web 候选、测试同步为 31421；连接/探测均读当前服务器 `baseUrl`，不硬编码端口。

### 8.2 SPA fallback 与 work 能力探测

- 0.2.x 服务端未注册 `/api/*` 路径时，SPA fallback 返回 **200 text/html**（而非 404 JSON）；
- 能力探测（`core/network/probe.dart`）判定 work 支持：`/api/workspaces` 返回 JSON 且含 `workspaces` 数组才视为支持；HTML/非 JSON 视为不支持；
- 优雅降级：work 不支持时列表页提示「当前服务端 piter {version} 不支持 work 模块」，chat 不受影响（与 0.3.0/移动端App 规划一致）。

### 8.3 服务端版本现状（联调验证）

- 本机运行服务端 `/api/health` 返回 `version: 0.2.1`、`pi_version: 0.83.0`——**尚无 work API**；
- Flutter 侧 `HttpApiClient`/`HttpWsClient` 已按本契约实现，后端 work API 落地后即真实可用（数据源按当前服务器自动切换，见 `features/work/providers/data_sources.dart`）。

### 8.4 mDNS 服务端已实现（2026-08-12）

- 服务端 `crates/pi_server/src/gateway/mdns.rs` 用 **mdns-sd** 注册 `_piter._tcp` 广播，TXT `port`/`proto`/`name` 对齐契约 §4；
- 实例名：环境变量 `PITER_MDNS_NAME` 优先，否则取主机名；
- 启动 gateway 时自动注册（端口确定后）；失败仅告警不阻塞（扫码/手动为保底）；
- 新增 `GET /api/mdns/status` → `{enabled, instanceName, port, serviceType}`（未注册时 `{enabled:false}`）；
- 注册校验注意：host_name 必须以 `.local.` 结尾（mdns-sd 要求）；
- 真实组播验证：`cargo test -p pi_server mdns -- --ignored`（注册 + browse 自发现）。

### 8.5 Work 后端已落地（2026-08-12，契约定稿）

- **REST**：`/api/workspaces` 全端点已实现——CRUD、`files`（含 basePath）、`upload`（multipart `files` 字段，单文件 ≤50MB，拒绝 `output/` 与 `..`/绝对路径）、`mark-deliverable`、`artifacts?sinceTurn=`、`deliverables`、`download?path=`（锚定 real_dir）、`zip`（直接流式）、额外 `PUT /:id/mode`（ask|allow|deny 写边界）；
- **错误格式**：`{"error": "<code>", "message": "<human>"}` + 非 2xx 状态码（对齐 §1.1；非 SPA HTML）；
- **`approve_write` data 需携带 `workspaceId` + `path`**：客户端把 `write_block` 事件里的两个字段原样回填（契约 §3.3 示例省略了它们）；
- **写边界软约束**：workspace 创建时自动生成 `.pi/extensions/constraint.ts`（注册为项目扩展，工作空间会话 spawn 自动携带），拦截工作空间外 write/edit；`allow` 放行 / `deny` 全拦 / `ask` 按 `.pi/approvals.json` 白名单放行；模式存 `.pi/workspace.json`；
- **`write_block` 推断**：服务端从 `tool_execution_start`（toolName=write/edit，目标在工作空间外且未在白名单）推断并推送；已批准路径不再重复打扰；
- **产物**：`deliverable` = `output/` 前缀 ∪ 手动标记；turn_end 快照 diff 后推送 `turn_artifacts`（items 无 id 轻量形态）；
- **验证**：`cargo test -p pi_server` 61 通过（含 CRUD/快照 diff/路径穿越/artifacts 分组/REST 流程/live server 路由）+ `cargo check --workspace` 通过；Flutter 端数据源自动切换后即可真实联调（§8.3）。

### 8.6 工作空间基目录（2026-08-13 落地，对齐《工作空间与产物管理》定案）

- **定案实现**：real_dir = `<基目录>/workspaces/<id>`；基目录 = Admin 配置（DB `workspace_config` 单行表）优先 → 默认安装目录（pi 所在目录）→ 写入保护（Program Files）回退 app 数据目录；
- **API**：`GET/PUT /api/workspaces/base-dir`——GET 返回生效/默认基目录、可写性、迁移队列与各工作空间活跃状态；PUT（`{"baseDir": ""}` 清空回默认）校验可写 → 持久化 → 构建迁移队列；
- **迁移（gateway/migrate.rs）**：后台 2s 调度；不活跃工作空间立即迁移（同卷 rename / 跨卷 copy+`.migrating` 暂存+delete）；活跃会话等待 agent_end 后迁移；等待期间 `create_workspace_session` 拒绝（防饿死）；迁移完成先文件落位再事务更新 `projects.cwd` + `sessions.cwd`；队列持久化 `data_dir/migration-queue.json`（temp+rename 原子写），重启恢复；pending 期间禁止再次修改基目录（409）；
- **兼容**：既有工作空间（0.3.0 前建在 AppData）在下次改基目录时一并迁移；get/list/delete/mode 一律读 DB cwd，迁移中途不串位；
- **验证**：`cargo test -p pi_server` 66 通过（含迁移单测：同卷迁移 + 队列持久化恢复）。

---

## 关联

- [[开发计划/0.3.0/工作空间与产物管理]]（数据模型 / 快照 diff / 约束机制）
- [[开发计划/0.3.0/工作视图与下载]]（多 SPA 路由 / 下载预览 / 分享连接页）
- [[开发计划/0.3.0/移动端App]]（mDNS / 双模块 / PIN）
- [[开发计划/0.3.0/Flutter工程规划]]（mock 先行流程 / lib 结构 / 网络层选型）
- [[开发计划/0.2.0/局域网鉴权]]（token 形态前置）
