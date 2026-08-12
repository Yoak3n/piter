---
type: 任务提示词
任务: Flutter work 模块 mock 实现（0.3.0 工程底座）
适用: 交给专门 coding agent 执行
---

# Coding Agent 任务提示词：Flutter work 模块（mock 先行）

> **你是一个 Flutter 移动端/Web 工程师**，负责为 piter 项目实现 0.3.0「工作空间（work）」模块的 Flutter 客户端骨架——**本阶段全部用 mock 数据驱动，不依赖真实后端**。

## 一、项目背景

piter 是一个 Tauri 桌面应用 + Web 移动端的 AI agent 会话管理器（对标 TRAE Work）：用户创建**工作空间**（虚拟目录 → 磁盘 `workspaces/<id>/`），上传文件让 pi agent 在里面工作，最终从浏览器/App 下载**产物**（交付物）。

- 0.3.0 目标：**Flutter 移动端 App（chat + work 双模块）+ 浏览器 work 视图（Flutter Web）**
- 本任务只做 **work 模块的 Flutter 客户端 + mock 数据层**，chat 模块（WebView 嵌入）不在本次范围
- 仓库：`E:/Project/RustProject/piter`，Flutter 工程在 **`work/`** 目录（Dart 包名 `piter_work`）

## 二、必读文档（开工前完整阅读）

| 文档 | 路径 | 用途 |
|---|---|---|
| **mock 契约基线** | `work/docs/mock-contract.md` | **本任务的核心依据**——REST/WS/mDNS 的 JSON 契约，mock 数据必须与之一致 |
| 架构定案 | `开发计划/0.3.0/Flutter工程规划.md` | lib/ 目录结构、技术选型、状态管理定案 |
| 功能规划 | `开发计划/0.3.0/工作空间与产物管理.md` | 数据模型（workspace/artifacts）、快照 diff、写边界约束 |
| 视图规划 | `开发计划/0.3.0/工作视图与下载.md` | work 详情页三区、下载/预览、多 SPA 路由 |
| 现有 WS 协议 | `docs/gateway-api-reference.md` | 既有 /ws 事件格式（复用，不另起炉灶） |

## 三、技术栈（已定案，不可更换）

| 层 | 选型 | 说明 |
|---|---|---|
| 状态管理 | **Riverpod**（`flutter_riverpod`） | 不用代码生成（不加 riverpod_annotation）；流式状态用 Notifier/StreamProvider |
| HTTP | **dio** | multipart 上传、下载进度、拦截器（token）；App/Web 双端 |
| WebSocket | **web_socket_channel** | 双端自动适配（App→IOWebSocketChannel，Web→浏览器原生） |
| 路由 | **go_router** | Web 需要 URL 同步（/work、/workspaces/:id） |
| 本地存储 | shared_preferences | 服务器列表 + token（Web 落 localStorage） |
| 平台能力 | **条件导入桥**（core/platform/ + `_web` stub） | mDNS/推送/原生存储在 Web 是 stub；**本阶段可只留接口 + mock，不接真实插件** |

> ⚠️ 本阶段**不引入**：bonsoir（mDNS）、mobile_scanner、file_picker、path_provider、share_plus 等原生插件——它们依赖真实设备/后端。平台桥留接口 + `_web` stub 即可，**后续阶段再接**。

## 四、本任务目标（范围）

### 交付物

1. **`work/lib/` 完整目录骨架**（见下节结构），`main.dart` 用 `kIsWeb` 分支入口
2. **全部 mock 数据源**（`lib/core/network/` 下 `MockApiClient`/`MockWsClient` 或等价命名）：按 mock-contract.md 的 JSON 结构硬编码样例数据，**不依赖网络**
3. **work 核心 UI 可运行**：
   - **Web 端**（`kIsWeb`）：仅 work 模块（无 tab）——工作空间列表页 + 详情页（文件/消息/产物三区）
   - **App 端**（非 web）：底部双 tab（chat 占位页 + work）——chat tab 本阶段显示"待接入"占位
4. **单元测试**：mock 数据层解析、状态层（Riverpod Notifier）核心逻辑

### 明确不做（本次范围外）

- ❌ chat 模块实现（WebView 嵌入）——只做占位
- ❌ 真实后端联调、真实 WS 连接
- ❌ mDNS / 扫码 / PIN 鉴权真实实现——平台桥留接口 + stub
- ❌ Flutter 桌面端（windows/linux/macos runner 是脚手架默认，无需改）
- ❌ 任何 `开发计划/0.3.0/` 下后端文档提到的 Rust/Vue 代码

## 五、目录结构（按 Flutter工程规划.md 定案）

