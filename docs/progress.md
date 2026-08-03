# Piter 项目进度报告

> 参考目标：[Picot](E:\GitVault\picot) (v0.2.2) — Pi Agent Desktop 会话管理器  
> 对比日期：2026-08-02

---

## 一、已完成的后端功能

| 模块 | 状态 | 功能说明 |
|---|---|---|
| **Pi 二进制解析** (`crates/pi_server/src/resolve.rs`) | ✅ 完成 | 多路径搜索（PATH、npm、bun、scoop、Picot）+ 评分排序 + 符号链接/复制 + GitHub release 下载及压缩包提取（zip/tar.gz），支持进度流 |
| **PiRpcClient** (`crates/pi_rpc/`) | ✅ 完成 | RPC 协议类型定义（命令、事件、消息、模型） |
| **Gateway** (`crates/pi_server/src/gateway/`) | ✅ 完成 | 基于 axum 的统一 HTTP+WebSocket 服务：REST API（health/lan/sessions/projects/rpc）、WebSocket 转发 pi 事件、LAN IP 发现、QR 码生成、静态资源服务（SPA fallback） |
| **会话管理** (`gateway/session_manager.rs`) | ✅ 完成 | 多并行会话、消息缓存、空闲清理（超时卸载）、自动命名、评审等待（WaitingReview→Idle）、会话恢复 |
| **项目管理** (`gateway/project.rs`) | ✅ 完成 | 项目 CRUD、置顶/归档、扩展发现与解析（直接+包入口） |
| **使用统计** (`crates/pi_server/src/stats/`) | ✅ 完成 | 聚合 pi 会话文件（usage/cost），7d/30d/90d 范围、全局/当前项目作用域，输出总览/模型/项目/会话/每日趋势/365 天活动热力图/工具分布 |
| **窗口管理器** (`src-tauri/src/base/window/`) | ✅ 完成 | 窗口创建/切换/隐藏/销毁，多显示器浮动窗口定位，状态缓存 |
| **系统托盘** (`src-tauri/src/base/tray.rs`) | ✅ 完成 | 退出/显示/自动启动菜单，左键单击切换窗口 |
| **轻量模式** (`src-tauri/src/base/lightweight.rs`) | ✅ 完成 | 所有窗口关闭后延迟进入轻量模式（10 分钟），重新聚焦时取消 |
| **定时器框架** (`src-tauri/src/base/timer.rs`) | ✅ 完成 | 基于 `delay_timer` 的调度引擎，支持增量 diff 更新 |
| **管理命令** (`src-tauri/src/admin/cmd/`) | ✅ 完成 | auth（Provider API Key 管理）、config（主题/自启/超时）、extensions（扩展概览）、pi（重启/停止/包管理/agent 设置）、stats（费用面板）、status、system、version（Pi 下载/卸载） |
| **Provider 认证** (`src-tauri/src/admin/cmd/auth.rs`) | ✅ 完成 | 读写 `~/.pi/agent/auth.json`（0600），30+ 已知 Provider + OAuth 订阅条目，models.json 编辑器 |
| **单实例** (`src-tauri/src/base/init.rs`) | ✅ 完成 | `tauri-plugin-single-instance`，二次启动聚焦主窗口 |

## 二、已完成的前端功能

| 模块 | 状态 | 功能说明 |
|---|---|---|
| **ChatPane.vue** (chat/) | ✅ 完成 | Markdown 渲染、流式输出、按 turn 分组的对话界面 |
| **Composer / MessageTimeline / ThinkingBlock / ToolCard** (chat/) | ✅ 完成 | 输入区、消息时间线、AI 思考过程、工具调用展示 |
| **SessionSidebar.vue** (chat/) | ✅ 完成 | 按项目分组的会话列表 |
| **ModelSelector.vue** (chat/) | ✅ 完成 | 模型选择组件 |
| **usePiConnection** (chat/) | ✅ 完成 | WebSocket 客户端，含事件处理/自动重连/消息管理 |
| **useSessions** (chat/) | ✅ 完成 | 会话 CRUD 的 REST API 集成 |
| **AdminView.vue** (src/) | ✅ 完成 | 管理面板主视图 |
| **AdminNav.vue** (src/) | ✅ 完成 | 分组导航（Status/Usage、Pi 分组、Extensions 分组、Appearance） |
| **StatusTab / UsageTab / PiConfigTab / ProvidersTab / PiVersionsTab / ExtensionsTab / MarketplaceTab / AppearanceTab** (src/) | ✅ 完成 | 管理面板各标签页 |
| **useAdmin / useMarketplace** (src/) | ✅ 完成 | 管理命令与包市场集成 |
| **design-system.css** (src/ + chat/) | ✅ 完成 | CSS 主题变量与基础样式系统（system/light/dark） |

