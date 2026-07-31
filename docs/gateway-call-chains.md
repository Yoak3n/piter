# Piter Gateway 功能调用路线图

本文档追踪每个核心功能的内部调用链，可用于对照实现和排查问题。

---

## A. 创建新会话 (new_session)

```
客户端发送 {"type":"new_session", "cwd":"/...", "name":"..."}
│
▼
ws/mod.rs :: route_ui_message()
├─ extract_cwd(&value)                    [ws/helper.rs]  — 从 payload/cwd 或顶层 cwd 取值，拒绝相对路径
│  └─ 失败 → notify_undeliverable("missing_or_invalid_cwd") → return
│
├─ 取 name（默认 "New Project"）                           [ws/mod.rs:131]
│
▼
session_manager.rs :: SessionManager::create_session(sm, gw, cwd, name, client_id)
│
├─ DB 查询: find_project_by_cwd_and_name(cwd, name)        [db.rs]
│  ├─ 找到 → 复用已有 project.id
│  └─ 未找到 →
│     └─ project::create_project(db, name, cwd, [])         [project.rs]
│        ├─ uuid::Uuid::new_v4() 生成 id
│        ├─ db.create_project(id, name, cwd)                 [db.rs]  — INSERT projects
│        └─ 返回 Project
│
├─ resolve_project_extensions(db, project_id, cwd)           [project.rs]
│  └─ db.get_project_extensions_with_paths(pid)              [db.rs]  — 直接从 DB 读取已索引的 (name, path) 对
│     ├─ path 有效且文件存在 → 直接使用
│     └─ path 为空或文件丢失 → 重新 resolve_extension_name(name, cwd) 并回写 DB
│
├─ spawn_persistent_for_gateway(gw, cwd, &extensions)        [handlers/pi.rs]
│  ├─ state.spawn().cwd(cwd).extensions(&exts).run()         [mod.rs]  — 启动 pi 子进程
│  ├─ routes.lock().insert(instance_id, instance_id)         — 自映射注册
│  ├─ event_tx 发送 {"type":"pi_started", "instanceId":..., "cwd":...}
│  └─ 返回 instance_id
│
├─ db.register_session(instance_id, cwd, project_id)         [db.rs]  — INSERT sessions (session_path=NULL)
│
├─ mgr.pending_links.insert(instance_id, project_id)
├─ 创建 ManagedSession（state=Active, messages=[], subscribers={client_id}, turn_count=0, title_set=false）
├─ mgr.sessions.insert(instance_id, session)
├─ mgr.dirty = true
│
▼ 回到 route_ui_message()
│
├─ push_sessions_list_to_clients(state)                      [mod.rs]
│  ├─ build_project_session_tree(state)                      — 构建 project-session 树
│  └─ broadcast_to_clients("sessions_list", projects)        — 推送给所有 WS 客户端
│
├─ 通过 instances 找到 pi → stdin_tx.send("get_state")       — 请求 pi 状态
│
├─ 发送给当前客户端: session_snapshot（messages=[], messageSeq=0）
└─ 发送给当前客户端: response {command:"new_session", success:true, instanceId}
```

**后续异步（pi 响应 get_state）**：

```
事件循环 process_broker_event()                              [mod.rs]
├─ event_type=="response", command=="get_state", success==true
├─ 提取 sessionFile
│  ├─ db.complete_session(instance_id, sessionFile)          [db.rs]  — UPDATE sessions SET session_path=?
│  └─ routes.lock().insert(sessionFile, instance_id)
├─ 清除 pending_link → push_sessions_list_to_clients()
├─ 提取 sessionId → routes.lock().insert(sessionId, instance_id)
├─ 解析 PiSessionState → session_manager.update_pi_state()   — 存储 model、session_name 等
└─ dirty=true → push_sessions_list_to_clients()
```

---

## B. 切换到已有会话 (switch_session)

