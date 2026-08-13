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

  final StreamController<WsEvent> _controller = StreamController<WsEvent>.broadcast(sync: true);
  WebSocketChannel? _channel;
  StreamSubscription<dynamic>? _sub;
  bool _connected = false;

  @override
  Stream<WsEvent> get events => _controller.stream;

  @override
  bool get isConnected => _connected;

  @override
  Future<void> connect({String? workspaceId}) async {
    if (_connected) return;
    _connected = true;
    _channel = WebSocketChannel.connect(Uri.parse(wsUrl));
    _sub = _channel!.stream.listen(
      _onRaw,
      onError: (Object _) => _controller.add(UnknownEvent(type: 'ws_error')),
      onDone: () {
        _connected = false;
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

  void _onRaw(dynamic raw) {
    if (raw is! String) return;
    try {
      final decoded = jsonDecode(raw);
      if (decoded is Map<String, dynamic>) {
        _controller.add(parseWsEvent(decoded));
      }
    } catch (_) {
      // 忽略无法解析的帧。
    }
  }

  @override
  void sendGatewayCommand(String requestId, String command, Map<String, dynamic> data) {
    _channel?.sink.add(jsonEncode({
      'type': 'gateway_command',
      'requestId': requestId,
      'command': command,
      'data': data,
    }));
  }

  @override
  void sendBrokerCommand(String instanceId, Map<String, dynamic> payload) {
    _channel?.sink.add(jsonEncode({
      'type': 'broker_command',
      'instanceId': instanceId,
      'payload': payload,
    }));
  }

  @override
  Future<void> disconnect() async {
    _connected = false;
    await _sub?.cancel();
    _sub = null;
    await _channel?.sink.close();
    _channel = null;
  }
}