## 三、对比 Picot 的功能对照

### 3.1 已补齐（此前缺失，现已实现）

| 功能 | 说明 |
|---|---|
| **Broker 控制命令** | `broker_control` 命令分发（ping/info）+ `gateway_command` 业务命令 |
| **对话框插件** | `tauri-plugin-dialog` 已接入 |
| **PATH 增强** | `build_augmented_path()` 合并 nvm/volta/bun/homebrew/cargo 等 shim 路径（`broker/util.rs`） |
| **专用会话进程** | 多并行 pi 进程管理（每会话独立实例） |
| **外部应用启动** | `open_path` 在文件管理器中打开目录 |
| **外部 URL 打开** | `tauri-plugin-opener` 打开 URL |
| **安全/能力模型** | `capabilities/default.json` 权限声明 |
| **启动错误处理** | Pi 二进制缺失时跳过启动，通过 `start_pi_gateway` 按需重试 |

### 3.2 待补（中等优先级）

| 功能 | 说明 |
|---|---|
| **Updater 集成** | 缺少 `tauri-plugin-updater`，无更新检查/下载安装（含进度流） |
| **健康检查 + 端点等待** | `wait_for_health()` / `wait_for_endpoint()` 辅助方法 |
| **启动恢复** | `find_latest_session_boot_target()` 恢复上次活动会话/工作目录 |
| **启动错误处理** | `bootstrap.html` 错误窗口 + `cmd_retry_startup` 重试机制 |

### 3.3 较低优先级 / 前端

| 功能 | 说明 |
|---|---|
| 费用看板增强 | 会话级费用明细图表 |
| 文件浏览器 | 内嵌文件浏览 |
| 语音输入 | 基于浏览器 API 的语音输入 |
| 会话搜索 | 跨会话内容搜索 |
| 引导流程 | 首次使用的侧边栏引导 |
| 工作区操作 | 项目/工作区管理 UI（聊天端） |

## 四、架构差异

**Picot 架构：**
```
PiManager (子进程) + BrokerWs (WS 代理) + embedded-server.ts (JS REST API)
```

- 三层分离：Rust 管理进程，Rust WS 路由，Node.js 扩展处理 HTTP API

**Piter 架构：**
```
PiBroker/Gateway (统一集成) — 替代 PiManager + BrokerWs + embedded-server.ts
```

- 将进程管理、WS 路由、REST API 全部整合到单个 axum 服务的 Rust 代码中
- 无 Node.js 扩展依赖，进程数更少
- 统计模块（stats）独立于 gateway，纯函数式聚合，供桌面管理面板与未来 web REST 复用

核心 RPC 协议（JSON line 到 pi stdin/stdout）保持不变。

## 五、进度总览

| 领域 | 完成度 | 备注 |
|---|---|---|
| 后端核心 | ~90% | Gateway/Broker/会话/项目/统计/管理命令就绪，Updater 与启动恢复待补 |
| 前端 | ~70% | 聊天界面 + 管理面板就绪，费用图表/搜索/引导等增强待做 |
| 基础设施 | ~50% | 单实例、托盘、自动启动、日志、NSIS 打包就绪，CI/CD 与自动更新待建 |

### 下一阶段关键任务

1. 引入 `tauri-plugin-updater`，实现更新检查/下载安装（含进度流）
2. 实现启动恢复逻辑（恢复上次活动会话/工作目录）
3. 使用统计 REST 化：将 `get_cost_dashboard` 暴露为 `/api/cost-dashboard`（stats 模块已与 gateway 解耦，可直接复用）
4. 补充健康检查等待、启动错误处理（bootstrap.html + 重试）
