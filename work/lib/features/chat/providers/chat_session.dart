/// 原生 chat 会话状态 + 纯函数 reducer（移植 Vue handlePiEvent 语义，
/// 保留 thinking/toolCall/image 结构；与 work 的 workspace 会话解耦）。
///
/// reducer 与 Riverpod 解耦，便于单元测试；Notifier 只做连接/订阅/发送薄壳。
library;

import '../../../core/network/models/models.dart';
import '../../../shared/ws_events.dart';

// ─── 时间线条目 ────────────────────────────────────────────────────────────

sealed class ChatEntry {
  const ChatEntry();
}

/// 一条消息（user 右 / assistant 左 / system 居中）。流式时 textBuffer /
/// thinkingBuffer 累积增量，message_end 定型为 blocks。
class ChatMsgEntry extends ChatEntry {
  const ChatMsgEntry({
    required this.message,
    this.streaming = false,
    this.textBuffer = '',
    this.thinkingBuffer = '',
    this.toolResults = const {},
  });

  final ChatMessage message;
  final bool streaming;
  final String textBuffer;
  final String thinkingBuffer;

  /// 快照中 toolResult 平级消息的折叠结果：toolCallId → output。
  final Map<String, String> toolResults;

  ChatMsgEntry copyWith({
    ChatMessage? message,
    bool? streaming,
    String? textBuffer,
    String? thinkingBuffer,
    Map<String, String>? toolResults,
  }) =>
      ChatMsgEntry(
        message: message ?? this.message,
        streaming: streaming ?? this.streaming,
        textBuffer: textBuffer ?? this.textBuffer,
        thinkingBuffer: thinkingBuffer ?? this.thinkingBuffer,
        toolResults: toolResults ?? this.toolResults,
      );
}

/// 运行时工具块（tool_execution_* 三态驱动）。
class ChatToolEntry extends ChatEntry {
  const ChatToolEntry({required this.tool});
  final ToolExecution tool;
}

/// 系统提示（错误 / 重试 / 投递失败等）。
class ChatNoticeEntry extends ChatEntry {
  const ChatNoticeEntry({required this.kind, required this.message});
  final String kind;
  final String message;
}

/// 扩展 UI 卡片（未应答阻塞请求；应答后只读历史）。
class ChatExtEntry extends ChatEntry {
  const ChatExtEntry({required this.ui});
  final ChatExtUi ui;
}

// ─── 会话状态 ──────────────────────────────────────────────────────────────

class ChatSessionState {
  const ChatSessionState({
    this.connected = false,
    this.instanceId = '',
    this.entries = const [],
    this.streaming = false,
    this.status = 'idle',
    this.queue = const [],
    this.slashCommands = const [],
    this.outbox = const [],
  });

  final bool connected;

  /// 活动会话的 pi 实例 id（switch_session/new_session 回填；发 prompt 用）。
  final String instanceId;

  final List<ChatEntry> entries;
  final bool streaming;

  /// running | idle
  final String status;

  /// pi 插队队列（steering 文本，只读展示）。
  final List<String> queue;

  /// 斜杠命令缓存（get_commands 结果，按会话失效）。
  final List<SlashCommand> slashCommands;

  /// 流式中排队待投递的 prompt 文本（agent_end 后投递最新一条）。
  final List<String> outbox;

  ChatSessionState copyWith({
    bool? connected,
    String? instanceId,
    List<ChatEntry>? entries,
    bool? streaming,
    String? status,
    List<String>? queue,
    List<SlashCommand>? slashCommands,
    List<String>? outbox,
  }) =>
      ChatSessionState(
        connected: connected ?? this.connected,
        instanceId: instanceId ?? this.instanceId,
        entries: entries ?? this.entries,
        streaming: streaming ?? this.streaming,
        status: status ?? this.status,
        queue: queue ?? this.queue,
        slashCommands: slashCommands ?? this.slashCommands,
        outbox: outbox ?? this.outbox,
      );
}

// ─── Reducer ────────────────────────────────────────────────────────────────

/// 应用一个 WS 事件，返回新状态（不可变）。
ChatSessionState reduceChatSession(ChatSessionState state, WsEvent event) {
  return switch (event) {
    MessageEvent() => _reduceMessage(state, event),
    ToolEvent() => _reduceTool(state, event),
    SessionSnapshotEvent() => _reduceSnapshot(state, event),
    ExtUiRequestEvent() => _reduceExtUi(state, event),
    CommandResponseEvent() => _reduceResponse(state, event),
    QueueUpdateEvent() => state.copyWith(queue: event.steering),
    SessionStatusEvent() => state.copyWith(status: event.status),
    PiErrorEvent() => _reduceError(state, event),
    AutoRetryEvent() => _reduceAutoRetry(state, event),
    SystemNoticeEvent() => _reduceNotice(state, event),
    AgentEndEvent() => _finalizeStreaming(state),
    TurnEndEvent() => _finalizeStreaming(state),
    CapabilitiesEvent() => state.copyWith(connected: true),
    TurnArtifactsEvent() => state,
    WriteBlockEvent() => state,
    SessionsListEvent() => state,
    GatewayResponseEvent() => state,
    UnknownEvent() => state,
  };
}

