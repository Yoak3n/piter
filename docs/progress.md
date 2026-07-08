# Piter 项目进度报告

> 参考目标：[Picot](E:\GitVault\picot) (v0.2.2) — Pi Agent Desktop 会话管理器  
> 对比日期：2026-07-07

---

## 一、已完成的后端功能

| 模块 | 状态 | 功能说明 |
|---|---|---|
| **Pi 二进制解析** (`pi/resolve.rs`) | ✅ 完成 | 多路径搜索（PATH、npm、bun、scoop、Picot）+ 评分排序 + 符号链接/复制 + GitHub release 下载及压缩包提取（zip/tar.gz） |
| **PiRpcClient** (`pi/client.rs`) | ✅ 完成 | 多会话 pi 子进程管理器，支持 stdin/stdout 桥接、事件广播、命令通道、会话生命周期管理 |
| **PiBroker** (`pi/broker.rs`) | ✅ 完成（整合版） | 基于 axum 的统一 HTTP+WebSocket 服务：REST API（health/lan/sessions/rpc）、WebSocket 转发 pi 事件、LAN IP 发现、QR 码生成、静态资源服务 |
| **窗口管理器** (`base/window/`) | ✅ 完成 | 窗口创建/切换/隐藏/销毁，多显示器浮动窗口定位，状态缓存 |
| **系统托盘** (`base/tray.rs`) | ✅ 完成 | 退出/显示/自动启动菜单，左键单击切换窗口 |
| **轻量模式** (`base/lightweight.rs`) | ✅ 完成 | 所有窗口关闭后延迟进入轻量模式（10 分钟），重新聚焦时取消 |
| **定时器框架** (`base/timer.rs`) | ✅ 完成 | 基于 `delay_timer` 的调度引擎，支持增量 diff 更新 |

## 二、已完成的前端功能

| 模块 | 状态 | 功能说明 |
|---|---|---|
| **ChatPane.vue** | ✅ 完成 | Markdown 渲染、流式输出、按 turn 分组的对话界面 |
| **SessionSidebar.vue** | ✅ 完成 | 按项目分组的会话列表 |
| **ModelSelector.vue** | ✅ 完成 | 模型选择组件 |
| **usePiConnection** | ✅ 完成 | WebSocket 客户端，含事件处理/自动重连/消息管理 |
| **useSessions** | ✅ 完成 | 会话 CRUD 的 REST API 集成 |
| **design-system.css** | ✅ 完成 | CSS 主题变量与基础样式系统 |

## 三、对比 Picot 缺失的功能

### 3.1 较高优先级

| 功能 | 说明 |
|---|---|
| **Broker 控制命令** | `broker_control` 命令分发 — open_workspace、install/remove package、pick_folder、open_in_app、open_devtools、relaunch |
| **Updater 集成** | 缺少 `tauri-plugin-updater`，无更新检查/下载安装（含进度流） |
| **对话框插件** | 缺少 `tauri-plugin-dialog`，无原生文件夹选择器 |
| **PATH 增强** | Picot 的 `build_augmented_path()` 会合并 nvm/volta/bun/homebrew/cargo 等 shim 路径 |
| **健康检查 + 端点等待** | `wait_for_health()` / `wait_for_endpoint()` 辅助方法 |
| **专用会话进程** | `spawn_session_dedicated()` + `kill_workspace_dedicated()` 并发运行多 pi 进程 |

### 3.2 中等优先级

| 功能 | 说明 |
|---|---|
| **外部应用启动** | 在编辑器/终端/文件管理器中打开项目目录 |
| **外部 URL 打开** | 在系统默认浏览器中打开 URL |
| **启动恢复** | `find_latest_session_boot_target()` 恢复上次活动会话/工作目录 |
| **启动错误处理** | `bootstrap.html` 错误窗口 + `cmd_retry_startup` 重试机制 |
| **安全/能力模型** | `capabilities.json` + `permissions/default.toml` 权限声明 |

### 3.3 较低优先级 / 前端

| 功能 | 说明 |
|---|---|
| 设置页面（API Key、主题、包管理）| 配置管理 UI |
| 费用看板 | 每个会话的费用跟踪 |
| 文件浏览器 | 内嵌文件浏览 |
| 语音输入 | 基于浏览器 API 的语音输入 |
| 会话搜索 | 跨会话内容搜索 |
| 引导流程 | 首次使用的侧边栏引导 |
| 工作区操作 | 项目/工作区管理 UI |

## 四、架构差异

**Picot 架构：**
```
PiManager (子进程) + BrokerWs (WS 代理) + embedded-server.ts (JS REST API)
```

- 三层分离：Rust 管理进程，Rust WS 路由，Node.js 扩展处理 HTTP API

**Piter 架构：**
```
PiBroker (统一集成) — 替代 PiManager + BrokerWs + embedded-server.ts
```

- 将进程管理、WS 路由、REST API 全部整合到单个 axum 服务的 Rust 代码中
- 无 Node.js 扩展依赖，进程数更少

核心 RPC 协议（JSON line 到 pi stdin/stdout）保持不变。

## 五、进度总览

| 领域 | 完成度 | 备注 |
|---|---|---|
| 后端核心 | ~70% | Broker/RPC/窗口/托盘/定时器就绪，插件和控制命令待补 |
| 前端 | ~40% | 核心聊天界面就绪，设置/费用/搜索等缺失 |
| 基础设施 | ~30% | CI/CD、自动更新、错误处理、权限模型待建设 |

### 下一阶段关键任务

1. 引入 `tauri-plugin-dialog` / `tauri-plugin-updater` / `tauri-plugin-fs` / `tauri-plugin-process`
2. 实现 Broker 控制命令处理器（open_workspace / pick_folder / open_in_app 等）
3. 实现窗口销毁时的 Pi 进程清理
4. 实现启动恢复逻辑
