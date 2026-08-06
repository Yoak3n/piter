# Changelog

本文件记录 piter 的重要变更。格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [0.1.2] - 2026-08-06

> 版本主线：图片/文本附件与多模态 + 模型切换加固 + admin 分享与连接页 + 应用更新检查 UI。

### 新增

- **图片与文本文件嵌入**：Composer 附件（拖拽 / 粘贴 / 选择），图片自动压缩（canvas 缩放 ≤1024px、>8MB 拒绝）与时间线渲染（明暗主题），文本文件 >200KB 截断拼入 prompt；模型视觉能力检测（正则规则表，不支持时选图弱提示）
- **`/api/pi/model-catalog`**：无需启动 pi 即可加载本地模型目录（新会话/模型选择预检提速）
- **会话模型信息持久化**：set_model 成功即写入 DB（gateway `handle_model_response`），重启/恢复会话后保留所用模型
- **admin「分享与连接」页**：管理面板新增专门 Tab（AdminNav 新入口）——LAN 分享卡片（URL + 二维码，复用 `/api/lan-qr`，一键复制）+ 连接信息（broker WS/HTTP、端口、健康状态）+ 扫码 / 手动 `IP:端口` 连接引导
- **应用更新检查与安装 UI**：admin Status 页「检查更新」按钮 + 更新弹窗（当前/最新版本、更新说明、下载进度、安装重启）；Rust 侧 `check_for_update` / `install_update` 命令（进度走 Channel），Linux 构建为桩（更新由系统包管理器管理）
- **i18n 国际化**：vue-i18n en/zh 双语言——共享消息集（packages/ui/src/i18n），双端接入；admin 设置页语言切换即时生效并持久化，chat 端经 nav.rs 注入 `?lang=` 跟随；相对时间改用 `Intl.RelativeTimeFormat`（随语言变化）；Rust 侧 `AppSettings.language` 字段
- **共享 UI 组件**：@piter/ui 新增 EmptyState / InlineConfirm / StatusDot / SkeletonList / PanelCard / StatCard / ChartCard
- **首启三步引导**：欢迎页引导（配置 provider → 创建会话 → 提问），localStorage 一次后跳过
- **会话手动重命名**：侧栏会话项内联编辑（铅笔 → 输入框，Enter 保存/Esc 取消）；后端同步 DB + 内存会话名（防自动命名覆盖）并推送所有客户端

### 重构

- **标题栏窗口控制改事件通道**：chat 运行在网关远程源（http://127.0.0.1:PORT），invoke 自定义命令被 Tauri 命令 ACL 静默拒绝——改用事件通道（window-minimize / toggle-maximize / close / query-maximized）+ Resized 事件同步最大化状态（emit_maximized_state），WM 缓存保持与实际窗口一致
- **design-system 单源治理**：双份 design-system.css 合并为 @piter/ui 单源（exports `./styles/*`），消除漂移
- **UI 令牌升级**：主色 #6a7a8a → #2f6fed 明亮蓝（暗色 #6ea8ff）、圆角体系 12/16/20、主色色调投影、动效 .16-.25s；新增语义状态 token（--state-idle/busy/review/error）与图表色板（--chart-1..6）；主按钮改软填充+描边（蓝只点缀不做大面积背景）
- **SessionSidebar 拆分**：1012 → 433 行，拆出 ProjectGroup / SessionItem，正常/归档重复渲染消除，删除确认统一走 sessionKey
- **UsageTab 图表主题化**：OVERVIEW_TONES 硬编码色板 → 运行时读 --chart-1..6（跟随明暗主题）
- **全量 i18n 文案**：chat 与 admin 所有界面文案抽取，错误提示友好化（给“怎么办”）

### 修复

- **模型切换失败无反馈（BUG-017）**：set_model/cycle_model 失败经现有 response 链路渲染 system 消息提示（i18n 文案），不再静默；配套 gateway `handle_model_response`——成功即刷新运行时模型状态并持久化，失败清掉 default 备份
- 模型切换加固：`sync_model_if_needed` 发送 set_model 前备份 settings.json 全局 default，成功后恢复（default 只允许 admin 修改）
- `save_pi_agent_settings` 自动创建 `~/.pi/agent/` 目录（新机尚无目录时保存不再失败）
- 会话删除/归档确认在无 instanceId 会话上失效（确认框不弹出）——统一改用 sessionKey（instanceId 兑底 id）
- ModelSelector 同 id 跨 provider 模型双双高亮——高亮对比补 provider 维度 + 列表 key 改 provider/id 组合
- 窗口关闭/恢复：托盘恢复不重连 WS（桌面 WebView 不触发 visibilitychange）——Rust Focused(true) 发 piter-window-shown；标题栏关闭后 WM 缓存不同步导致托盘 toggle 误判——CloseRequested 统一同步缓存；标题栏窗口操作统一走 invoke → WindowManager

## [0.1.1] - 2026-08-05

> 版本主线：统一自定义标题栏 + Bug 修复批次 + Linux 构建支持。

### 新增

- **统一自定义标题栏**：packages/ui 共享包（TitleBar 外壳 + useTauriWindow 窗口控制），chat 与 admin 双端接入，替代系统标题栏；data-tauri-drag-region 拖拽 + min/max/close 按钮，非 tauri 环境自动降级
- **Linux 构建支持**：CI matrix 双平台（Windows NSIS + Linux AppImage/deb），uploadUpdaterJson 仅 Windows

### 修复

- **BUG-010**：流式输出 think 时无法向上滚动——粘滞暂停方案多次迭代后定案（方向判断 + 手机端 touch 补强）
- **BUG-011**：新建会话页侧边栏仍高亮旧会话——哨兵值 NewSession + 后端 deactivate_session；衍生：窗口关闭时主动断开 WS（订阅清理不再双重拖延）
- **BUG-012**：日志时间戳时区用 UTC——timezone_strategy(UseLocal) 切换本地时区
- **BUG-013**：provider 故障时发送无反馈——auto_retry 错误可见 + error 容错 + 90s 无进展 watchdog
- **BUG-014**：pi 未安装时启动加载 /chat 404 白屏——回退内置 admin 面板引导下载 pi

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