// ─── 消息（流式 text/thinking 组装）────────────────────────────────────────

ChatSessionState _reduceMessage(ChatSessionState state, MessageEvent evt) {
  final entries = [...state.entries];
  switch (evt.phase) {
    case 'start':
      // 只处理 assistant：user 消息由 sendPrompt 本地回显，避免双写重复。
      if (evt.rawMessage != null &&
          (evt.rawMessage!['role'] as String? ?? '') == 'assistant') {
        entries.add(ChatMsgEntry(
          message: ChatMessage.fromSnapshotJson(evt.rawMessage!),
          streaming: true,
        ));
        return state.copyWith(entries: entries, streaming: true);
      }
      return state;
    case 'update':
      final idx = _lastStreamingIndex(entries);
      if (idx < 0) {
        // 无流式条目则新建（兼容缺 message_start 的时序）。
        entries.add(ChatMsgEntry(
          message: const ChatMessage(role: 'assistant'),
          streaming: true,
        ));
        return _appendDelta(state, entries, entries.length - 1, evt);
      }
      return _appendDelta(state, entries, idx, evt);
    case 'end':
      final idx = _lastStreamingIndex(entries);
      if (idx < 0) return state;
      final current = entries[idx] as ChatMsgEntry;
      if (evt.rawMessage != null) {
        final full = ChatMessage.fromSnapshotJson(evt.rawMessage!);
        // 定型：正文取完整消息的 blocks；思考保留流式缓冲（快照块可能不含思考）。
        entries[idx] = current.copyWith(
          message: ChatMessage(
            role: 'assistant',
            blocks: full.blocks,
            model: full.model,
            timestamp: full.timestamp,
          ),
          streaming: false,
          textBuffer: '',
        );
      } else {
        entries[idx] = current.copyWith(streaming: false, textBuffer: '');
      }
      return state.copyWith(entries: entries, streaming: false);
  }
  return state;
}

int _lastStreamingIndex(List<ChatEntry> entries) {
  for (var i = entries.length - 1; i >= 0; i--) {
    final e = entries[i];
    if (e is ChatMsgEntry && e.streaming && e.message.role == 'assistant') return i;
  }
  return -1;
}

ChatSessionState _appendDelta(
    ChatSessionState state, List<ChatEntry> entries, int idx, MessageEvent evt) {
  final current = entries[idx] as ChatMsgEntry;
  if (evt.deltaType == 'thinking') {
    entries[idx] = current.copyWith(thinkingBuffer: current.thinkingBuffer + evt.delta);
  } else {
    entries[idx] = current.copyWith(textBuffer: current.textBuffer + evt.delta);
  }
  return state.copyWith(entries: entries, streaming: true);
}

// ─── 回合结束兜底定型（agent_end / turn_end）─────────────────────────────

/// 结束流式：把最后一条流式助手消息定型（保留缓冲文本，标记非流式）。
ChatSessionState _finalizeStreaming(ChatSessionState state) {
  final entries = [...state.entries];
  final idx = _lastStreamingIndex(entries);
  if (idx >= 0) {
    final cur = entries[idx] as ChatMsgEntry;
    entries[idx] = cur.copyWith(
      streaming: false,
      message: cur.message.blocks.isEmpty && cur.textBuffer.isNotEmpty
          ? ChatMessage(role: 'assistant', blocks: [TextBlock(cur.textBuffer)])
          : cur.message,
      textBuffer: '',
    );
  }
  return state.copyWith(entries: entries, streaming: false);
}

// ─── 工具（运行时三态）──────────────────────────────────────────────────────

