# Piter 开发者指南

本文档面向 Piter 的开发者与贡献者，涵盖项目结构、架构、核心功能实现、通信协议与开发环境。安装与使用请见 [README](../README.md)。

## 项目结构

```
├── crates/
│   ├── pi_rpc/            # Pi RPC 协议类型定义（命令、事件、消息、模型）
│   └── pi_server/         # 核心后端服务
│       ├── src/
│       │   ├── broker/    # Pi 进程管理（spawn、生命周期、子进程 I/O、PATH 增强）
│       │   ├── gateway/   # Gateway 核心（axum HTTP + WebSocket）
│       │   │   ├── handlers/    # REST API 处理器
│       │   │   │   ├── extensions.rs
│       │   │   │   ├── mod.rs
│       │   │   │   ├── pi.rs
│       │   │   │   ├── project.rs
│       │   │   │   ├── session.rs
│       │   │   │   └── system.rs
│       │   │   ├── ws/         # WebSocket 消息路由（broker_control / gateway_command / broker_command）
│       │   │   ├── broadcast.rs # 会话列表与事件广播
│       │   │   ├── command.rs   # pi 命令封装
│       │   │   ├── db.rs        # SQLite 数据库
│       │   │   ├── mod.rs       # Gateway 状态定义、事件循环与路由注册
│       │   │   ├── project.rs   # 项目管理与扩展发现
│       │   │   └── session_manager.rs  # 会话内存管理、空闲清理、自动命名
│       │   ├── stats/     # 使用统计/费用聚合（解析 pi 会话文件）
│       │   │   ├── aggregate.rs  # 按维度聚合（模型/项目/会话/每日/活动热力图）
│       │   │   ├── parse.rs      # 会话文件解析
│       │   │   ├── state.rs      # 内部累积状态
│       │   │   └── types.rs      # 查询参数与响应类型
│       │   ├── lib.rs
│       │   └── resolve.rs  # Pi 二进制文件查找与下载
│       └── Cargo.toml
├── chat/                  # chat 前端（轻量聊天界面，无 Tauri 依赖，网关路由 /chat）
│   └── src/
│       ├── components/    # Vue 组件（ChatPane、Composer、SessionSidebar、ModelSelector 等）
│       ├── composables/   # Vue 组合式函数（usePiConnection、useSessions）
│       ├── types/         # TypeScript 类型定义
│       └── utils/         # 工具函数
├── src/                   # Tauri 桌面前端（管理面板）
│   └── src/
│       ├── components/    # Vue 组件（AdminNav、StatusTab、UsageTab、PiConfigTab 等）
│       │   └── admin/     # 管理面板子组件
│       ├── composables/   # Vue 组合式函数（useAdmin、useMarketplace）
│       ├── router/        # Vue Router 配置
│       ├── styles/        # 设计系统（design-system.css）
│       ├── utils/         # 工具函数（theme.ts）
│       └── views/         # 页面视图（AdminView.vue）
├── src-tauri/             # Tauri 后端
│   └── src/
│       ├── admin/         # 管理面板命令（auth/config/extensions/pi/stats/status/system/version）
│       ├── base/          # 窗口管理、系统托盘、轻量模式、定时任务、单实例、启动初始化
│       └── pi/            # Pi 二进制解析与版本管理
├── docs/                  # 项目文档（API 参考、调用链、进度报告）
├── public/
├── scripts/
│   └── pi-version.json    # 锁定的 pi 版本号
└── Cargo.toml             # Rust Workspace 配置
```

## 技术栈

| 层 | 技术 |
|----|------|
| 桌面壳 | Tauri 2.x（single-instance / autostart / dialog / opener / log 插件） |
| 前端 | Vue 3 + Vite + TypeScript（lucide-vue-next 图标） |
| 后端 | Rust (axum + tokio) |
| 数据库 | SQLite (rusqlite) |
| 通信 | WebSocket + REST API |
| 包管理 | pnpm (workspace) |

## 架构概览

