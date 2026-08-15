/// 数据源注入：直接注入真实实现（后端 work API 已就绪）。
///
/// - REST：HttpApiClient（dio）
/// - WS：HttpWsClient（web_socket_channel，gateway /work-ws——path 定前端，
///   gateway 连接注册表据此把本连接识别为 work 客户端）
/// - work 能力探测见 connection/providers/capability_provider.dart
library;

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../core/config/server_config.dart';
import '../../../core/network/api_client.dart';
import '../../../core/network/http_api_client.dart';
import '../../../core/network/ws_client.dart';
import '../../../core/network/ws_client_impl.dart';
import '../../connection/providers/capability_provider.dart';

/// REST 数据源：当前服务器（无记录 → 抛错，引导去连接页，不连默认地址）。
final apiClientProvider = Provider<ApiClient>((ref) {
  final server = ref.watch(currentServerProvider);
  if (server == null) {
    throw StateError('未连接服务器，请先在连接页添加 Piter 服务端');
  }
  return HttpApiClient(baseUrl: server.baseUrl);
});

/// work 专用 WS 端点：`ws://host:port/work-ws`（由通用 wsUrl 派生，
/// 与 chat 的 `/ws` 区分——gateway 按连接 path 判定客户端类型）。
String workWsUrl(ServerInfo server) =>
    server.wsUrl.replaceFirst(RegExp(r'/ws$'), '/work-ws');

/// chat 专用 WS 端点：`ws://host:port/chat-ws`（chat 前端分类）。
String chatWsUrl(ServerInfo server) =>
    server.wsUrl.replaceFirst(RegExp(r'/ws$'), '/chat-ws');

/// WS 数据源：当前服务器（无记录 → 抛错，引导去连接页，不连默认地址）。
final wsClientProvider = Provider<WsClient>((ref) {
  final server = ref.watch(currentServerProvider);
  if (server == null) {
    throw StateError('未连接服务器，请先在连接页添加 Piter 服务端');
  }
  return HttpWsClient(wsUrl: workWsUrl(server));
});

/// chat 专用 WS 数据源（原生 chat，/chat-ws）。
final chatWsClientProvider = Provider<WsClient>((ref) {
  final server = ref.watch(currentServerProvider);
  if (server == null) {
    throw StateError('未连接服务器，请先在连接页添加 Piter 服务端');
  }
  return HttpWsClient(wsUrl: chatWsUrl(server));
});