```
客户端发送 {"type":"switch_session", "instanceId":"xxx"}
│
▼
ws/mod.rs :: route_ui_message()
├─ 提取 instanceId
│  └─ 缺失 → notify_undeliverable("missing_instanceId") → return
│
▼
session_manager.rs :: SessionManager::switch_session(sm, instance_id, client_id)
│
├─ 查找 sessions[instance_id]
│  ├─ 找到（Active 或 Idle）→
│  │  ├─ subscribers.insert(client_id)
│  │  ├─ state = Active
│  │  ├─ 更新 last_active / last_active_epoch
│  │  └─ 返回 SessionResult::Switched { messages, message_seq }
│  │
│  └─ 未找到（Unloaded 或不存在）→
│     └─ 返回 SessionResult::NeedSpawn { instance_id }
│
▼ 路径 A: Switched
│
├─ send_snapshot(client_tx, instance_id, messages, message_seq)  — 发送 session_snapshot
└─ tokio::spawn（延迟 500ms）
   └─ forward_to_instance(switch_session 命令给 pi stdin)        — 转发给 pi
│
▼ 路径 B: NeedSpawn
│
├─ db.all_sessions()                                             [db.rs]
│  └─ 找到匹配的 SessionRow（含 session_path, cwd）
├─ 确定 cwd（优先 payload，其次 DB）
│  └─ 缺失 → notify_undeliverable("missing_cwd") → return
│
├─ load_session(session_path)                                    [handlers/session.rs]
│  └─ 逐行读取 .jsonl → 筛选 type=="message" → 提取消息体
│
├─ resolve_project_extensions(db, cwd, cwd)
├─ state.spawn().cwd(&cwd).extensions(&exts).id(iid).session_path(sp).run()
│  └─ 使用持久化的 instance_id 和 session_path 恢复 pi
│
├─ routes.lock().insert(new_iid, new_iid)
│
├─ register_instance(sm, iid, cwd, client_id)                     [session_manager.rs]
│  └─ 创建 ManagedSession（Active），插入 mgr.sessions
│
├─ 将 existing_messages 写入 session.messages，设置 message_seq
│
├─ push_sessions_list_to_clients(state)
├─ send_snapshot(client_tx, iid, messages, message_seq)          — 包含历史消息
└─ tokio::spawn（延迟 800ms）
   └─ forward_to_instance(switch_session 命令给 pi stdin)
```

---

## C. 会话空闲超时清理

```
独立线程（每 60 秒执行一次）                                     [session_manager.rs :: spawn_cleanup_task]
│
├─ session_manager.lock().find_expired_sessions()
│  └─ 遍历所有 session
│     └─ 状态为 Idle { since } 且 now - since > idle_timeout（默认 600s）→ 加入过期列表
│
├─ 过期列表为空 → continue（等待下一轮）
│
├─ 对每个过期的 instance_id：
│  ├─ inner.instances.lock().remove(iid)                        — 移除实例
│  ├─ inst.running.store(false, SeqCst)                        — 标记停止
│  ├─ inst.child.kill()                                         — 杀死 pi 子进程
│
├─ session_manager.lock().mark_unloaded(&expired)
│  └─ 每个过期 session:
│     ├─ state = Unloaded
│     ├─ messages 清空
│     ├─ partial_message = None
│     └─ dirty = true
│
└─ event_tx.send({"type":"session_cleanup"})
   │
   ▼ 事件循环收到后
   └─ push_sessions_list_to_clients(state)                      — 推送更新给所有客户端
```

**生命周期**：`Active` → 客户端全部断开 → `Idle { since }` → 超时 → `Unloaded` + 进程被杀

---

## D. 客户端断开清理

```
WebSocket 循环终止（Close / 连接错误 / EOF）                      [ws/mod.rs :: handle_ws]
│
├─ ui_clients.lock().remove(client_id)                           — 从客户端表移除
│
├─ session_manager.lock().deactivate_all_for_client(client_id)   [session_manager.rs]
│  └─ 遍历所有 session:
│     ├─ subscribers.remove(client_id)
│     └─ 若 subscribers 为空 且 state == Active →
│        ├─ state = Idle { since: Instant::now() }
│        └─ dirty = true
│
└─ send_task.abort()                                             — 终止发送任务

后续：dirty=true → 事件循环推送 sessions_list → 进入 idle 超时倒计时
```

---

## E. 自动标题生成 (Auto-title)