```
┌─────────────────────────────────────────────────────────────────┐
│                        客户端层                                  │
│  ┌──────────────┐    ┌──────────────┐                           │
│  │  Tauri 桌面   │    │   Chat 前端   │                          │
│  │  (管理面板)   │    │  (轻量聊天)   │                          │
│  └──────┬───────┘    └──────┬───────┘                           │
│         │  HTTP / WS        │  HTTP / WS                        │
├─────────┼────────────────────┼───────────────────────────────────┤
│         ▼                    ▼                                   │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │                   pi_server (Gateway)                    │    │
│  │  ┌──────────┐  ┌──────────────┐  ┌──────────────────┐   │    │
│  │  │ REST API │  │  WebSocket   │  │  Event Loop      │   │    │
│  │  │ Handlers │  │  Router      │  │  (pi ↔ clients)  │   │    │
│  │  └────┬─────┘  └──────┬───────┘  └────────┬─────────┘   │    │
│  │       │               │                    │             │    │
│  │       ▼               ▼                    ▼             │    │
│  │  ┌──────────┐  ┌──────────────────────────────────┐      │    │
│  │  │   SQLite  │  │       SessionManager             │      │    │
│  │  │   (DB)    │  │  (内存会话状态、消息缓存、空闲清理)│      │    │
│  │  └──────────┘  └──────────────────────────────────┘      │    │
│  │                                                          │    │
│  │  ┌──────────────┐  ┌────────────────────────────────┐    │    │
│  │  │  Stats 聚合   │  │ Broker (进程管理)              │    │    │
│  │  │ (会话文件 →  │  │  spawn → stdin_tx ──→ pi 子进程 │    │    │
│  │  │  使用/费用)  │  │  ──→ stdout 事件流             │    │    │
│  │  └──────────────┘  └────────────────────────────────┘    │    │
│  └──────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
```

Gateway 负责管理多个 pi coding agent 进程实例，每个实例对应一个会话（Session）。前端通过 WebSocket 与 Gateway 实时通信，Gateway 负责将命令转发给对应的 pi 进程，并将 pi 的事件流广播回前端。

## 核心功能

### 会话管理

- **创建会话**：指定工作目录（cwd）和可选的项目 ID，Gateway 启动一个 pi 子进程
- **切换会话**：支持在多个并行会话间切换，每个会话维护独立的消息历史和状态
- **删除会话**：终止 pi 进程、清理内存状态、删除数据库记录和磁盘文件
- **会话恢复**：从数据库和磁盘文件恢复历史会话（pi 进程按需重启）
- **空闲清理**：超过空闲超时时间（默认 10 分钟）的会话自动卸载以节省资源
- **自动命名**：根据用户消息内容自动生成会话标题
- **评审等待**：`ack_review` 机制，会话等待用户评审时可过渡为 Idle 状态

### 项目管理

- 项目与工作目录（cwd）绑定，支持 CRUD 操作
- 支持项目置顶（pin）和归档（archive）
- 每个项目可配置独立的扩展（extensions），支持从磁盘自动发现扩展

### 使用统计面板

- 聚合 pi 会话文件（`~/.pi/agent/sessions/**/*.jsonl`）中的 usage/cost 数据，镜像 Picot 的 `cost-dashboard` 载荷
- 时间范围：`7d` / `30d` / `90d`
- 作用域：全部会话（`all`）或最近活跃项目（`current`）
- 统计维度：总览（费用/会话数/消息数/Token 数/连续活跃天数）、模型、项目、会话、每日趋势、365 天活动热力图、工具调用分布
- 仅聚合 Piter 数据库登记的会话，忽略其他客户端产生的零散文件

### Provider 认证管理

- 读写 `~/.pi/agent/auth.json`（API Key，0600 权限），支持 30+ 已知 Provider 及 OAuth 订阅条目
- 编辑 `~/.pi/agent/models.json`（自定义 Provider 配置）

### Pi 版本管理

- 锁定版本号（`scripts/pi-version.json`），支持下载指定版本、解压安装到 resources/pi/、卸载
- 卸载后自动停止 gateway，可通过 `start_pi_gateway` 在会话中按需重新启动

### 扩展与包市场

- **Installed**：查看/启用/禁用全局与项目级扩展（DB 中 `global_extensions` 全局基准 + `project_added_extensions` 项目增量 + `project_excluded_extensions` 项目排除）
- **Market**：通过 `pi install <source>` / `pi remove <source>` 管理包，安装成功后自动注册为全局扩展

### Tauri 桌面特有功能

