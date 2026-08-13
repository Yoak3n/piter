/// 多服务端管理状态（服务器列表 + 当前选中），持久化于 ServerConfig。
library;

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../core/config/server_config.dart';

/// 服务器列表（发现 + 手动添加 + 删除）。
final serversProvider =
    AsyncNotifierProvider<ServersNotifier, List<ServerInfo>>(ServersNotifier.new);

/// 当前选中服务器 id。
final currentServerIdProvider =
    NotifierProvider<CurrentServerNotifier, String>(CurrentServerNotifier.new);

class ServersNotifier extends AsyncNotifier<List<ServerInfo>> {
  @override
  Future<List<ServerInfo>> build() async {
    final servers = await ServerConfig.loadServers();
    // 回填当前服务器（保证 currentServerIdProvider 始终可用）。
    ref.read(currentServerIdProvider.notifier).load(servers);
    return servers;
  }

  /// 手动添加服务器（baseUrl 形如 http://192.168.1.5:31421）。
  Future<ServerInfo> addServer({required String name, required String baseUrl}) async {
    final id = 'srv_${DateTime.now().millisecondsSinceEpoch.toRadixString(36)}';
    final wsBase = baseUrl.replaceFirst(RegExp(r'^http'), 'ws');
    final server = ServerInfo(id: id, name: name, baseUrl: baseUrl, wsUrl: '$wsBase/ws');
    final next = [...?state.value, server];
    await ServerConfig.saveServers(next);
    state = AsyncData(next);
    // 首个服务器自动设为当前。
    final current = ref.read(currentServerIdProvider);
    if (current.isEmpty) {
      ref.read(currentServerIdProvider.notifier).set(server.id);
    }
    return server;
  }

  Future<void> removeServer(String id) async {
    final next = [...?state.value]..removeWhere((s) => s.id == id);
    await ServerConfig.saveServers(next);
    state = AsyncData(next);
    final current = ref.read(currentServerIdProvider);
    if (current == id) {
      final fallback = next.isNotEmpty ? next.first.id : '';
      ref.read(currentServerIdProvider.notifier).set(fallback);
    }
  }
}

class CurrentServerNotifier extends Notifier<String> {
  @override
  String build() => '';

  Future<void> load(List<ServerInfo> servers) async {
    state = await ServerConfig.loadCurrentServerId(servers);
  }

  Future<void> set(String id) async {
    state = id;
    await ServerConfig.saveCurrentServerId(id);
  }
}
