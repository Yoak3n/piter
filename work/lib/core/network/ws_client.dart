/// WsClient 抽象（对应 mock-contract §3 WS 契约）。
/// 真实实现为 HttpWsClient（web_socket_channel），经 data_sources.dart 注入。
library;

import '../../shared/ws_events.dart';

/// WS 客户端接口：事件流 + 客户端命令。
abstract class WsClient {
  /// 解析后的 work 事件流（capabilities → sessions_list → 生命周期/产物/写阻断…）。
  Stream<WsEvent> get events;

  bool get isConnected;

  /// 建立连接；work 会话携带 workspaceId 以便推送对应工作空间的事件。
  Future<void> connect({String? workspaceId});

  Future<void> disconnect();

  /// 发送 gateway_command（如 approve_write）。
  void sendGatewayCommand(String requestId, String command, Map<String, dynamic> data);

  /// 发送 broker_command 透传给 pi 实例（如 prompt / steer）。
  void sendBrokerCommand(String instanceId, Map<String, dynamic> payload);
}
