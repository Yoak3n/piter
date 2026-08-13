/// 连接服务器设置按钮（聊天 / 工作空间共用入口）。
library;

import 'package:flutter/material.dart';

import '../features/connection/connection_page.dart';

class ServerSettingsButton extends StatelessWidget {
  const ServerSettingsButton({super.key});

  @override
  Widget build(BuildContext context) {
    return IconButton(
      tooltip: '服务器',
      icon: const Icon(Icons.cast),
      onPressed: () => Navigator.of(context).push(
        MaterialPageRoute<void>(builder: (_) => const ConnectionPage()),
      ),
    );
  }
}
