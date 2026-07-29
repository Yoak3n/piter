# Piter Gateway API 参考文档

## 概述

Piter Gateway 通过 **WebSocket** 和 **HTTP REST API** 两种方式对外提供服务。

- WebSocket 端点：`/ws`、`/ui-ws`（等价）
- HTTP 基础路径：`/api/*`

连接 WebSocket 后，服务器会立即推送两条消息：
1. `{"type": "capabilities", "protocolVersion": "...", "client_id": <u64>}`
2. `{"type": "sessions_list", "projects": [...]}`

---

## 一、WebSocket 命令

客户端发送的 JSON 消息按 `type` 字段路由到不同处理逻辑。

### 1.1 消息路由规则

| type | 处理方 | 说明 |
|------|--------|------|
| `broker_control` | `dispatch_control()` | 系统控制命令 |
| `gateway_command` | `dispatch_gateway_command()` | 网关业务命令（项目/会话管理） |
| `new_session` | `route_ui_message()` | 创建新会话（gateway 直接处理） |
| `switch_session` | `route_ui_message()` | 切换到已有会话（gateway 直接处理） |
| 其他类型 | `forward_to_instance()` | 透传给 pi 子进程 |

### 1.2 broker_control（系统控制）

```json
{
  "type": "broker_control",
  "requestId": "xxx",
  "command": "ping" | "info"
}
```

| command | 响应 result |
|---------|------------|
| `ping` | `{"pong": true}` |
| `info` | `{"version": "...", "features": ["rpc","ws","lan","health","multi_instance"]}` |

响应格式：`{"type": "control_response", "requestId": "...", "ok": true, "result": {...}}`

### 1.3 gateway_command（网关业务）

```json
{
  "type": "gateway_command",
  "requestId": "xxx",
  "command": "<command>",
  "data": { ... }
}
```

响应格式：`{"type": "gateway_response", "requestId": "...", "success": true|false, "data": {...} | "error": "..."}`

#### 项目管理

| command | data 参数 | data 响应 |
|---------|----------|-----------|
| `list_projects` | `archived?: bool` | `{"projects": [Project...]}` |
| `create_project` | `name`, `cwd`, `extensions?: [str]` | `{"project": Project}` |
| `update_project` | `id`, `name?: str`, `extensions?: [str]` | `{"project": Project}` |
| `delete_project` | `id` | `{}` |
| `pin_project` | `id`, `pinned?: i32` (默认 1) | `{}` |
| `archive_project` | `id`, `archived?: bool` (默认 true) | `{}` |

#### 会话管理

| command | data 参数 | data 响应 |
|---------|----------|-----------|
| `list_sessions` | 无 | `{"projects": [ProjectGroup...]}` |
| `delete_session` | `instanceId` | `{}` |
| `rename_session` | `path`, `name` | `{}` |
| `get_messages` | `instanceId` | `{"instanceId", "messages": [...], "messageSeq": u64}` |
| `get_active_sessions` | 无 | `{"sessions": [{instanceId, cwd, ...}]}` |

#### 系统信息

| command | data 响应 |
|---------|-----------|
| `get_health` | `HealthResponse` |
| `get_lan_info` | `LanInfoResponse` |

### 1.4 new_session（创建会话）

```json
{
  "type": "new_session",
  "cwd": "/absolute/path",
  "name": "Project Name"
}
```

| 字段 | 必填 | 说明 |
|------|------|------|
| `cwd` | 是 | 绝对路径，项目工作目录 |
| `name` | 否 | 项目名，默认 `"New Project"` |

> `name` 和 `cwd` 组合用于查找或自动创建 project。

**响应流程**（按顺序发送）：

1. 广播：`{"type": "sessions_list", "projects": [...]}` — 所有客户端收到更新
2. 快照：`{"type": "session_snapshot", "instanceId": "...", "messages": [], "messageSeq": 0}`
3. 结果：`{"type": "response", "command": "new_session", "success": true, "instanceId": "..."}`

后续 pi 进程初始化完成后，会推送 `get_state` 响应并更新会话状态。

### 1.5 switch_session（切换会话）

```json
{
  "type": "switch_session",
  "instanceId": "target-instance-id"
}
```

| 字段 | 必填 | 说明 |
|------|------|------|
| `instanceId` | 是 | 目标会话的实例 ID |

