/// Work 会话 Notifier：连接 WS（gateway /ws），订阅事件流，经 reducer 驱动状态。
library;

import 'dart:async';
import 'dart:convert';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../core/network/models/models.dart';
import '../../../core/platform/storage/storage.dart';
import '../../../shared/ws_events.dart';
import 'data_sources.dart';
import 'work_session.dart';

final workSessionProvider =
    NotifierProvider<WorkSessionNotifier, WorkSessionState>(WorkSessionNotifier.new);

class WorkSessionNotifier extends Notifier<WorkSessionState> {
  StreamSubscription<WsEvent>? _sub;
  String? _workspaceId;
  Timer? _reconnectTimer;

  /// 自动重连退避（秒）：3 → 6 → 12 → 24 → 60，耗尽后停止等待手动重试。
  static const _backoffSecs = [3, 6, 12, 24, 60];
  static const _maxAttempts = 5;
  int _reconnectAttempt = 0;

  @override
  WorkSessionState build() {
    // Provider 销毁时释放连接与订阅。
    ref.onDispose(() {
      _reconnectTimer?.cancel();
      _reconnectTimer = null;
      _sub?.cancel();
      _sub = null;
    });
    return const WorkSessionState();
  }

  bool get isRunning => _sub != null;

  /// 连接 WS 并开始消费 gateway 事件流（主动打开工作空间，清空历史状态）。
  Future<void> startSession(String workspaceId) async {
    if (isRunning && _workspaceId == workspaceId) return;
    await _stop();
    _workspaceId = workspaceId;
    state = const WorkSessionState();
    await _connect();
    // 恢复该工作空间本地持久化的未应答交互卡片（刷新/重启后不丢）。
    await _loadPendingExt();
  }

  /// 建立 WS 连接 + create_ws（断线重连复用，不清空 entries 等状态）。
  Future<void> _connect() async {
    _reconnectTimer?.cancel();
    _reconnectTimer = null;
    final wid = _workspaceId;
    if (wid == null || wid.isEmpty) return;
    final ws = ref.read(wsClientProvider);
    _sub?.cancel();
    _sub = ws.events.listen(_onEvent);
    await ws.connect(workspaceId: wid);
    state = state.copyWith(connected: true, reconnectFailed: false);
  }

  void _onEvent(WsEvent evt) {
    // 连接断开：标记状态并安排自动重连（重连后 create_ws 恢复会话订阅）。
    if (evt is UnknownEvent && evt.type == 'ws_error') {
      if (state.connected) {
        state = state.copyWith(connected: false, reconnectFailed: false);
        _scheduleReconnect();
      }
      return;
    }
    // capabilities 握手到达 = 连接确认，重置退避计数。
    if (evt is CapabilitiesEvent) {
      _reconnectAttempt = 0;
    }
    final isBlockingExt = evt is ExtUiRequestEvent && _isBlockingMethod(evt.method);
    state = reduceWorkSession(state, evt);
    // 阻塞交互请求新增未应答卡片 → 本地持久化（刷新/重启后恢复）。
    if (isBlockingExt) {
      _persistPendingExt();
    }
  }

  static bool _isBlockingMethod(String method) =>
      method != 'notify' &&
      method != 'setStatus' &&
      method != 'setWidget' &&
      method != 'setTitle' &&
      method != 'set_editor_text';

  /// 未应答交互卡片的本地持久化（SharedPreferences，按工作空间隔离）。
  Future<void> _persistPendingExt() async {
    final wid = _workspaceId;
    if (wid == null || wid.isEmpty) return;
    final cards = [
      for (final e in state.entries)
        if (e is WorkExtEntry && !e.ui.answered) e.ui.toJson(),
    ];
    await ref
        .read(storageServiceProvider)
        .write('work.pendingExt.$wid', jsonEncode(cards));
  }

  /// 恢复该工作空间本地持久化的未应答卡片。
  Future<void> _loadPendingExt() async {
    final wid = _workspaceId;
    if (wid == null || wid.isEmpty) return;
    final raw = await ref.read(storageServiceProvider).read('work.pendingExt.$wid');
    if (raw == null || raw.isEmpty) return;
    final cards = (jsonDecode(raw) as List<dynamic>? ?? const [])
        .whereType<Map<String, dynamic>>()
        .map(ChatExtUi.fromJson)
        .where((u) => !u.answered)
        .toList();
    if (cards.isEmpty) return;
    state = state.copyWith(entries: [
      ...state.entries,
      for (final u in cards) WorkExtEntry(ui: u),
    ]);
  }

  /// 指数退避自动重连；达到上限后停止，标记失败等待用户手动重试。
  void _scheduleReconnect() {
    _reconnectAttempt++;
    if (_reconnectAttempt > _maxAttempts) {
      state = state.copyWith(connected: false, reconnectFailed: true);
      return;
    }
    final delay = _backoffSecs[(_reconnectAttempt - 1).clamp(0, _backoffSecs.length - 1)];
    _reconnectTimer?.cancel();
    _reconnectTimer = Timer(Duration(seconds: delay), _connect);
  }

  /// 用户手动重试连接（连接失败态调用）。
  void retryConnect() {
    _reconnectAttempt = 0;
    state = state.copyWith(connected: false, reconnectFailed: false);
    _connect();
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

  /// 应答交互插件扩展 UI 请求（pi 依赖回执解除阻塞，含 cancelled）。
  void sendExtResponse(String id, {dynamic value, bool? confirmed, bool? cancelled}) {
    final ws = ref.read(wsClientProvider);
    if (state.instanceId.isEmpty) return;
    ws.sendBrokerCommand(state.instanceId, {
      'type': 'extension_ui_response',
      'id': id,
      'value': ?value,
      'confirmed': ?confirmed,
      'cancelled': ?cancelled,
    });
    // 本地标记卡片已应答（保留为只读历史）。
    state = state.copyWith(entries: [
      for (final e in state.entries)
        if (e is WorkExtEntry && e.ui.id == id)
          WorkExtEntry(ui: e.ui.copyWith(
            answered: true,
            result: value ?? (confirmed == true ? true : (cancelled == true ? false : null)),
          ))
        else
          e,
    ]);
    // 应答后重写本地持久化（已应答卡片不再保留）。
    _persistPendingExt();
  }

  Future<void> stop() => _stop();

  Future<void> _stop() async {
    _reconnectTimer?.cancel();
    _reconnectTimer = null;
    await _sub?.cancel();
    _sub = null;
    final ws = ref.read(wsClientProvider);
    if (ws.isConnected) await ws.disconnect();
  }
}
