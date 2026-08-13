/// 工作空间详情页：响应式三区。
/// 宽屏（≥900）三栏（文件 | 消息 | 产物）；窄屏单列 TabBar（文件/消息/产物）。
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../core/network/models/models.dart';
import '../providers/workspace_detail_provider.dart';
import '../widgets/workspace_mode_badge.dart';
import 'artifacts_panel.dart';
import 'file_tree_panel.dart';
import 'message_panel.dart';

class WorkspaceDetailPage extends ConsumerWidget {
  const WorkspaceDetailPage({super.key, required this.workspaceId});

  final String workspaceId;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final detail = ref.watch(workspaceDetailProvider(workspaceId));

    return Scaffold(
      appBar: AppBar(
        title: detail.maybeWhen(
          data: (d) => Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Flexible(
                child: Text(d.workspace.name, overflow: TextOverflow.ellipsis),
              ),
              const SizedBox(width: 8),
              // 点击模式徽标切换写边界策略（ask | allow | deny）。
              PopupMenuButton<WorkspaceMode>(
                tooltip: '写边界策略',
                onSelected: (m) =>
                    ref.read(workspaceDetailProvider(workspaceId).notifier).setMode(m),
                itemBuilder: (context) => [
                  // 三种模式只约束工作空间外的写入，内部路径始终放行。
                  const PopupMenuItem(
                    enabled: false,
                    child: Text(
                      '仅约束工作空间以外的写入，\n内部路径始终放行',
                      style: TextStyle(fontSize: 12),
                    ),
                  ),
                  const PopupMenuDivider(),
                  for (final m in WorkspaceMode.values)
                    PopupMenuItem(value: m, child: Text(_modeLabel(m))),
                ],
                child: Padding(
                  padding: const EdgeInsets.symmetric(vertical: 4),
                  child: WorkspaceModeBadge(mode: d.workspace.mode),
                ),
              ),
            ],
          ),
          orElse: () => const Text('工作空间'),
        ),
      ),
      body: LayoutBuilder(
        builder: (context, constraints) {
          if (constraints.maxWidth >= 900) {
            return Row(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                SizedBox(
                  width: 260,
                  child: FileTreePanel(workspaceId: workspaceId),
                ),
                const VerticalDivider(width: 1),
                Expanded(
                  flex: 3,
                  child: MessagePanel(workspaceId: workspaceId),
                ),
                const VerticalDivider(width: 1),
                SizedBox(
                  width: 320,
                  child: ArtifactsPanel(workspaceId: workspaceId),
                ),
              ],
            );
          }
          return DefaultTabController(
            length: 3,
            child: Column(
              children: [
                const TabBar(
                  tabs: [
                    Tab(text: '文件'),
                    Tab(text: '消息'),
                    Tab(text: '产物'),
                  ],
                ),
                Expanded(
                  child: TabBarView(
                    children: [
                      FileTreePanel(workspaceId: workspaceId),
                      MessagePanel(workspaceId: workspaceId),
                      ArtifactsPanel(workspaceId: workspaceId),
                    ],
                  ),
                ),
              ],
            ),
          );
        },
      ),
    );
  }
}

/// 模式菜单文案（ask=每次询问 / allow=放行 / deny=拒绝）。
String _modeLabel(WorkspaceMode mode) => switch (mode) {
      WorkspaceMode.ask => '每次询问（ask）',
      WorkspaceMode.allow => '放行（allow）',
      WorkspaceMode.deny => '拒绝（deny）',
    };