```
pi 发出 MessageEnd 事件
│
▼
session_manager.rs :: on_event()
│
├─ 提取 role，若为 "user" 且 title_set == false：
│  ├─ extract_message_text(m)                                   [session_manager.rs]
│  │  ├─ content 为字符串 → 直接返回
│  │  └─ content 为数组 → 筛选 type=="text" 的块，拼接
│  │
│  └─ text.len() >= 10 → push 到 session.title_candidates
│
pi 发出 TurnEnd 事件
│
▼
├─ session.turn_count += 1
├─ 条件检查: !title_set && turn_count >= 2 && !title_candidates.is_empty()
│  └─ 满足 →
│     ├─ generate_session_title(&title_candidates)              [session_manager.rs]
│     │  ├─ 过滤空消息
│     │  ├─ 过滤纯问候语（hey/hello/hi/...）
│     │  ├─ 过滤 "read your memory/seed" 指令
│     │  ├─ 找第一条长度 >= 10 的消息
│     │  ├─ 去掉对话开头词（ok/so/hey/can you/i want to/...）
│     │  ├─ 取第一行
│     │  ├─ 提取第一个句子（在字符 10-80 间找 .!?）
│     │  ├─ 截断到 60 字符（最近空格处截断，加 "..."）
│     │  └─ 首字母大写
│     │
│     ├─ session.session_name = Some(title)
│     ├─ session.title_set = true
│     ├─ dirty = true
│     └─ pending_names.push((instance_id, title))
│
▼ 事件循环末尾                                                       [mod.rs]
│
├─ take_pending_names() → 取出待持久化列表
│  └─ 对每个 (iid, name):
│     └─ db.set_session_name(iid, name)                          [db.rs]  — UPDATE sessions SET name=?
│
└─ push_sessions_list_to_clients(state)                           — 推送更新给所有客户端
```

---

## F. Project CRUD

### F1. 创建项目

```
WS: {"type":"gateway_command", "command":"create_project", "data":{name, cwd, extensions?}}
或 HTTP: POST /api/projects  {name, cwd, extensions?}
│
▼
dispatch_gateway_command() 或 create_project_handler()
│
├─ project::create_project(db, name, cwd, extensions)           [project.rs]
│  ├─ uuid::Uuid::new_v4() → id
│  ├─ db.create_project(id, name, cwd)                          [db.rs]  — INSERT projects
│  └─ db.update_project(id, None, Some(&extensions))            [db.rs]  — DELETE + INSERT project_extensions
│
└─ 返回 Project → 响应客户端
```

### F2. 更新项目

```
WS: {"command":"update_project", "data":{id, name?, extensions?}}
或 HTTP: PUT /api/projects/:id  {name?, extensions?}
│
▼
project::update_project(db, id, name, extensions)                [project.rs]
├─ db.update_project(id, name, extensions)                       [db.rs]
│  ├─ name 非空 → UPDATE projects.name + updated_at
│  └─ extensions 非空 → DELETE 旧扩展 + INSERT 新扩展 + UPDATE updated_at
├─ db.get_project(id)                                            — 读取更新后的记录
├─ db.get_project_extensions(id)                                 — 读取扩展列表
└─ 返回 Project → 响应客户端
```

### F3. 删除项目

```
WS: {"command":"delete_project", "data":{id}}
或 HTTP: DELETE /api/projects/:id
│
▼
project::delete_project(db, id)                                  [project.rs]
└─ db.delete_project(id)                                         [db.rs]  — DELETE projects WHERE id=?
   ├─ project_extensions: ON DELETE CASCADE → 自动删除
   └─ sessions.project_id: ON DELETE SET NULL → 关联会话变为孤儿
```

### F4. 置顶 / F5. 归档

```
WS: {"command":"pin_project", "data":{id, pinned?}}
└─ db.set_pinned(id, pinned)                                     — UPDATE projects SET pinned=?, updated_at=?

WS: {"command":"archive_project", "data":{id, archived?}}
└─ db.set_archived(id, archived)                                 — UPDATE projects SET archived=?, updated_at=?
```

---

## G. 消息转发（pi command → response 管线）

