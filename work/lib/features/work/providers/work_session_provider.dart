/// Work 会话 Notifier：连接 WS（gateway /ws），订阅事件流，经 reducer 驱动状态。
library;

import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../core/network/models/models.dart';
import '../../../shared/ws_events.dart';
import 'data_sources.dart';
import 'work_session.dart';

final workSessionProvider =
    NotifierProvider<WorkSessionNotifier, WorkSessionState>(WorkSessionNotifier.new);

class WorkSessionNotifier extends Notifier<WorkSessionState> {
  StreamSubscription<WsEvent>? _sub;
  String? _workspaceId;

  @override
  WorkSessionState build() {
    // Provider 销毁时释放连接与订阅。
    ref.onDispose(() {
      _sub?.cancel();
      _sub = null;
    });
    return const WorkSessionState();
  }

  bool get isRunning => _sub != null;

  /// 连接 WS 并开始消费 gateway 事件流。
  Future<void> startSession(String workspaceId) async {
    if (isRunning && _workspaceId == workspaceId) return;
    await _stop();
    _workspaceId = workspaceId;
    state = const WorkSessionState();
    final ws = ref.read(wsClientProvider);
    _sub = ws.events.listen(_onEvent);
    await ws.connect(workspaceId: workspaceId);
    state = state.copyWith(connected: true);
  }

  /// 发送 prompt（会话建立后；契约 §3.2 broker_command 透传）。
  void sendPrompt(String text) {
    final ws = ref.read(wsClientProvider);
    final iid = state.instanceId;
    if (!state.connected || iid.isEmpty) return;
    ws.sendBrokerCommand(iid, {'type': 'prompt', 'message': text});
    // 本地即时回显用户消息（对齐 Vue chat；reducer 只处理 assistant 流式，
    // 不依赖服务端回推 user message_start，避免双写重复）。
    state = state.copyWith(
      entries: [
        ...state.entries,
        MessageEntry(message: PiMessage(role: PiMessageRole.user, content: text)),
      ],
    );
  }

  /// 批准/拒绝写阻断（ask 模式）。data 回填 write_block 事件携带的
  /// workspaceId + path（契约定稿 §8.5：approve_write 需要这两个字段）。
  Future<void> approveWrite({required bool allow}) async {
    final ws = ref.read(wsClientProvider);
    String? requestId;
    String? workspaceId;
    String? path;
    for (final entry in state.entries.reversed) {
      if (entry is WriteBlockEntry) {
        requestId = entry.block.requestId;
        workspaceId = entry.block.workspaceId;
        path = entry.block.path;
        break;
      }
    }
    ws.sendGatewayCommand(requestId ?? 'wb_unknown', 'approve_write', {
      'allow': allow,
      'remember': true,
      'workspaceId': workspaceId ?? '',
      'path': path ?? '',
    });
  }

  Future<void> stop() => _stop();

  Future<void> _stop() async {
    await _sub?.cancel();
    _sub = null;
    final ws = ref.read(wsClientProvider);
    if (ws.isConnected) await ws.disconnect();
  }

  void _onEvent(WsEvent evt) {
    state = reduceWorkSession(state, evt);
  }
}
