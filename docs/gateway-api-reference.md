# Piter Gateway API 参考文档

## 概述

Piter Gateway 通过 **WebSocket** 和 **HTTP REST API** 两种方式对外提供服务。

- WebSocket 端点（按 path 判定客户端类型，0.3.0 起）：
  - `/chat-ws` → chat（Vue chat / App chat WebView）
  - `/work-ws` → work（工作空间视图/App；初始握手不发 `sessions_list`）
  - `/ws`、`/ui-ws` → ui（历史/管理兼容，不冒充业务客户端）
- HTTP 基础路径：`/api/*`

连接 WebSocket 后，服务器会立即推送 `capabilities`；其中 `work` 客户端**不再推送 `sessions_list`**（chat/ui 客户端仍会收到）：

1. `{"type": "capabilities", "protocolVersion": "...", "client_id": <u64>}`
2. `{"type": "sessions_list", "projects": [...]}`（work 客户端无此条）

---

## 一、WebSocket 命令

客户端发送的 JSON 消息按 `type` 字段路由到不同处理逻辑。

### 1.1 消息路由规则

| type | 处理方 | 说明 |
|------|--------|------|
| `broker_control` | `dispatch_control()` | 系统控制命令（ping / info） |
| `gateway_command` | `dispatch_gateway_command()` | 网关业务命令（项目/会话管理） |
| `broker_command` | `handler_broker_command()` | 会话级命令，按 `payload.type` 分发（new_session / switch_session / ack_review），其余带 `instanceId` 的命令透传给 pi |
| 其他类型 | `forward_to_instance()` | 按 `instanceId` 路由透传给 pi 子进程 |

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

通过 `broker_command` 发送：

```json
{
  "type": "broker_command",
  "payload": {
    "type": "new_session",
    "cwd": "/absolute/path",
    "name": "Project Name",
    "model": { "id": "model-id", "provider": "provider-id" }
  }
}
```

| 字段 | 必填 | 说明 |
|------|------|------|
| `cwd` | 是 | 绝对路径，项目工作目录（顶层或 `payload.cwd`） |
| `name` | 否 | 项目名，默认 `"New Project"` |
| `model` | 否 | 初始模型，`{id, provider?}` → `"provider/id"` 传给 pi |

> `name` 和 `cwd` 组合用于查找或自动创建 project。

**响应流程**（按顺序发送）：

1. 广播：`{"type": "sessions_list", "projects": [...]}` — 所有客户端收到更新
2. 快照：`{"type": "session_snapshot", "instanceId": "...", "messages": [], "messageSeq": 0}`
3. 结果：`{"type": "response", "command": "new_session", "success": true, "instanceId": "..."}`

后续 pi 进程初始化完成后，会推送 `get_state` 响应并更新会话状态。

### 1.5 switch_session（切换会话）

通过 `broker_command` 发送：

```json
{
  "type": "broker_command",
  "payload": {
    "type": "switch_session",
    "instanceId": "target-instance-id"
  }
}
```

| 字段 | 必填 | 说明 |
|------|------|------|
| `instanceId` | 是 | 目标会话的实例 ID（顶层或 `payload.instanceId`） |

**响应**：`{"type": "session_snapshot", "instanceId": "...", "messages": [...], "messageSeq": N}`

如果目标会话处于 Unloaded 状态，会自动重新启动 pi 进程并加载历史消息（可能有数百毫秒延迟）。

### 1.6 ack_review（评审确认）

前端在用户查看/切换到等待评审的会话时发送，使会话从 `WaitingReview` 过渡为 `Idle`，以便 RPC fallback 可用。

```json
{
  "type": "broker_command",
  "instanceId": "xxx",
  "payload": { "type": "ack_review" }
}
```

### 1.7 消息透传

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
| GET | `/api/health` | 健康检查：`{status, version, pi_version, lan_urls, broker_url, uptime_secs}` |
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

### 2.6 局域网鉴权（LAN auth，0.2.0 P3）

非 loopback（局域网）请求在鉴权开启时需先通过 PIN 换取 30 天设备 cookie（`piter_lan_token`）。
未授权时：`/api/*` 与 WS upgrade 返回 `401 {success:false, error:"lan_auth_required"}`；页面请求返回服务端内联 PIN 页（不暴露 SPA）。

| 方法 | 路径 | Body | 响应 |
|------|------|------|------|
| POST | `/api/lan/auth` | `{pin}` | 校验成功：`200 {success, expiresAt}` + `Set-Cookie: piter_lan_token=...`；错误 PIN：`401 {success:false, error:"lan_auth_bad_pin"}` |
| GET | `/api/lan/auth/config` | — | `{success, enabled, pinSet}`（不返回 PIN 明文） |
| PUT | `/api/lan/auth/config` | `{enabled?}` 和/或 `{regenerate:true}` | `{success, enabled, pinSet, pin?}`（`pin` 仅在重新生成时返回一次） |
| GET | `/api/lan/auth/devices` | — | `{success, devices: [{token, createdAt, expiresAt}...]}` |
| DELETE | `/api/lan/auth/devices/:id` | — | `{success}`（`id` 即设备 token） |
| POST | `/api/lan/auth/revoke` | — | `{success}`（清空全部设备） |

豁免路径：loopback 请求（桌面端）、鉴权未开启、`/api/lan/auth` 本身、`/api/health`。