- **管理面板**：Status（运行状态）、Usage（使用统计）、Pi Config / Providers / Versions、Extensions Installed / Market、Appearance（主题）
- **窗口管理**：多窗口支持、状态追踪、浮动定位、多显示器适配
- **系统托盘**：显示/隐藏切换、开机自启、退出；关闭窗口时隐藏而非退出
- **轻量模式**：窗口全部关闭后自动计时（10 分钟），超时后释放资源
- **单实例**：二次启动时聚焦已有主窗口
- **主题系统**：system / light / dark，前端设计系统变量驱动
- **局域网分享**：生成 QR 码，支持局域网内其他设备通过浏览器访问

## WebSocket 实时通信

前端通过 WebSocket 发送以下命令：

| 消息类型 | 说明 |
|---------|------|
| `broker_control` | 系统控制（ping / info） |
| `gateway_command` | 网关业务命令（项目/会话 CRUD、健康检查等） |
| `broker_command` | 会话级命令，`payload.type` 决定行为（new_session / switch_session / ack_review），其余带 `instanceId` 的命令透传给 pi |
| 其他类型 | 按 `instanceId` 路由透传给 pi 子进程（prompt、set_model 等） |

后端推送以下事件：

| 事件类型 | 说明 |
|---------|------|
| `capabilities` | 连接时协议能力与 client_id |
| `session_snapshot` | 会话完整消息历史（切换时发送） |
| `sessions_list` | 更新后的项目-会话列表 |
| `gateway_response` | gateway_command 执行结果 |
| `control_response` | broker_control 执行结果 |
| `agent_start/end` | Agent 开始/结束处理 |
| `message_update` | 流式文本更新（text_delta / thinking_delta） |
| `tool_execution_*` | 工具调用状态更新 |
| `turn_start/end` | 对话轮次开始/结束 |
| `response` | 命令执行响应 |
| `pi_started` | 新 pi 进程启动通知 |
| `command_undeliverable` | 消息无法投递（缺失 instanceId / 无路由等） |

## REST API

| 端点 | 说明 |
|------|------|
| `GET /api/health` | 健康检查（含 broker_url） |
| `GET /api/lan-info` / `GET /api/lan-qr` | 局域网访问信息与二维码 |
| `GET /api/git-branch` | 当前 Git 分支 |
| `GET /api/sessions` | 项目-会话树 |
| `GET /api/load-session` / `GET /api/delete-session` | 加载/删除会话 |
| `POST /api/sessions/create` / `POST /api/sessions/rename` | 创建/重命名会话 |
| `GET /api/pi/status` / `GET /api/pi/settings` | Pi 状态与设置 |
| `POST /api/pi/restart` / `POST /api/pi/stop` | 重启/停止 Pi 实例 |
| `POST /api/rpc` / `POST /api/rpc/ephemeral` | 向活跃实例发送 RPC / 临时实例 RPC |
| `GET/POST/PUT/DELETE /api/projects` | 项目 CRUD 与置顶/归档 |
| `GET/PUT /api/global-extensions` | 全局扩展管理 |
| `GET/PUT /api/session-config` | 会话配置（空闲超时） |
| `GET /chat-ws` | WebSocket 端点（chat 客户端） |
| `GET /work-ws` | WebSocket 端点（work 客户端，初始握手不发 sessions_list） |
| `GET /ws` / `GET /ui-ws` | WebSocket 端点（ui/历史/管理兼容） |

详细契约见 [gateway-api-reference.md](gateway-api-reference.md)。

## 开发环境

### 前置要求

以下依赖**每一项都需要单独安装**，详细的分平台安装命令见 [README → 源码构建的依赖与安装](../README.md#源码构建的依赖与安装)：

- Node.js >= 18（推荐 20+ LTS）
- pnpm（最新稳定版，依赖 Node.js）
- Rust 工具链（stable，Tauri 2 要求 1.77.2+；推荐 rustup 安装）
- C/C++ 编译环境（Windows MSVC / macOS Xcode CLT / Linux build-essential + Tauri 系统依赖）
- Tauri CLI（^2，已包含在 devDependencies，无需单独安装）

### 安装依赖

```bash
pnpm install
```

### 开发运行

仅 Chat 前端（需要单独启动 pi_server）：

```bash
cd chat && pnpm dev
```

Tauri 桌面应用（完整开发环境）：

```bash
pnpm tauri dev
```

### 构建

Chat 前端：

```bash
cd chat && pnpm build
```

Tauri 桌面应用：

```bash
pnpm tauri build
```

## 推荐 IDE 配置

- [VS Code](https://code.visualstudio.com/)
- [Vue - Official](https://marketplace.visualstudio.com/items?itemName=Vue.volar)
- [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode)
- [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
