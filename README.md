# Piter

Piter 是一个 AI 编程助手客户端，以 Tauri 桌面应用和 Web UI（Chat）两种形式提供，通过 WebSocket + REST API 驱动后端管理的 pi coding agent 进程。

## 功能特性

- **多会话并行**：一个项目可同时运行多个独立会话，支持切换、删除、恢复与自动命名；空闲会话自动卸载以节省资源
- **项目管理**：项目与工作目录绑定，支持 CRUD、置顶、归档，以及项目级扩展配置
- **使用统计面板**：聚合 pi 会话文件，提供费用/Token/活跃度等 7d / 30d / 90d 维度统计与活动热力图
- **Provider 认证管理**：读写 `~/.pi/agent/auth.json`（0600 权限），内置 30+ 已知 Provider 与 OAuth 订阅条目；自定义 Provider 编辑 `~/.pi/agent/models.json`
- **Pi 版本管理**：锁定 pi 版本，内置下载、安装、卸载与版本切换
- **扩展与包市场**：查看/启用/禁用全局与项目级扩展，通过 `pi install` 管理包市场
- **局域网访问**：同一局域网内其他设备可通过浏览器直接使用 Chat 界面，支持二维码分享与移动端模式
- **双形态**：Tauri 桌面应用（系统托盘、开机自启、主题系统）+ 轻量 Web 聊天界面

## 安装

### 桌面应用（推荐）

**Windows**：从 Release 页面下载 NSIS 安装包，或本地构建：

```bash
pnpm install
pnpm tauri build
```

构建产物位于 `src-tauri/target/release/bundle/nsis/`。

**macOS / Linux**：需要源码构建（`bundle.targets` 目前仅配置了 `nsis`）：

```bash
pnpm install
pnpm tauri build
```

> 源码构建的前置要求：Node.js >= 18、pnpm、Rust 工具链、Tauri CLI（随 devDependencies 安装）。

### Web 版（开发 / 自托管）

仅运行 Chat 前端（需要单独启动 pi_server）：

```bash
pnpm install
cd chat && pnpm dev
```

## 首次使用

Piter 依赖 [pi coding agent](https://github.com/earendil-works/pi)（当前锁定版本 `v0.80.3`，见 `scripts/pi-version.json`）作为对话引擎。首次启动需完成两步配置：

### 1. 下载 pi

- **桌面端**：打开管理面板 **Settings → Pi Config / Versions**，在 Versions 选项卡中下载并安装 pi。安装完成后 Gateway 自动启动；若未安装 pi，启动时会提示前往 Settings > Versions 下载
- **自动发现**：Piter 也会自动发现已有的 pi 安装，包括 PATH 中的 `pi`、npm / bun 全局安装的 `@earendil-works/pi-coding-agent`、Picot 自带 pi、scoop / homebrew 安装等，无需重复下载

### 2. 配置 Provider

打开管理面板 **Settings → Providers**：

- 在列表中选择 Provider 并填写 API Key，Piter 会写入 `~/.pi/agent/auth.json`（0600 权限）；OAuth 订阅（如 ChatGPT / Claude 订阅计划）也在此管理
- 支持 30+ 已知 Provider，如 Anthropic、OpenAI、DeepSeek、Google Gemini、xAI、OpenRouter、Kimi、MiniMax、Groq、Mistral 等
- 自定义 Provider（`baseUrl` / `api` / `compat` / `models`）需编辑 `~/.pi/agent/models.json`
- 更完整的 Provider 配置说明见 [pi 的 providers 文档](https://github.com/earendil-works/pi)（`packages/coding-agent/docs/providers.md`）

### 3. 开始对话

创建项目（绑定工作目录）→ 新建会话 → 选择模型 → 开始对话。多会话可并行运行，会话空闲超过默认 10 分钟自动卸载。

## 局域网访问

Piter 的 Gateway 监听 `0.0.0.0`，同一局域网内的设备可以直接通过浏览器访问 Chat 界面。

- **查看地址与二维码**：在 Chat 界面顶部的分享按钮中可查看局域网访问地址并生成二维码
- **桌面访问地址**：`http://<局域网IP>:<端口>/chat`
- **移动端访问地址**：`http://<局域网IP>:<端口>/chat?brokerWs=ws://<局域网IP>:<端口>/ws&mobile=1`
- **注意事项**：
  - 设备需与运行 Piter 的主机处于同一局域网
  - 如无法访问，请检查系统防火墙是否放行了 Piter 使用的端口
  - 局域网分享面向受信网络；Gateway 不做身份鉴权，请勿在公网或不信任网络环境下开放

## 开发者文档

项目结构、架构概览、WebSocket 协议、REST API、开发环境搭建等内容见 [docs/developer-guide.md](docs/developer-guide.md)；API 详细契约见 [docs/gateway-api-reference.md](docs/gateway-api-reference.md)。