安全约束：
- **爆破防护**：`POST /api/lan/auth` 按来源 IP 连续 5 次输错锁定 60 秒（内存表，重启清零），锁定期间返回 `429 {error:"lan_auth_rate_limited", retryAfter}`。
- **配置变更仅限本机**：`PUT /api/lan/auth/config`、`DELETE /api/lan/auth/devices/:id`、`POST /api/lan/auth/revoke` 仅 loopback 可调用（admin 面板走 127.0.0.1），非 loopback 返回 `403 {error:"lan_forbidden_local_only"}`；只读查询（config/devices）保持 cookie 门禁。
- **威胁模型**：PIN 以加盐 SHA-256（快哈希）存储，6 位 PIN 熵低——本地 `piter.db` 被窃取即可秒破 PIN；鉴权定位是"防误入"，不视为强密码。

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
  "name": "项目名",
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

`agent_start`, `agent_end`, `agent_settled`, `turn_start`, `turn_end`, `message_start`, `message_update`, `message_end`, `bash_execution_update`, `tool_execution_start`, `tool_execution_update`, `tool_execution_end`, `queue_update`, `compaction_start`, `compaction_end`, `auto_retry_start`, `auto_retry_end`, `summarization_retry_scheduled`, `summarization_retry_attempt_start`, `summarization_retry_finished`, `extension_error`

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

### project_added_extensions

项目在全局基础上额外启用的扩展（增量）。`project_excluded_extensions` 存项目显式排除的扩展（即使全局启用也不加载）。

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

### lan_auth_config（单行，id=1）

| 列 | 类型 | 约束 |
|----|------|------|
| id | INTEGER | PRIMARY KEY CHECK (id=1) |
| enabled | INTEGER | NOT NULL DEFAULT 0 |
| pin_hash | TEXT | NOT NULL DEFAULT ''（salt 化 SHA-256，不落明文） |
| pin_salt | TEXT | NOT NULL DEFAULT '' |
| updated_at | TEXT | NOT NULL (RFC3339) |

### lan_tokens（每设备一条）

| 列 | 类型 | 约束 |
|----|------|------|
| token | TEXT | PRIMARY KEY（随机 32 hex） |
| created_at | TEXT | NOT NULL (RFC3339) |
| expires_at | TEXT | NOT NULL (RFC3339)，30 天后过期 |

---

## 六、桌面管理命令（Tauri IPC）

桌面管理面板通过 Tauri IPC 调用以下命令（非 REST/WS，仅供 `src/` 管理面板使用）：

| 命令 | 说明 |
|------|------|
| `get_admin_status` | 运行状态：pi_running、active_sessions、pi_version、broker URL、uptime、data_dir |
| `get_admin_config` / `update_admin_config` | 应用配置（theme / auto_start / start_minimized / request_timeout_secs / auto_restart_on_crash） |
| `get_cost_dashboard` | 使用统计（镜像 Picot `/api/cost-dashboard`） |
| `get_pi_install_info` / `download_pi_version` / `uninstall_pi` | Pi 版本管理（下载走 progress Channel 流式进度） |
| `list_pi_auth_status` / `set_pi_api_key` / `remove_pi_api_key` | Provider 认证管理（`~/.pi/agent/auth.json`） |
| `get_pi_models_config` / `save_pi_models_config` | 自定义 Provider 配置（`~/.pi/agent/models.json`） |
| `get_extension_overview` / `set_global_extensions` / `set_project_added_extensions` / `set_project_excluded_extensions` | 扩展管理（全局基准 + 项目增量/排除） |
| `list_pi_packages` / `install_pi_package` / `remove_pi_package` | 包市场（`pi list/install/remove` + DB 注册） |
| `restart_pi` / `stop_pi` / `start_pi_gateway` / `get_pi_agent_settings` / `save_pi_agent_settings` | Pi 进程与 agent 设置 |
| `open_path` | 在系统文件管理器中打开路径 |

### get_cost_dashboard 参数与响应

| 参数 | 取值 | 默认 |
|------|------|------|
| `range` | `"7d"` / `"30d"` / `"90d"` | `"30d"` |
| `granularity` | `"day"`（当前唯一支持） | — |
| `scope` | `"all"` / `"current"` | `"all"` |

响应结构 `UsageDashboard`：

```json
{
  "range": { "range": "30d", "from": "2026-07-03", "to": "2026-08-02" },
  "overview": {
    "total_cost": 0.0, "sessions": 0, "messages": 0, "total_tokens": 0,
    "active_days": 0, "current_streak": 0, "longest_streak": 0,
    "input_tokens": 0, "output_tokens": 0, "cache_read": 0, "cache_write": 0,
    "tool_calls": 0
  },
  "usage": { "total_tokens": 0, "input_tokens": 0, "output_tokens": 0,
             "cache_read": 0, "cache_write": 0, "tool_calls": 0,
             "tools": [{ "name": "...", "count": 0, "cost": 0.0, "fraction": 0.0 }] },
  "models": [{ "name": "...", "total_tokens": 0, "input_tokens": 0,
               "output_tokens": 0, "cost": 0.0, "fraction": 0.0 }],
  "projects": [{ "name": "...", "cwd": "...", "sessions": 0, "cost": 0.0, "fraction": 0.0 }],
  "sessions": [{ "title": "...", "workspace": "...", "model": "...",
                 "total_tokens": 0, "tool_calls": 0, "total_cost": 0.0, "time": "..." }],
  "daily": [{ "key": "2026-07-03", "total": 0, "models": {} }],
  "activity": [{ "key": "2026-07-03", "value": 0 }]
}
```

> `scope = "current"` 时，仅聚合最近创建会话所在 cwd 的会话；聚合范围限定为 Piter DB 登记的会话文件（DB 不可读时回退为目录扫描）。
