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
- WS：复用现有 `/ws`、`/ui-ws`（**不加独立 work WS**——见 §3.1）
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

### 3.1 连接模型（复用现有 WS，不加独立端点）

- **同一 WS 连接**（`/ws` 或 `/ui-ws`）承载 chat + work 两类事件——App/Web 的 work 模块复用现有连接，不新增 `/work-ws`
- 与 chat 的差异仅在**命令与事件内容**（work 会话带 `cwd=workspace`）

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
| `port` | `"31421"` | 实际端口（固定默认 1421 + 冲突回退随机，QR/卡片同源） |
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

| # | 项 | 当前基线 | 待确认 |
|---|---|---|---|
| 1 | `/api/workspaces/:id/zip` 返回形态 | `downloadUrl` 间接 | 或直接流式 `application/zip`（推荐直接流式，少一跳） |
| 2 | 上传批量上限 | 单次 ≤20 文件 | 后端定 |
| 3 | `turn_artifacts` 推送时机 | turn_end 后 | 与 `turn_end` 事件顺序 |
| 4 | `approve_write` 的 `remember` 语义 | 写入 approvals.json 白名单 | 是否区分"仅本次" |
| 5 | mDNS TXT 字段名 | port/proto/name | 后端定稿 |

---

## 关联

- [[开发计划/0.3.0/工作空间与产物管理]]（数据模型 / 快照 diff / 约束机制）
- [[开发计划/0.3.0/工作视图与下载]]（多 SPA 路由 / 下载预览 / 分享连接页）
- [[开发计划/0.3.0/移动端App]]（mDNS / 双模块 / PIN）
- [[开发计划/0.3.0/Flutter工程规划]]（mock 先行流程 / lib 结构 / 网络层选型）
- [[开发计划/0.2.0/局域网鉴权]]（token 形态前置）
