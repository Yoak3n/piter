/// chat 模块（0.3.2 原生化）：Flutter 原生会话 UI，替代 WebView 嵌入 Vue chat。
///
/// 无配置服务器时引导到连接页（移动端无法回退到 127.0.0.1，必须显式添加
/// 局域网服务端）；有服务器时展示会话列表页。
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/config/server_config.dart';
import '../../app/no_server_view.dart';
import '../connection/connection_page.dart';
import '../connection/providers/servers_provider.dart';
import 'session_list_page.dart';

class ChatPage extends ConsumerWidget {
  const ChatPage({super.key, this.currentModule = 0, this.onSwitchModule});

  /// 当前模块（0 = 聊天），传给 AppBar title 的模块切换器。
  final int currentModule;
  final ValueChanged<int>? onSwitchModule;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    // 当前选中的服务器（与 work 侧同一逻辑：无则回退列表首个）。
    final servers = ref.watch(serversProvider).valueOrNull ?? const <ServerInfo>[];
    final currentId = ref.watch(currentServerIdProvider);
    ServerInfo? current;
    for (final s in servers) {
      if (s.id == currentId) {
        current = s;
        break;
      }
    }
    current ??= servers.isNotEmpty ? servers.first : null;
    if (current == null) {
      return NoServerView(
        title: '聊天',
        currentModule: currentModule,
        onSwitchModule: onSwitchModule,
        onOpenConnection: () => Navigator.of(context).push(
          MaterialPageRoute<void>(builder: (_) => const ConnectionPage()),
        ),
      );
    }
    return SessionListPage(currentModule: currentModule, onSwitchModule: onSwitchModule);
  }
}

