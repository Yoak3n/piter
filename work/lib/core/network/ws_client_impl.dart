/// HttpWsClient：web_socket_channel 实现的真实 WsClient（连接 gateway /ws）。
/// Web 端自动适配浏览器原生 WebSocket；App 端走 IOWebSocketChannel。
library;

import 'dart:async';
import 'dart:convert';

import 'package:web_socket_channel/web_socket_channel.dart';

import '../../shared/ws_events.dart';
import 'ws_client.dart';

class HttpWsClient implements WsClient {
  HttpWsClient({required this.wsUrl});

  final String wsUrl;

  /// 断线自动重连上限（对齐 web 端 MAX_RECONNECT_ATTEMPTS=3）。
  static const int _maxReconnectAttempts = 3;

  final StreamController<WsEvent> _controller = StreamController<WsEvent>.broadcast(sync: true);
  WebSocketChannel? _channel;
  StreamSubscription<dynamic>? _sub;
  bool _connected = false;
  Timer? _reconnectTimer;
  int _reconnectAttempts = 0;

  /// 上次 connect 携带的 workspaceId（重连时复用，工作空间会话恢复）。
  String? _pendingWorkspaceId;

  @override
  Stream<WsEvent> get events => _controller.stream;

  @override
  bool get isConnected => _connected;

  @override
  Future<void> connect({String? workspaceId}) async {
    if (_connected) return;
    _pendingWorkspaceId = workspaceId;
    // 手动重连时取消未决的自动重连定时器。
    _reconnectTimer?.cancel();
    _reconnectTimer = null;
    // 旧连接若未完全关闭（onError 场景），先关掉避免资源泄漏。
    if (_channel != null) {
      try {
        await _channel!.sink.close();
      } catch (_) {}
    }
    _channel = null;
    _connected = true;
    _channel = WebSocketChannel.connect(Uri.parse(wsUrl));
    _sub = _channel!.stream.listen(
      _onRaw,
      onError: (Object _) {
        _connected = false;
        _controller.add(UnknownEvent(type: 'ws_error'));
        _scheduleReconnect();
      },
      onDone: () {
        _connected = false;
        _scheduleReconnect();
      },
    );
    // 等待服务端 capabilities 握手后再建会话（gateway 收包顺序保证）。
    if (workspaceId != null && workspaceId.isNotEmpty) {
      await Future<void>.delayed(const Duration(milliseconds: 300));
      sendGatewayCommand('create_ws', 'create_workspace_session', {
        'workspaceId': workspaceId,
      });
    }
  }

  /// 断线后按 2s/4s/6s 退避重连（上限 3 次）；成功后由上层收到新的
  /// capabilities 重新 ack 会话（对齐 web 端 scheduleReconnect）。
  void _scheduleReconnect() {
    if (_reconnectTimer != null) return;
    if (_reconnectAttempts >= _maxReconnectAttempts) return;
    _reconnectAttempts++;
    final delay = Duration(seconds: _reconnectAttempts * 2);
    _reconnectTimer = Timer(delay, () {
      _reconnectTimer = null;
      _connected = false;
      connect(workspaceId: _pendingWorkspaceId);
    });
  }

  void _onRaw(dynamic raw) {
    if (raw is! String) return;
    try {
      final decoded = jsonDecode(raw);
      if (decoded is Map<String, dynamic>) {
        // 能解出帧说明连接存活：复位重连计数（下次断线从头退避）。
        _reconnectAttempts = 0;
        _controller.add(parseWsEvent(decoded));
      }
    } catch (_) {
      // 忽略无法解析的帧。
    }
  }

  @override
  void sendGatewayCommand(String requestId, String command, Map<String, dynamic> data) {
    try {
      _channel?.sink.add(jsonEncode({
        'type': 'gateway_command',
        'requestId': requestId,
        'command': command,
        'data': data,
      }));
    } catch (_) {
      // 连接已断开（重连中）：丢弃该帧，重连后由上层重发。
    }
  }

  @override
  void sendBrokerCommand(String instanceId, Map<String, dynamic> payload) {
    try {
      _channel?.sink.add(jsonEncode({
        'type': 'broker_command',
        'instanceId': instanceId,
        'payload': payload,
      }));
    } catch (_) {
      // 连接已断开（重连中）：丢弃该帧，重连后由上层重发。
    }
  }

  @override
  Future<void> disconnect() async {
    _reconnectTimer?.cancel();
    _reconnectTimer = null;
    _connected = false;
    await _sub?.cancel();
    _sub = null;
    await _channel?.sink.close();
    _channel = null;
  }
}
