/// 原生 chat 会话 Notifier：连接 /chat-ws、订阅事件、命令发送。
/// 状态推进全部走纯函数 reduceChatSession，本类只做薄壳。
library;

import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../core/network/models/models.dart';
import '../../../core/network/ws_client.dart';
import '../../../shared/ws_events.dart';
import '../../work/providers/data_sources.dart';
import 'chat_session.dart';

final chatSessionProvider =
    NotifierProvider<ChatSessionNotifier, ChatSessionState>(ChatSessionNotifier.new);

class ChatSessionNotifier extends Notifier<ChatSessionState> {
  WsClient? _client;
  StreamSubscription<WsEvent>? _sub;

  @override
  ChatSessionState build() {
    ref.onDispose(disconnect);
    return const ChatSessionState();
  }

  /// 建立 /chat-ws 连接并订阅事件（幂等）。
  Future<void> connect() async {
    if (state.connected) return;
    final client = ref.read(chatWsClientProvider);
    _client = client;
    _sub = client.events.listen(_onEvent);
    await client.connect();
  }

  void _onEvent(WsEvent event) {
    // agent_end 后投递 outbox 最新一条（对齐 Vue：流式中消息进队列，回合结束补发）。
    if (event is AgentEndEvent) {
      final outbox = state.outbox;
      final next = outbox.isEmpty ? null : outbox.last;
      if (next != null) {
        state = state.copyWith(outbox: const []);
        _sendPromptRaw(next);
      }
    }
    state = reduceChatSession(state, event);
  }

  /// 打开会话：switch_session + ack_review（恢复 WaitingReview 态）。
  Future<void> switchSession(String instanceId) async {
    if (_client == null) return;
    state = const ChatSessionState(connected: true);
    _client!.sendBrokerCommand(instanceId, {
      'type': 'switch_session',
      'instanceId': instanceId,
    });
    _client!.sendBrokerCommand(instanceId, {
      'type': 'ack_review',
      'instanceId': instanceId,
    });
  }

  /// 新建会话（cwd 指定项目工作目录；name 可空）。
  Future<void> newSession({String? cwd, String? name, String? modelId, String? provider}) async {
    if (_client == null) return;
    state = state.copyWith(entries: const [], instanceId: '');
    _client!.sendBrokerCommand('', {
      'type': 'new_session',
      if (cwd != null && cwd.isNotEmpty) 'cwd': cwd,
      if (name != null && name.isNotEmpty) 'name': name,
      if (modelId != null && modelId.isNotEmpty)
        'model': {'id': modelId, if (provider != null && provider.isNotEmpty) 'provider': provider},
    });
  }

  /// 发送 prompt：流式中先入 outbox，否则立即发送（均本地回显 user 消息）。
  void sendPrompt(
    String text, {
    String? modelId,
    String? provider,
    List<Map<String, dynamic>>? images,
  }) {
    if (text.trim().isEmpty) return;
    _echoUser(text, images: images);
    if (state.streaming) {
      state = state.copyWith(outbox: [...state.outbox, text]);
      return;
    }
    _sendPromptRaw(text, modelId: modelId, provider: provider, images: images);
  }

  void _sendPromptRaw(
    String text, {
    String? modelId,
    String? provider,
    List<Map<String, dynamic>>? images,
  }) {
    _client?.sendBrokerCommand(state.instanceId, {
      'type': 'prompt',
      'message': text,
      if (modelId != null && modelId.isNotEmpty)
        'desiredModel': {
          'id': modelId,
          if (provider != null && provider.isNotEmpty) 'provider': provider,
        },
      if (images != null && images.isNotEmpty) 'images': images,
    });
  }

  void _echoUser(String text, {List<Map<String, dynamic>>? images}) {
    final blocks = <ChatBlock>[if (text.isNotEmpty) TextBlock(text)];
    if (images != null) {
      for (final img in images) {
        blocks.add(ImageBlock(
          data: img['data'] as String? ?? '',
          mimeType: img['mimeType'] as String? ?? 'image/png',
        ));
      }
    }
    state = state.copyWith(entries: [
      ...state.entries,
      ChatMsgEntry(message: ChatMessage(role: 'user', blocks: blocks)),
    ]);
  }

  /// 停止当前回合（流式保留已输出内容）。
  void abort() {
    _client?.sendBrokerCommand(state.instanceId, {'type': 'abort'});
  }

  /// 离开会话时通知后端移除订阅者（切回列表态）。
  void deactivate() {
    if (state.instanceId.isNotEmpty) {
      _client?.sendBrokerCommand(state.instanceId, {'type': 'deactivate_session'});
    }
  }

  /// 拉取斜杠命令（get_commands，结果缓存到 state.slashCommands）。
  void getCommands() {
    _client?.sendBrokerCommand(state.instanceId, {
      'type': 'get_commands',
      'id': 'slash-cmds',
    });
  }

  /// 应答扩展 UI 请求（pi 依赖回执解除阻塞，含 cancelled）。
  void sendExtResponse(String id, {dynamic value, bool? confirmed, bool? cancelled}) {
    _client?.sendBrokerCommand(state.instanceId, {
      'type': 'extension_ui_response',
      'id': id,
      'value': ?value,
      'confirmed': ?confirmed,
      'cancelled': ?cancelled,
    });
    // 本地标记卡片已应答。
    state = state.copyWith(entries: [
      for (final e in state.entries)
        if (e is ChatExtEntry && e.ui.id == id)
          ChatExtEntry(ui: e.ui.copyWith(
            answered: true,
            result: value ?? (confirmed == true ? true : (cancelled == true ? false : null)),
          ))
        else
          e,
    ]);
  }

  /// 清空消息流（切会话/离开时）。
  void reset() {
    state = const ChatSessionState();
  }

  Future<void> disconnect() async {
    await _sub?.cancel();
    _sub = null;
    await _client?.disconnect();
    _client = null;
  }
}
