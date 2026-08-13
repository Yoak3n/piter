/// 服务器能力探测状态（health 版本 + work 模块支持判定）。
library;

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../core/config/server_config.dart';
import '../../../core/network/probe.dart';
import 'servers_provider.dart';

export '../../../core/network/probe.dart' show ServerCapability;

/// 当前选中的服务器；无记录返回 null（不臆造默认本机服务器）。
ServerInfo? currentServerOf(Ref ref) {
  final servers = ref.watch(serversProvider).valueOrNull ?? const <ServerInfo>[];
  final currentId = ref.watch(currentServerIdProvider);
  for (final s in servers) {
    if (s.id == currentId) return s;
  }
  return servers.isNotEmpty ? servers.first : null;
}

/// 当前服务器的能力探测：统一走 probeServer（后端未实现 work API 时优雅降级）。
final serverCapabilityProvider =
    AsyncNotifierProvider<ServerCapabilityNotifier, ServerCapability>(
        ServerCapabilityNotifier.new);

class ServerCapabilityNotifier extends AsyncNotifier<ServerCapability> {
  @override
  Future<ServerCapability> build() async {
    final server = currentServerOf(ref);
    if (server == null) {
      // 没有连接服务器就是没有服务器——标记为不可达，不探测任何默认地址。
      return const ServerCapability(reachable: false, error: '未连接服务器');
    }
    return probeServer(server.baseUrl);
  }
}
