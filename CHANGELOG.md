# Changelog

本文件记录 piter 的重要变更。格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

## [0.1.0] - 2026-08-03

> 首个公开发布版本。piter 是一个 AI 编程助手客户端，以 Tauri 桌面应用 + 移动端网页（局域网）两种形式提供，通过 WebSocket + REST API 驱动后端管理的 pi coding agent 进程。

### 新增

#### 会话管理
- 多并行会话：每会话独立 pi 进程实例，可并行运行与切换
- 会话恢复：从数据库与磁盘文件恢复历史会话，pi 进程按需重启
- 空闲清理：超过空闲超时（默认 10 分钟）的会话自动卸载释放资源
- 自动命名：根据用户消息内容自动生成会话标题
- 评审等待：`ack_review` 机制，等待用户评审时过渡为 Idle
- **项目置顶 / 归档**：常用项目置顶（排序置前）、归档（隐藏可恢复），Web 侧底部 Archive 分区
- **会话手动重命名**（后端 API 支持）

#### 消息控制
- 流式输出「停止」：abort 命令 + 状态重置
- 发送「插队」（steer）：流式中立即投递，走 pi 原生队列
- 发送「排队」（followUp）：流式中进入本地 outbox，agent_end 后自动投递；支持取消 / 升级为插队
- 队列状态展示：steer 队列与本地排队消息的实时状态条

#### 消息显示
- Markdown 渲染（`marked`），按 turn 分组对话视图
- 思考过程（thinking）与工具调用（tool execution）独立展示
- 代码块独立复制按钮 + 语言标识
- 消息发送时间显示（agent 消息以一轮结束为标记）
- 消息时间线自动滚动 + 用户回看暂停（粘滞暂停，滚动回底部或点按钮恢复）

#### 会话创建与草稿
- 项目侧边栏快捷新建会话（预填项目 cwd/name）
- 每会话独立输入框草稿：切换不串文本、返回保留
- 输入框扩展至全屏编辑（移动端友好）

#### Provider 与 Pi 管理
- Provider 认证配置：读写 `~/.pi/agent/auth.json`（0600 权限），30+ 已知 Provider + models.json 编辑器
- Pi 版本管理：锁定版本、下载/解压/卸载，进度流
- 扩展管理：全局/项目双作用域控制（继承/启用/排除），`--no-extensions` + 白名单方案
- 扩展包市场：`pi install/remove` 管理，安装后自动注册全局扩展

#### 使用统计
- cost 聚合看板：7d/30d/90d 范围，全局/当前项目作用域
- 总览卡片 + 365 天活动热力图 + 模型/项目/工具排行 + 近期会话列表

#### 桌面端（Tauri）
- 管理面板：Status / Usage / Pi Config / Providers / Versions / Extensions / Marketplace / Appearance
- 窗口管理：多窗口、多显示器浮动定位、状态缓存
- 系统托盘：显示/隐藏切换、开机自启、退出
- 轻量模式：窗口全关后延迟释放资源（10 分钟计时）
- 单实例：二次启动聚焦已有主窗口
- 主题系统：system / light / dark
- 局域网分享：LAN IP 实时发现 + QR 码，移动端浏览器访问
- **应用自更新**（updater）：启动自动检查 → 下载（进度走日志）→ 系统确认框 → 安装重启；GitHub Releases 静态 JSON 载体

#### 基础设施
- NSIS 安装包（含卸载时 pi 进程清理 hook）
- 固定默认端口 31421（冲突回退随机），QR/分享链路带实际端口
- 应用图标：全平台品牌图标（ico/icns/png 全套）

### 修复

- 每轮 final answer 重复（BUG-001）
- 新用户无 Provider 配置入口（BUG-002）
- 横向滚动裁剪消息内容（长表格/大图/inline HTML 宽度约束）（BUG-003）
- 会话空闲卸载后前端状态指示灯不更新（BUG-006）
- Windows 下外部命令调用弹出控制台窗口（BUG-007）
- 流式排队消息在 A 会话完成后错误发到 B（BUG-008）
- 排队消息过早渲染进时间线 + 队列按钮被裁剪不可见（BUG-009）
- 流式输出 think 内容时无法向上滚动（BUG-010）
- Linux 兼容性修复

### 已知问题

- 新建会话页时侧边栏仍高亮旧 session（BUG-011，根因已定位，修复方案待实施：哨兵值 + 后端订阅判断）

### 技术栈

- Tauri 2.x（single-instance / autostart / dialog / opener / log / updater 插件）
- Rust（axum + tokio + rusqlite）
- Vue 3 + Vite + TypeScript
- WebSocket + REST API

[Unreleased]: https://github.com/Yoak3n/piter/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Yoak3n/piter/releases/tag/v0.1.0
