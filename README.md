# Piter

Piter 是一个 AI 编程助手客户端，以 Web UI 和 Tauri 桌面应用两种形式提供，通过 WebSocket + REST API 驱动后端管理的 pi coding agent 进程。

## 项目结构

```
├── crates/
│   ├── pi_rpc/            # Pi RPC 协议类型定义（命令、事件、消息、模型）
│   └── pi_server/         # 核心后端服务
│       ├── src/
│       │   ├── broker/    # Pi 进程管理（spawn、生命周期、子进程 I/O）
│       │   ├── gateway/   # Gateway 核心
│       │   │   ├── handlers/  # REST API 处理器
│       │   │   │   ├── extensions.rs
│       │   │   │   ├── mod.rs
│       │   │   │   ├── pi.rs
│       │   │   │   ├── project.rs
│       │   │   │   ├── session.rs
│       │   │   │   └── system.rs
│       │   │   ├── ws/        # WebSocket 消息路由
│       │   │   ├── db.rs      # SQLite 数据库
│       │   │   ├── mod.rs     # Gateway 状态定义与事件循环
│       │   │   ├── project.rs # 项目管理
│       │   │   └── session_manager.rs  # 会话内存管理、空闲清理、自动命名
│       │   ├── lib.rs
│       │   └── resolve.rs     # Pi 二进制文件查找与下载
│       └── Cargo.toml
├── web/                   # 独立 Web 前端（轻量版，无 Tauri 依赖）
│   └── src/
│       ├── components/    # Vue 组件（ChatPane、SessionSidebar、ModelSelector 等）
│       ├── composables/   # Vue 组合式函数（usePiConnection、useSessions）
│       ├── types/         # TypeScript 类型定义
│       └── utils/         # 工具函数
├── src/                   # Tauri 桌面前端（完整版，含 Admin 面板）
│   └── src/
│       ├── components/    # Vue 组件（ChatPane、LanShare、Admin 子组件）
│       ├── composables/   # Vue 组合式函数（useAdmin、usePiConnection）
│       └── views/         # 页面视图（ChatView、AdminView、DesktopView）
├── src-tauri/             # Tauri 后端
│   └── src/
│       ├── admin/         # 管理面板命令
│       ├── base/          # 窗口管理、系统托盘、轻量模式、定时任务
│       └── pi/            # Pi 二进制文件解析与 Tauri 集成
├── public/
├── scripts/
│   └── pi-version.json    # 锁定的 pi 版本号
└── Cargo.toml             # Rust Workspace 配置
```

## 技术栈

| 层 | 技术 |
|----|------|
| 桌面壳 | Tauri 2.x |
| 前端 | Vue 3 + Vite + TypeScript |
| 后端 | Rust (axum + tokio) |
| 数据库 | SQLite (rusqlite) |
| 通信 | WebSocket + REST API |
| 包管理 | pnpm (workspace) |

## 架构概览

```
┌─────────────────────────────────────────────────────────────────┐
│                        客户端层                                  │
│  ┌──────────────┐    ┌──────────────┐                           │
│  │  Tauri 桌面   │    │   Web 前端    │                          │
│  │  (src/)       │    │  (web/src/)   │                          │
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
│  │  ┌──────────────────────────────────────────────────┐    │    │
│  │  │              Broker (进程管理)                     │    │    │
│  │  │  spawn → stdin_tx ──→ pi 子进程 ──→ stdout 事件流 │    │    │
│  │  └──────────────────────────────────────────────────┘    │    │
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

### 项目管理

- 项目与工作目录（cwd）绑定，支持 CRUD 操作
- 支持项目置顶（pin）和归档（archive）
- 每个项目可配置独立的扩展（extensions）

### WebSocket 实时通信

前端通过 WebSocket 发送以下命令：

| 消息类型 | 说明 |
|---------|------|
| `broker_command` | 向 pi 进程发送命令（prompt、set_model 等） |
| `switch_session` | 切换活跃会话 |
| `new_session` | 创建新会话 |

后端推送以下事件：

| 事件类型 | 说明 |
|---------|------|
| `session_snapshot` | 会话完整消息历史（切换时发送） |
| `sessions_list` | 更新后的项目-会话列表 |
| `agent_start/end` | Agent 开始/结束处理 |
| `message_update` | 流式文本更新（text_delta / thinking_delta） |
| `tool_execution_*` | 工具调用状态更新 |
| `turn_start/end` | 对话轮次开始/结束 |
| `response` | 命令执行响应 |
| `pi_started` | 新 pi 进程启动通知 |

### REST API

| 端点 | 说明 |
|------|------|
| `GET /api/sessions` | 项目-会话树 |
| `GET /api/delete-session` | 删除会话 |
| `POST /api/sessions/create` | 创建会话 |
| `GET /api/pi/status` | Pi 运行状态 |
| `POST /api/rpc` | 向活跃实例发送 RPC |
| `GET/POST/PUT /api/projects` | 项目 CRUD |
| `GET/PUT /api/global-extensions` | 全局扩展管理 |
| `GET /api/health` | 健康检查 |
| `GET /api/lan-info` | 局域网访问信息 |
| `GET /ws` | WebSocket 端点 |

### Tauri 桌面特有功能

- **窗口管理**：多窗口支持、状态追踪、浮动定位、多显示器适配
- **系统托盘**：显示/隐藏切换、开机自启、退出
- **轻量模式**：窗口全部关闭后自动计时，超时后释放资源
- **局域网分享**：生成 QR 码，支持局域网内其他设备通过浏览器访问

## 开发环境

### 前置要求

- Node.js >= 18
- pnpm
- Rust 工具链
- Tauri CLI（已包含在 devDependencies）

### 安装依赖

```bash
pnpm install
```

### 开发运行

仅 Web 前端（需要单独启动 pi_server）：

```bash
cd web && pnpm dev
```

Tauri 桌面应用（完整开发环境）：

```bash
pnpm tauri dev
```

### 构建

Web 前端：

```bash
cd web && pnpm build
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
