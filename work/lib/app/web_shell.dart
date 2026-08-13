/// Web 端壳：仅 work 模块（无 tab），URL 同步由 go_router 处理。
library;

import 'package:flutter/material.dart';

import '../features/work/workspace_list/workspace_list_page.dart';

class WebShell extends StatelessWidget {
  const WebShell({super.key});

  @override
  Widget build(BuildContext context) => const WorkspaceListPage();
}
