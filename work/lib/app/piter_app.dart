/// PiterApp：MaterialApp + 主题（seed #6a7a8a）+ go_router 路由。
library;

import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';

import '../core/theme/app_theme.dart';
import '../features/work/workspace_detail/workspace_detail_page.dart';
import 'app_shell.dart';
import 'web_shell.dart';

class PiterApp extends StatelessWidget {
  const PiterApp({super.key, required this.isWeb});

  /// true=Web 端（仅 work，无 tab）；false=App 端（底部双 tab）。
  final bool isWeb;

  @override
  Widget build(BuildContext context) {
    return MaterialApp.router(
      title: 'Piter Work',
      debugShowCheckedModeBanner: false,
      theme: AppTheme.light(),
      darkTheme: AppTheme.dark(),
      themeMode: ThemeMode.system,
      routerConfig: GoRouter(
        initialLocation: '/',
        routes: [
          GoRoute(
            path: '/',
            builder: (context, state) => isWeb ? const WebShell() : const AppShell(),
          ),
          // Web 目标经 gateway 分发到 /work：pathname 是 /work，go_router 按
          // 完整 path 匹配，需显式登记（App 端不会命中；/work/ 由 gateway
          // 重定向到 /work，go_router 路由不允许尾斜杠）。
          GoRoute(
            path: '/work',
            builder: (context, state) => isWeb ? const WebShell() : const AppShell(),
          ),
          GoRoute(
            path: '/workspaces/:id',
            builder: (context, state) => WorkspaceDetailPage(
              workspaceId: state.pathParameters['id']!,
            ),
          ),
        ],
      ),
    );
  }
}
