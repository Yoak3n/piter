/// chat 模块（0.3.2 原生化）：Flutter 原生会话 UI，替代 WebView 嵌入 Vue chat。
///
/// 无配置服务器时引导到连接页（移动端无法回退到 127.0.0.1，必须显式添加
/// 局域网服务端）；有服务器时展示会话列表页。
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/config/server_config.dart';
import '../connection/connection_page.dart';
import '../connection/providers/servers_provider.dart';
import 'session_list_page.dart';

class ChatPage extends ConsumerWidget {
  const ChatPage({super.key});

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
      return _NoServerView(
        onOpenConnection: () => Navigator.of(context).push(
          MaterialPageRoute<void>(builder: (_) => const ConnectionPage()),
        ),
      );
    }
    return const SessionListPage();
  }
}

/// 无可用服务器的引导视图（首次使用 App 必经）。
class _NoServerView extends StatelessWidget {
  const _NoServerView({required this.onOpenConnection});

  final VoidCallback onOpenConnection;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Scaffold(
      appBar: AppBar(title: const Text('聊天')),
      body: Center(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Icon(Icons.cast_connected, size: 56, color: scheme.outline),
              const SizedBox(height: 16),
              Text('尚未连接服务器', style: Theme.of(context).textTheme.titleMedium),
              const SizedBox(height: 8),
              Text(
                '请先在「连接页」添加局域网内的 Piter 服务端（手动输入 IP:端口），聊天与工作空间将共用同一连接。',
                style: Theme.of(context).textTheme.bodySmall?.copyWith(color: scheme.outline),
                textAlign: TextAlign.center,
              ),
              const SizedBox(height: 16),
              FilledButton.icon(
                onPressed: onOpenConnection,
                icon: const Icon(Icons.settings_ethernet),
                label: const Text('服务器设置'),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