ChatSessionState _reduceTool(ChatSessionState state, ToolEvent evt) {
  final entries = [...state.entries];
  switch (evt.phase) {
    case 'start':
      entries.add(ChatToolEntry(
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
  List<ChatEntry> entries,
  ToolEvent evt, {
  required ToolExecutionStatus status,
  required String output,
  bool isError = false,
}) {
  for (var i = entries.length - 1; i >= 0; i--) {
    final e = entries[i];
    if (e is ChatToolEntry && e.tool.toolCallId == evt.toolCallId) {
      final prev = e.tool;
      entries[i] = ChatToolEntry(
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

// ─── 快照（历史回放：消息 + toolCall/toolResult 折叠）────────────────────

ChatSessionState _reduceSnapshot(ChatSessionState state, SessionSnapshotEvent evt) {
  final entries = <ChatEntry>[];
  // toolCallId → toolResult 输出（先扫一遍平级 toolResult 消息）。
  final toolOutputs = <String, ({String output, String name})>{};
  for (final raw in evt.rawMessages) {
    final role = raw['role'] as String? ?? '';
    if (role == 'toolResult') {
      final id = raw['toolCallId'] as String? ?? '';
      if (id.isNotEmpty) {
        toolOutputs[id] = (
          output: raw['output'] is String ? raw['output'] as String : '',
          name: raw['toolName'] as String? ?? '',
        );
      }
    }
  }
  for (final raw in evt.rawMessages) {
    final role = raw['role'] as String? ?? '';
    if (role == 'toolResult') continue;
    final msg = ChatMessage.fromSnapshotJson(raw);
    if (role == 'assistant') {
      // 把消息内 toolCall block 提出来渲染为工具条目，输出由 toolResult 折叠。
      final calls = msg.blocks.whereType<ToolCallBlock>().toList();
      for (final call in calls) {
        final result = toolOutputs[call.id];
        entries.add(ChatToolEntry(
          tool: ToolExecution(
            toolCallId: call.id,
            toolName: call.name,
            status: ToolExecutionStatus.complete,
            args: call.args,
            output: result?.output ?? '',
            isError: false,
          ),
        ));
      }
    }
    final extUi = msg.extUi;
    if (extUi != null) {
      entries.add(ChatExtEntry(ui: extUi));
    } else {
      entries.add(ChatMsgEntry(message: msg));
    }
  }
  return state.copyWith(entries: entries, instanceId: evt.instanceId, streaming: false);
}

// ─── 扩展卡片 ──────────────────────────────────────────────────────────────

ChatSessionState _reduceExtUi(ChatSessionState state, ExtUiRequestEvent evt) {
  if (evt.method == 'notify' || evt.method == 'setStatus' ||
      evt.method == 'setWidget' || evt.method == 'setTitle' ||
      evt.method == 'set_editor_text') {
    // 非阻塞方法：不进消息流（toast/状态由 UI 层处理）。
    return state;
  }
  return state.copyWith(entries: [
    ...state.entries,
    ChatExtEntry(
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

// ─── 命令应答（new_session 回填 instanceId / get_commands 缓存）────────────

ChatSessionState _reduceResponse(ChatSessionState state, CommandResponseEvent evt) {
  switch (evt.command) {
    case 'new_session':
      if (!evt.success) return state;
      final iid = evt.data?['instanceId'];
      return iid is String && iid.isNotEmpty ? state.copyWith(instanceId: iid) : state;
    case 'get_commands':
      final list = evt.data?['commands'];
      if (list is! List) return state;
      return state.copyWith(
        slashCommands: list
            .whereType<Map<String, dynamic>>()
            .map(SlashCommand.fromJson)
            .toList(),
      );
    default:
      return state;
  }
}

// ─── 错误 / 重试 / 系统提示 ────────────────────────────────────────────────

ChatSessionState _reduceError(ChatSessionState state, PiErrorEvent evt) {
  if (evt.aborted) {
    // 停止：保留已输出内容，仅结束流式。
    final entries = [...state.entries];
    final idx = _lastStreamingIndex(entries);
    if (idx >= 0) {
      final cur = entries[idx] as ChatMsgEntry;
      entries[idx] = cur.copyWith(
        streaming: false,
        message: cur.message.blocks.isEmpty && cur.textBuffer.isNotEmpty
            ? ChatMessage(
                role: 'assistant',
                blocks: [TextBlock(cur.textBuffer)],
              )
            : cur.message,
        textBuffer: '',
      );
    }
    return state.copyWith(entries: entries, streaming: false);
  }
  final message = evt.message ?? evt.reason ?? evt.error ?? '发生错误';
  return state.copyWith(
    streaming: false,
    entries: [...state.entries, ChatNoticeEntry(kind: 'error', message: message)],
  );
}

ChatSessionState _reduceAutoRetry(ChatSessionState state, AutoRetryEvent evt) {
  if (evt.phase == 'start') {
    return state.copyWith(entries: [
      ...state.entries,
      ChatNoticeEntry(
        kind: 'retry',
        message: '[重试 ${evt.attempt}/${evt.maxAttempts}] ${evt.errorMessage ?? ''}',
      ),
    ]);
  }
  if (!evt.success) {
    return state.copyWith(
      streaming: false,
      entries: [
        ...state.entries,
        ChatNoticeEntry(kind: 'error', message: '[错误] ${evt.finalError ?? ''}'),
      ],
    );
  }
  return state;
}

ChatSessionState _reduceNotice(ChatSessionState state, SystemNoticeEvent evt) {
  return state.copyWith(entries: [
    ...state.entries,
    ChatNoticeEntry(kind: evt.kind, message: evt.message),
  ]);
}
