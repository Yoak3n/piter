/// Work 会话状态 + 纯函数 reducer（移植 Vue handlePiEvent 骨架，只处理 work 相关事件）。
///
/// reducer 与 Riverpod 解耦，便于单元测试；Notifier 仅做连接与订阅薄壳。
library;

import '../../../core/network/models/models.dart';
import '../../../shared/ws_events.dart';

// ─── 时间线条目（消息区按序渲染：消息 / 工具 / 写阻断）─────────────────────

sealed class TimelineEntry {
  const TimelineEntry();
}

/// 一条消息气泡（streaming=true 表示正在流式输出）。
class MessageEntry extends TimelineEntry {
  const MessageEntry({required this.message, this.streaming = false});

  final PiMessage message;
  final bool streaming;

  MessageEntry copyWith({PiMessage? message, bool? streaming}) => MessageEntry(
        message: message ?? this.message,
        streaming: streaming ?? this.streaming,
      );
}

/// 一个工具块（edit 带 details.patch → diff 渲染）。
class ToolEntry extends TimelineEntry {
  const ToolEntry({required this.tool});

  final ToolExecution tool;
}

enum WriteBlockState { pending, approved, denied }

/// 写阻断批准条目。
class WriteBlockEntry extends TimelineEntry {
  const WriteBlockEntry({required this.block, this.state = WriteBlockState.pending});

  final WriteBlockEvent block;
  final WriteBlockState state;

  WriteBlockEntry copyWith({WriteBlockState? state}) =>
      WriteBlockEntry(block: block, state: state ?? this.state);
}

/// 扩展 UI 请求卡片（ask_user_question 等交互插件；未应答阻塞 pi，应答后只读历史）。
class WorkExtEntry extends TimelineEntry {
  const WorkExtEntry({required this.ui});

  final ChatExtUi ui;
}

// ─── 会话状态 ───────────────────────────────────────────────────────────────

class WorkSessionState {
  const WorkSessionState({
    this.connected = false,
    this.reconnectFailed = false,
    this.instanceId = '',
    this.entries = const [],
    this.liveArtifacts = const [],
    this.turnCount = 0,
  });

  final bool connected;

  /// 自动重连耗尽（服务端长时间不可达），等待用户手动重试。
  final bool reconnectFailed;

  /// 工作空间会话的 pi 实例 id（create_workspace_session 响应回填；发 prompt 用）。
  final String instanceId;

  final List<TimelineEntry> entries;

  /// 实时产物（turn_artifacts 累积，驱动产物区"实时"分组）。
  final List<TurnArtifactItem> liveArtifacts;
  final int turnCount;

  WorkSessionState copyWith({
    bool? connected,
    bool? reconnectFailed,
    String? instanceId,
    List<TimelineEntry>? entries,
    List<TurnArtifactItem>? liveArtifacts,
    int? turnCount,
  }) =>
      WorkSessionState(
        connected: connected ?? this.connected,
        reconnectFailed: reconnectFailed ?? this.reconnectFailed,
        instanceId: instanceId ?? this.instanceId,
        entries: entries ?? this.entries,
        liveArtifacts: liveArtifacts ?? this.liveArtifacts,
        turnCount: turnCount ?? this.turnCount,
      );
}

// ─── Reducer ────────────────────────────────────────────────────────────────

/// 应用一个 WS 事件，返回新状态（不可变）。
WorkSessionState reduceWorkSession(WorkSessionState state, WsEvent event) {
  return switch (event) {
    MessageEvent() => _reduceMessage(state, event),
    ToolEvent() => _reduceTool(state, event),
    TurnArtifactsEvent() => _reduceArtifacts(state, event),
    WriteBlockEvent() => _reduceWriteBlock(state, event),
    GatewayResponseEvent() => _reduceGatewayResponse(state, event),
    SessionSnapshotEvent() => _reduceSnapshot(state, event),
    TurnEndEvent() => state.copyWith(turnCount: state.turnCount + 1),
    CapabilitiesEvent() => state,
    SessionsListEvent() => state,
    AgentEndEvent() => state,
    // 交互插件扩展 UI 请求（ask_user_question 等；非阻塞方法在 reducer 内过滤）。
    ExtUiRequestEvent() => _reduceExtUi(state, event),
    // chat 专用事件（work 不消费）。
    CommandResponseEvent() => state,
    QueueUpdateEvent() => state,
    SessionStatusEvent() => state,
    PiErrorEvent() => state,
    AutoRetryEvent() => state,
    SystemNoticeEvent() => state,
    UnknownEvent() => state,
  };
}

WorkSessionState _reduceMessage(WorkSessionState state, MessageEvent evt) {
  final entries = [...state.entries];
  switch (evt.phase) {
    case 'start':
      // 只处理 assistant：user 消息由 sendPrompt 本地回显（对齐 Vue chat，
      // 不依赖服务端回推 user message_start，避免双写重复）。
      if (evt.message?.role == PiMessageRole.assistant) {
        // 开始一条流式助手消息
        entries.add(MessageEntry(
          message: evt.message ?? const PiMessage(role: PiMessageRole.assistant, content: ''),
          streaming: true,
        ));
      }
    case 'update':
      final last = entries.isEmpty ? null : entries.last;
      if (last is MessageEntry && last.streaming && evt.delta.isNotEmpty) {
        final content = last.message.content + evt.delta;
        entries[entries.length - 1] = last.copyWith(
          message: PiMessage(role: last.message.role, content: content),
        );
      }
    case 'end':
      final last = entries.isEmpty ? null : entries.last;
      if (last is MessageEntry && last.streaming) {
        final content = evt.message?.content.isNotEmpty == true
            ? evt.message!.content
            : last.message.content;
        entries[entries.length - 1] = last.copyWith(
          message: PiMessage(role: last.message.role, content: content),
          streaming: false,
        );
      }
  }
  return state.copyWith(entries: entries);
}

