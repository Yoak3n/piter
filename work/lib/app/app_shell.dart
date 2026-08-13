/// App 端壳：顶部模块切换（聊天 / 工作空间），替代底部双 tab。
library;

import 'package:flutter/material.dart';

import '../features/chat/chat_page.dart';
import '../features/work/workspace_list/workspace_list_page.dart';

class AppShell extends StatefulWidget {
  const AppShell({super.key});

  @override
  State<AppShell> createState() => _AppShellState();
}

class _AppShellState extends State<AppShell> {
  // 默认落在 work 模块（0.3.0 主目标）。
  int _index = 1;

  void _switchTo(int i) => setState(() => _index = i);

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: IndexedStack(
        index: _index,
        children: [
          ChatPage(currentModule: 0, onSwitchModule: _switchTo),
          WorkspaceListPage(currentModule: 1, onSwitchModule: _switchTo),
        ],
      ),
    );
  }
}
