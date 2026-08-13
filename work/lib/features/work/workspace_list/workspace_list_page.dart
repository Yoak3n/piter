/// 工作空间列表页：卡片（名称/文件数/大小/mode 徽标）+ 新建。
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../../core/network/models/models.dart';
import '../../../app/module_switcher.dart';
import '../../../app/server_settings_button.dart';
import '../../connection/connection_page.dart';
import '../../connection/providers/capability_provider.dart';
import '../providers/workspaces_provider.dart';
import '../widgets/workspace_mode_badge.dart';
import 'workspace_create_dialog.dart';

class WorkspaceListPage extends ConsumerWidget {
  const WorkspaceListPage({super.key, this.currentModule = 1, this.onSwitchModule});

  /// 当前模块（1 = 工作空间），传给 AppBar title 的模块切换器。
  final int currentModule;
  final ValueChanged<int>? onSwitchModule;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final workspaces = ref.watch(workspacesProvider);
    final capability = ref.watch(serverCapabilityProvider);

    return Scaffold(
      appBar: AppBar(
        title: onSwitchModule != null
            ? ModuleSwitcher(current: currentModule, onSwitch: onSwitchModule!)
            : const Text('工作空间'),
        actions: [
          IconButton(
            tooltip: '新建',
            icon: const Icon(Icons.add),
            onPressed: () => showCreateWorkspaceDialog(context),
          ),
          const ServerSettingsButton(),
        ],
      ),
      body: capability.when(
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (e, _) => _ErrorView(
          error: e,
          onRetry: () => ref.invalidate(serverCapabilityProvider),
          onOpenConnection: () => Navigator.of(context).push(
            MaterialPageRoute<void>(builder: (_) => const ConnectionPage()),
          ),
        ),
        data: (cap) {
          // 优雅降级：真实服务端无 work 模块时提示，聊天不受影响。
          if (!cap.workSupported) {
            return _WorkUnavailableView(
              version: cap.health?.version,
              error: cap.error,
              onOpenConnection: () => Navigator.of(context).push(
                MaterialPageRoute<void>(builder: (_) => const ConnectionPage()),
              ),
            );
          }
          return workspaces.when(
            loading: () => const Center(child: CircularProgressIndicator()),
            error: (e, _) => _ErrorView(error: e, onRetry: () => ref.invalidate(workspacesProvider)),
            data: (list) => list.isEmpty
                ? _EmptyView(onCreate: () => showCreateWorkspaceDialog(context))
                : RefreshIndicator(
                    onRefresh: () => ref.refresh(workspacesProvider.future),
                    child: ListView.builder(
                      padding: const EdgeInsets.all(12),
                      itemCount: list.length,
                      itemBuilder: (context, i) => _WorkspaceCard(workspace: list[i]),
                    ),
                  ),
          );
        },
      ),
    );
  }
}

/// 服务端不支持 work 模块的降级视图。
class _WorkUnavailableView extends StatelessWidget {
  const _WorkUnavailableView({
    required this.version,
    required this.error,
    required this.onOpenConnection,
  });

  final String? version;
  final String? error;
  final VoidCallback onOpenConnection;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Container(
              width: 64,
              height: 64,
              decoration: BoxDecoration(
                color: scheme.errorContainer,
                shape: BoxShape.circle,
              ),
              child: Icon(Icons.extension_off_outlined, size: 32, color: scheme.onErrorContainer),
            ),
            const SizedBox(height: 16),
            Text(
              version != null ? '当前服务端 piter $version 不支持 work 模块' : '当前服务端不支持 work 模块',
              style: Theme.of(context).textTheme.titleMedium,
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: 8),
            Text(
              error ?? '请升级服务端到支持 work 的版本后重试；聊天功能不受影响。',
              style: Theme.of(context).textTheme.bodySmall?.copyWith(color: scheme.outline),
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: 16),
            OutlinedButton.icon(
              onPressed: onOpenConnection,
              icon: const Icon(Icons.cast),
              label: const Text('服务器设置'),
            ),
          ],
        ),
      ),
    );
  }
}

class _WorkspaceCard extends StatelessWidget {
  const _WorkspaceCard({required this.workspace});

  final Workspace workspace;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Card(
      margin: const EdgeInsets.only(bottom: 10),
      child: InkWell(
        borderRadius: BorderRadius.circular(12),
        onTap: () => context.push('/workspaces/${workspace.id}'),
        child: Padding(
          padding: const EdgeInsets.all(14),
          child: Row(
            children: [
              Container(
                width: 40,
                height: 40,
                decoration: BoxDecoration(
                  color: scheme.primaryContainer,
                  borderRadius: BorderRadius.circular(10),
                ),
                child: Icon(Icons.folder_outlined, color: scheme.onPrimaryContainer),
              ),
              const SizedBox(width: 12),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      workspace.name,
                      style: Theme.of(context).textTheme.titleSmall,
                      overflow: TextOverflow.ellipsis,
                    ),
                    const SizedBox(height: 4),
                    Text(
                      '${workspace.fileCount} 个文件 · ${_formatSize(workspace.sizeBytes)} · ${_formatTime(workspace.updatedAt)}',
                      style: Theme.of(context)
                          .textTheme
                          .bodySmall
                          ?.copyWith(color: scheme.outline),
                    ),
                  ],
                ),
              ),
              const SizedBox(width: 8),
              WorkspaceModeBadge(mode: workspace.mode),
              const SizedBox(width: 4),
              Icon(Icons.chevron_right, color: scheme.outline),
            ],
          ),
        ),
      ),
    );
  }
}

class _EmptyView extends StatelessWidget {
  const _EmptyView({required this.onCreate});

  final VoidCallback onCreate;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Center(
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          Icon(Icons.folder_open_outlined, size: 56, color: scheme.outline),
          const SizedBox(height: 16),
          Text('还没有工作空间', style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 8),
          FilledButton.icon(
            onPressed: onCreate,
            icon: const Icon(Icons.add),
            label: const Text('新建工作空间'),
          ),
        ],
      ),
    );
  }
}

class _ErrorView extends StatelessWidget {
  const _ErrorView({
    required this.error,
    required this.onRetry,
    this.onOpenConnection,
  });

  final Object error;
  final VoidCallback onRetry;

  /// 服务器设置入口（连接失败/未配置时引导到连接页）。
  final VoidCallback? onOpenConnection;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          const Icon(Icons.error_outline, size: 40),
          const SizedBox(height: 12),
          Text('加载失败：$error'),
          const SizedBox(height: 12),
          Wrap(
            spacing: 8,
            children: [
              OutlinedButton(onPressed: onRetry, child: const Text('重试')),
              if (onOpenConnection != null)
                OutlinedButton.icon(
                  onPressed: onOpenConnection,
                  icon: const Icon(Icons.cast),
                  label: const Text('服务器设置'),
                ),
            ],
          ),
        ],
      ),
    );
  }
}

String _formatSize(int bytes) {
  if (bytes < 1024) return '$bytes B';
  if (bytes < 1024 * 1024) return '${(bytes / 1024).toStringAsFixed(1)} KB';
  return '${(bytes / (1024 * 1024)).toStringAsFixed(1)} MB';
}

String _formatTime(DateTime t) {
  final now = DateTime.now();
  final diff = now.difference(t);
  if (diff.inMinutes < 1) return '刚刚';
  if (diff.inHours < 1) return '${diff.inMinutes} 分钟前';
  if (diff.inDays < 1) return '${diff.inHours} 小时前';
  return '${t.year}-${t.month.toString().padLeft(2, '0')}-${t.day.toString().padLeft(2, '0')}';
}