WorkSessionState _reduceTool(WorkSessionState state, ToolEvent evt) {
  final entries = [...state.entries];
  switch (evt.phase) {
    case 'start':
      entries.add(ToolEntry(
        tool: ToolExecution(
          toolCallId: evt.toolCallId,
          toolName: evt.toolName,
          status: ToolExecutionStatus.pending,
          args: evt.args,
        ),
      ));
    case 'update':
      _updateTool(entries, evt,
          status: ToolExecutionStatus.streaming, output: evt.output);
    case 'end':
      _updateTool(entries, evt,
          status: evt.isError ? ToolExecutionStatus.error : ToolExecutionStatus.complete,
          output: evt.output,
          isError: evt.isError);
  }
  return state.copyWith(entries: entries);
}

void _updateTool(
  List<TimelineEntry> entries,
  ToolEvent evt, {
  required ToolExecutionStatus status,
  required String output,
  bool isError = false,
}) {
  for (var i = entries.length - 1; i >= 0; i--) {
    final e = entries[i];
    if (e is ToolEntry && e.tool.toolCallId == evt.toolCallId) {
      final prev = e.tool;
      entries[i] = ToolEntry(
        tool: prev.copyWith(
          status: status,
          output: output.isEmpty ? prev.output : output,
          isError: isError,
        ),
      );
      return;
    }
  }
}

WorkSessionState _reduceArtifacts(WorkSessionState state, TurnArtifactsEvent evt) {
  // 新→旧：本次事件 items 置顶，旧条目按 path 去重保留。
  final seen = <String>{};
  final merged = <TurnArtifactItem>[...evt.items];
  for (final item in evt.items) {
    seen.add(item.path);
  }
  for (final old in state.liveArtifacts) {
    if (seen.add(old.path)) merged.add(old);
  }
  return state.copyWith(liveArtifacts: merged);
}

WorkSessionState _reduceWriteBlock(WorkSessionState state, WriteBlockEvent evt) {
  return state.copyWith(
    entries: [...state.entries, WriteBlockEntry(block: evt)],
  );
}

/// 交互插件扩展 UI 请求：非阻塞方法（notify/setStatus/setWidget/setTitle/set_editor_text）
/// 不进时间线（toast/状态由 UI 层另行处理）；阻塞方法以卡片形式插入时间线等待应答。
WorkSessionState _reduceExtUi(WorkSessionState state, ExtUiRequestEvent evt) {
  if (evt.method == 'notify' || evt.method == 'setStatus' ||
      evt.method == 'setWidget' || evt.method == 'setTitle' ||
      evt.method == 'set_editor_text') {
    return state;
  }
  return state.copyWith(entries: [
    ...state.entries,
    WorkExtEntry(
      ui: ChatExtUi(
        id: evt.id,
        method: evt.method,
        title: evt.title,
        message: evt.message,
        placeholder: evt.placeholder,
        prefill: evt.prefill,
        options: evt.options,
        timeout: evt.timeout,
        createdAt: DateTime.now().millisecondsSinceEpoch,
      ),
    ),
  ]);
}

/// session_snapshot：历史消息回放为时间线（按文件序）。toolResult 角色不单独
/// 渲染气泡（工具块由 tool_execution 事件表达），与流式路径行为一致。
WorkSessionState _reduceSnapshot(WorkSessionState state, SessionSnapshotEvent evt) {
  final history = <TimelineEntry>[
    for (final m in evt.messages)
      if (m.role != PiMessageRole.toolResult) MessageEntry(message: m),
  ];
  // 保留本地未应答的扩展 UI 卡片：服务端快照不携带 extUi，若不保留，
  // 断线重连（create_ws 重推快照覆盖 entries）后 pi 仍阻塞等待应答而卡片已丢。
  final pending = state.entries
      .whereType<WorkExtEntry>()
      .where((e) => !e.ui.answered);
  return state.copyWith(entries: [...history, ...pending]);
}

WorkSessionState _reduceGatewayResponse(WorkSessionState state, GatewayResponseEvent evt) {
  // create_workspace_session 应答 → 回填会话 instanceId（发 prompt 的前置）。
  if (evt.requestId == 'create_ws') {
    final iid = evt.data?['instanceId'];
    if (evt.success && iid is String && iid.isNotEmpty) {
      return state.copyWith(instanceId: iid);
    }
    return state;
  }
  if (evt.requestId.isEmpty) return state;
  final entries = [...state.entries];
  for (var i = entries.length - 1; i >= 0; i--) {
    final e = entries[i];
    if (e is WriteBlockEntry && e.block.requestId == evt.requestId) {
      final approved = evt.success && evt.data?['approved'] == true;
      entries[i] = e.copyWith(
        state: evt.success
            ? (approved ? WriteBlockState.approved : WriteBlockState.denied)
            : WriteBlockState.pending,
      );
      return state.copyWith(entries: entries);
    }
  }
  return state;
}
