/// 无可用服务器的引导视图（首次使用 App 必经）。
///
/// chat 与 work 共用，保证两页"未连接"提示一致（见 0.3.x 交互定案）。
library;

import 'package:flutter/material.dart';

import 'module_switcher.dart';

class NoServerView extends StatelessWidget {
  const NoServerView({
    super.key,
    required this.title,
    required this.onOpenConnection,
    this.currentModule = 0,
    this.onSwitchModule,
  });

  /// AppBar 标题（模块切换器不可用时显示）。
  final String title;

  /// 跳转连接页（添加局域网服务端）。
  final VoidCallback onOpenConnection;

  /// 当前模块（0 = 聊天，1 = 工作空间），传给模块切换器。
  final int currentModule;
  final ValueChanged<int>? onSwitchModule;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Scaffold(
      appBar: AppBar(
        title: onSwitchModule != null
            ? ModuleSwitcher(current: currentModule, onSwitch: onSwitchModule!)
            : Text(title),
      ),
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