```
work/lib/
├── main.dart                  # kIsWeb 分支入口（调 runWorkApp / runMobileApp）
├── app/
│   ├── piter_app.dart         # MaterialApp + 主题（seed #6a7a8a 蓝灰）+ go_router 路由
│   ├── app_shell.dart         # App 端：底部双 tab（chat 占位 / work）
│   └── web_shell.dart         # Web 端：仅 work（无 tab）
├── core/
│   ├── config/                # ServerConfig：服务器列表 + token 持久化（shared_preferences）
│   ├── network/
│   │   ├── api_client.dart    # 抽象 ApiClient + MockApiClient（mock 数据源）
│   │   ├── ws_client.dart     # 抽象 WsClient + MockWsClient（mock 事件流）
│   │   └── models/            # workspace/artifact/file/message + JSON 解析（严格对齐契约）
│   ├── platform/              # 条件导入桥：discovery/ storage/（本阶段 interface + _web stub + _mock）
│   └── theme/                 # 主题（seed #6a7a8a，复用设计系统语义色）
├── features/
│   ├── connection/            # 服务器管理（mock：固定一条服务器记录 + 手动输入表单 UI）
│   ├── chat/                  # 占位页（"chat 待接入"）
│   └── work/
│       ├── workspace_list/    # 列表 + 创建（mock：返回 2-3 条固定 workspace）
│       ├── workspace_detail/  # 详情三区：文件树 / 消息流 / 产物列表
│       ├── widgets/           # 文件条目、产物卡片、消息气泡、工具块（edit diff 渲染骨架）
│       └── providers/         # Riverpod：workspacesProvider / workspaceDetailProvider / artifactsProvider
└── shared/
    └── ws_events.dart         # pi 事件解析（移植 Vue handlePiEvent 的骨架——本阶段只解析 work 相关：message_* / tool_execution_* / turn_artifacts / write_block）
```

## 六、mock 数据要求（严格对齐 mock-contract.md）

1. **Workspace**：至少 3 条固定样例（含 `mode: ask/allow/deny` 各一），字段完整（id/name/cwd/createdAt/updatedAt/fileCount/sizeBytes）
2. **文件树**：每 workspace 一组文件（含 `output/` 交付物目录 + 普通文件 + 子目录），`isDeliverable` 正确
3. **Artifacts**：按 turn 分组的样例（new/modified/deleted 三态都有，含 `output/report.md` 这种交付物）
4. **WS 事件流（MockWsClient）**：连接后模拟推送——`capabilities` → `sessions_list` → 若干 `message_start/update/end`（流式增量，模拟打字效果）→ `tool_execution_start/end`（含一个 **edit 工具带 `details.patch`**，验证 diff 渲染骨架）→ `turn_artifacts`
5. **写阻断**：mock 一个 `write_block` 事件（ask 模式），UI 显示批准条（批准/拒绝按钮，批准后调 `approve_write` 命令并更新状态）
6. **契约未定项**（mock-contract §7 开放项）按"当前基线"实现，字段名用下划线 snake_case

## 七、UI 要求（对标 TRAE Work）

- **响应式**：App 竖屏单列（消息流为主，文件/产物折叠入口）；Web 宽屏三栏（文件 | 消息 | 产物）
- **主题**：Material 3，seed 色 `#6a7a8a`（蓝灰，与 piter 设计系统一致）；支持暗色
- **关键交互**：
  - 工作空间列表：卡片（名称、文件数、大小、mode 徽标）+ 新建（对话框输入 name）
  - 详情页产物区：按 turn 分组展示，`deliverable` 高亮 + "下载/标记"按钮（本阶段点击显示 SnackBar"联调后可用"）
  - 工具块：edit 工具的 `details.patch` → **unified diff 渲染骨架**（本阶段自写轻量解析：`---/+++/@@` 行标记 + 行内 add/del 高亮）——这是最大自研点，优先做好
  - 写阻断批准条：消息流内嵌卡片（ask 模式），approve/deny 反馈
- **i18n**：本阶段中文字面量即可（不接 intl，避免范围膨胀）

## 八、验证标准（完成前必须全绿）

1. `flutter pub get` 成功
2. `flutter analyze` **0 issues**（严格，不允许 warning）
3. `flutter test` 全部通过（至少覆盖：契约模型 JSON 解析、workspacesProvider 状态流转、diff 解析器）
4. `flutter run -d chrome` 可打开：Web 端显示 work 列表 → 进详情 → 三区渲染 → mock 事件流驱动消息区动态更新 → write_block 批准条可交互
5. `flutter build web --release` 成功（确认无原生插件残留导致编译失败）
6. 代码组织遵循目录结构；mock 与真实实现隔离（未来换 ApiClient 真实实现时零 UI 改动）

## 九、工作方式与提交

1. **增量提交**：每个目录/模块完成即 `git commit`（仓库已初始化，分支 main），commit message 前缀 `feat(work):`
2. 参考模板的 `test/widget_test.dart` 已存在（Counter 示例），需替换为真实测试
3. 不要修改 `开发计划/` 目录（那是规划文档）；如发现契约矛盾，在 `work/docs/mock-contract.md` 追加「实现备注」段落记录（不改原契约）

## 十、开始

1. 先完整阅读 §二 的 5 份文档（尤其 mock-contract.md 和 Flutter工程规划.md）
2. 添加依赖（flutter_riverpod / dio / web_socket_channel / go_router / shared_preferences）
3. 从 `core/network/models/`（契约模型）→ `core/network/`（mock 数据源）→ `features/work/`（providers + UI）→ `app/`（壳与路由）顺序实现
4. 每步验证（§八），全部通过后汇总提交