**响应**：`{"type": "session_snapshot", "instanceId": "...", "messages": [...], "messageSeq": N}`

如果目标会话处于 Unloaded 状态，会自动重新启动 pi 进程并加载历史消息（可能有数百毫秒延迟）。

### 1.6 消息透传

不属于上述类型的其他消息，gateway 会原样转发给 pi 子进程的 stdin。

路由规则：
1. 使用消息中的 `instanceId`（顶层或 `/payload/instanceId`）查找目标实例
2. 若未指定或实例不存在 → 返回 `command_undeliverable`（reason: `no_route` 或 `missing_instanceId`）

> **注意**：客户端必须始终在消息中携带 `instanceId`。前端在新建或切换 session 时应记录当前 `instanceId`，后续所有消息都需要带上它。

---

## 二、HTTP REST API

### 2.1 系统

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/health` | 健康检查：`{status, version, pi_version, lan_urls, uptime_secs}` |
| GET | `/api/lan-info` | 局域网信息：`{broker_ws_url, http_url, lan_urls, qr_data}` |
| GET | `/api/lan-qr` | LAN 二维码 SVG（Content-Type: image/svg+xml） |
| GET | `/api/git-branch` | 当前 Git 分支：`{branch: Option<String>}` |

### 2.2 项目

| 方法 | 路径 | Body / Query | 响应 |
|------|------|-------------|------|
| GET | `/api/projects` | Query: `archived=true`（可选） | `{success, projects: [Project...]}` |
| POST | `/api/projects` | `{name, cwd, extensions?}` | `{success, project: Project}` |
| PUT | `/api/projects/:id` | `{name?, extensions?}` | `{success, project: Project}` |
| DELETE | `/api/projects/:id` | — | `{success}` |
| POST | `/api/projects/:id/pin` | `{pinned?: i32}` | `{success}` |
| POST | `/api/projects/:id/archive` | `{archived?: bool}` | `{success}` |

### 2.3 会话

| 方法 | 路径 | Body / Query | 响应 |
|------|------|-------------|------|
| GET | `/api/sessions` | — | `{projects: [ProjectGroup...]}` |
| GET | `/api/load-session` | Query: `path=<session_file_path>` | `[Message...]` |
| GET | `/api/delete-session` | Query: `instanceId=<id>` 或 `path=<path>` | `{success, error?}` |
| POST | `/api/sessions/create` | `{cwd, name?}` | `{success, id, file_path}` |
| POST | `/api/sessions/rename` | `{path, name}` | `{success}` |

### 2.4 Pi 控制

| 方法 | 路径 | Body | 响应 |
|------|------|------|------|
| GET | `/api/pi/status` | — | `{running, instance_id?, session_path?}` |
| GET | `/api/pi/settings` | — | `{default_provider, default_model, packages, ...}` |
| POST | `/api/pi/restart` | `{instanceId}` | `{success, instanceId}` |
| POST | `/api/pi/stop` | `{instanceId}` | `{success}` |
| POST | `/api/rpc` | `{instanceId, id?, type, ...}` | pi 原始响应（30s 超时） |
| POST | `/api/rpc/ephemeral` | `{cwd, command: {id?, type, ...}}` | pi 原始响应（临时实例，用后销毁） |

### 2.5 配置

| 方法 | 路径 | Body | 响应 |
|------|------|------|------|
| GET | `/api/global-extensions` | — | `{success, extensions: [str...]}` |
| PUT | `/api/global-extensions` | `{extensions: [str...]}` | `{success}` |
| GET | `/api/session-config` | — | `{success, idle_timeout_secs}` |
| PUT | `/api/session-config` | `{idle_timeout_secs}` | `{success}` |

---

## 三、数据模型

### Project

```json
{
  "id": "uuid",
  "name": "项目名",
  "cwd": "/absolute/path",
  "extensions": [".ts", ".vue"],
  "pinned": 0,
  "archived": false,
  "created_at": "2025-01-01T00:00:00Z",
  "updated_at": "2025-01-01T00:00:00Z"
}
```

### ProjectGroup（会话列表分组）

```json
{
  "path": "/absolute/cwd",
  "dirName": "项目名",
  "sessions": [SessionInfo...]
}
```

### SessionInfo

```json
{
  "id": "instance_id",
  "label": "显示名",
  "createdAt": "2025-01-01T00:00:00Z",
  "filePath": "/path/to/session.jsonl",
  "updatedAt": 1700000000,
  "preview": "最近消息预览",
  "cwd": "/absolute/path",
  "instanceId": "xxx",
  "state": "active" | "idle" | "unloaded",
  "model": "model-id",
  "thinkingLevel": "...",
  "messageCount": 42,
  "messageSeq": 42
}
```

### PiSessionState

```json
{
  "sessionFile": "/path/to/session.jsonl",
  "sessionId": "xxx",
  "sessionName": "自动生成的标题",
  "modelId": "model-id",
  "modelName": "Model Display Name",
  "modelProvider": "provider-id",
  "thinkingLevel": "...",
  "isStreaming": false,
  "isCompacting": false,
  "messageCount": 42,
  "pendingMessageCount": 0,
  "contextWindow": 128000
}
```

---

## 四、服务器推送消息类型

### WebSocket 推送

| type | 时机 | 关键字段 |
|------|------|---------|
| `capabilities` | 连接时 | `protocolVersion`, `client_id` |
| `sessions_list` | 会话列表变更时广播 | `projects: [ProjectGroup...]` |
| `session_snapshot` | 切换/新建会话时 | `instanceId`, `messages`, `messageSeq` |
| `response` | new_session 结果 | `command`, `success`, `instanceId` |
| `gateway_response` | gateway_command 结果 | `requestId`, `success`, `data/error` |
| `control_response` | broker_control 结果 | `requestId`, `ok`, `result/error` |
| `command_undeliverable` | 消息无法投递时 | `requestId`, `command`, `reason` |
| `event` | pi 生命周期事件 | `event`, `instanceId`, `messageSeq` |

### Pi 生命周期事件（封装在 `type: "event"` 内）

`session_start`, `session_shutdown`, `session_name`, `agent_start`, `agent_end`, `turn_start`, `turn_end`, `message_start`, `message_update`, `message_end`, `tool_execution_start`, `tool_execution_update`, `tool_execution_end`, `auto_compaction_start`, `auto_compaction_end`, `auto_retry_start`, `auto_retry_end`, `model_select`

### command_undeliverable 的 reason 值

| reason | 含义 |
|--------|------|
| `missing_or_invalid_cwd` | cwd 缺失或为相对路径 |
| `missing_instanceId` | 缺少 instanceId |
| `no_route` | 无法确定目标实例 |
| `upstream_unavailable` | pi 进程不可用 |
| `session_create_failed` | 创建会话失败 |
| `spawn_failed` | 启动 pi 进程失败 |

---

## 五、数据库 Schema

### projects

| 列 | 类型 | 约束 |
|----|------|------|
| id | TEXT | PRIMARY KEY |
| name | TEXT | NOT NULL |
| cwd | TEXT | NOT NULL |
| pinned | INTEGER | NOT NULL DEFAULT 0 |
| archived | INTEGER | NOT NULL DEFAULT 0 |
| created_at | TEXT | NOT NULL (RFC3339) |
| updated_at | TEXT | NOT NULL (RFC3339) |

### project_extensions

| 列 | 类型 | 约束 |
|----|------|------|
| project_id | TEXT | NOT NULL, FK → projects(id) ON DELETE CASCADE |
| extension_name | TEXT | NOT NULL |
| extension_path | TEXT | 可为 NULL（写入时自动解析存储） |
| | | PRIMARY KEY (project_id, extension_name) |

### sessions

| 列 | 类型 | 约束 |
|----|------|------|
| instance_id | TEXT | PRIMARY KEY |
| session_path | TEXT | 可为 NULL（初始注册时为空，pi 报告后填充） |
| project_id | TEXT | FK → projects(id) ON DELETE SET NULL |
| cwd | TEXT | NOT NULL |
| name | TEXT | 可为 NULL（自动标题或用户设置） |
| created_at | TEXT | NOT NULL (RFC3339) |

### global_extensions

| 列 | 类型 | 约束 |
|----|------|------|
| extension_name | TEXT | PRIMARY KEY |
| extension_path | TEXT | 可为 NULL（写入时自动解析存储） |