```
客户端发送非特殊类型消息
│
▼
ws/mod.rs :: route_ui_message()
│
├─ resolve_command_instance(&value, state)                       [ws/helper.rs]
│  ├─ 取 value.instanceId 或 value./payload/instanceId
│  ├─ 找到 → 返回该 instanceId
│  └─ 未找到或未指定 → notify_undeliverable("no_route"/"missing_instanceId") → return
│
├─ forward_to_instance(text, value, instance_id, state, tx)      [ws/mod.rs]
│  ├─ broker_command 类型 → 提取 payload 字段序列化
│  ├─ 其他 → 原样转发
│  └─ instances[instance_id].stdin_tx.send(text)
│     └─ 失败 → notify_undeliverable("upstream_unavailable")
│
▼ pi 进程处理后通过 stdout 返回事件
│
├─ event_tx 广播
│
▼ 事件循环 run_event_loop() → process_broker_event()               [mod.rs]
│
├─ 若 response 命令 → 路由表更新、get_state 处理（参见 A 的异步部分）
│
├─ session_manager.lock().on_event(&val, &instance_id)            [session_manager.rs]
│  ├─ 更新 last_active / last_active_epoch
│  ├─ 按事件类型更新消息跟踪（MessageStart/Update/End/MirrorSync/TurnEnd/...）
│  ├─ 收集标题候选文本（参见 E）
│  └─ 返回 message_seq
│
├─ 信封包装:
│  ├─ pi 生命周期事件 → {"type":"event", "event":val, "instanceId", "messageSeq", "protocolVersion"}
│  └─ 其他事件 → {"type":event_type, "payload":val, "instanceId", "messageSeq", "protocolVersion"}
│
├─ 分发:
│  ├─ 有 subscribers → broadcast_to_subscribers()                 — 仅发给订阅者
│  └─ 无 subscribers → broadcast_to_clients()                     — 发给所有客户端
│
├─ 特定事件（agent_end/turn_end）→ push_sessions_list_to_clients()
└─ dirty 或 pending_names 非空 → push_sessions_list_to_clients()
```

---

## H. 会话删除

```
WS: {"command":"delete_session", "data":{instanceId}}
或 HTTP: GET /api/delete-session?instanceId=...
│
▼
handlers/session.rs :: delete_session(instance_id, state)
│
├─ 第 1 步：获取 session_file 路径
│  ├─ db.get_session_path(instance_id)                           [db.rs]
│  └─ 回退: session_manager.sessions[iid].pi_state.session_file
│
├─ 第 2 步：杀死运行中的进程
│  └─ kill_instance_for_gateway(state, instance_id)              [handlers/pi.rs]
│     ├─ instances.lock().remove(instance_id)
│     ├─ inst.running.store(false, SeqCst)
│     ├─ inst.child.kill()
│     ├─ routes.lock().retain(|_, v| v != instance_id)           — 清除路由
│
├─ 第 3 步：移除内存状态
│  ├─ session_manager.sessions.remove(instance_id)
│  └─ pending_links.remove(instance_id)
│
├─ 第 4 步：移除 DB 记录
│  └─ db.delete_session_by_instance(instance_id)                 [db.rs]  — DELETE sessions
│
├─ 第 5 步：删除磁盘文件
│  └─ std::fs::remove_file(session_file)                         — 删除 .jsonl
│
└─ 第 6 步：广播更新
   └─ push_sessions_list_to_clients(state)
```

---

## 三层状态存储

| 层级 | 存储位置 | 持久性 | 内容 |
|------|---------|--------|------|
| 进程表 | `BrokerInner.instances` (HashMap) | 运行时 | pi 进程句柄、stdin_tx、cwd、session_path、running 标志 |
| 会话管理器 | `SessionManager.sessions` (HashMap) | 运行时 | 消息历史、subscribers、状态(Active/Idle/Unloaded)、pi_state、标题数据 |
| SQLite | `Db` (piter.db) | 持久 | projects、sessions(instance_id/session_path/project_id/cwd/name)、extensions(含已索引的 extension_path) |

## 路由表 (BrokerInner.routes)

三种键映射到同一个 instance_id：

| 键来源 | 注册时机 |
|--------|---------|
| `instance_id → instance_id` | spawn 时（自映射） |
| `session_file → instance_id` | pi 响应 get_state 时 |
| `session_id → instance_id` | pi 响应 get_state 时 |
