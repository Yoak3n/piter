/// 详情页·产物面板：实时产物（turn_artifacts）+ 历史产物（按 turn 分组）。
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../core/network/models/models.dart';
import '../../../core/platform/browser_io.dart';
import '../providers/artifacts_provider.dart';
import '../providers/data_sources.dart';
import '../providers/work_session_provider.dart';
import '../providers/workspace_detail_provider.dart';
import '../widgets/artifact_card.dart';

class ArtifactsPanel extends ConsumerWidget {
  const ArtifactsPanel({super.key, required this.workspaceId});

  final String workspaceId;

  Future<void> _download(BuildContext context, WidgetRef ref, String path) async {
    final messenger = ScaffoldMessenger.of(context);
    try {
      final api = ref.read(apiClientProvider);
      final bytes = await api.downloadFile(workspaceId, path);
      saveBytes(bytes, path.split('/').last);
      messenger.showSnackBar(SnackBar(content: Text('已开始下载 ${path.split('/').last}')));
    } catch (e) {
      messenger.showSnackBar(SnackBar(content: Text('下载失败：$e')));
    }
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final scheme = Theme.of(context).colorScheme;
    final session = ref.watch(workSessionProvider);
    final turns = ref.watch(artifactsProvider(workspaceId));

    final liveItems = [
      for (final item in session.liveArtifacts)
        _toArtifact(item, workspaceId),
    ];

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 12, 16, 4),
          child: Text('产物', style: Theme.of(context).textTheme.labelLarge),
        ),
        const Divider(height: 1),
        Expanded(
          child: ListView(
            padding: const EdgeInsets.all(12),
            children: [
              if (liveItems.isNotEmpty) ...[
                const _SectionTitle('本轮产物'),
                for (final item in liveItems)
                  ArtifactCard(
                    artifact: item,
                    onDownload: () => _download(context, ref, item.path),
                  ),
                const SizedBox(height: 8),
              ],
              const _SectionTitle('历史'),
              turns.when(
                loading: () => const Padding(
                  padding: EdgeInsets.all(16),
                  child: Center(child: CircularProgressIndicator()),
                ),
                error: (e, _) => Padding(
                  padding: const EdgeInsets.all(16),
                  child: Text('加载失败：$e', style: TextStyle(color: scheme.error)),
                ),
                data: (list) {
                  if (list.isEmpty) {
                    return Padding(
                      padding: const EdgeInsets.all(16),
                      child: Text(
                        '暂无产物',
                        style: Theme.of(context).textTheme.bodySmall,
                      ),
                    );
                  }
                  return Column(
                    children: [
                      for (final turn in list) ...[
                        Padding(
                          padding: const EdgeInsets.symmetric(vertical: 6),
                          child: Row(
                            children: [
                              Icon(Icons.adjust, size: 12, color: scheme.primary),
                              const SizedBox(width: 6),
                              Text(
                                '${_formatTime(turn.createdAt)} · ${_formatCount(turn.items.length)} 项',
                                style: Theme.of(context).textTheme.labelMedium,
                              ),
                            ],
                          ),
                        ),
                        for (final item in turn.items)
                          // 历史仅记录变更元数据，不保留文件内容快照，下载为磁盘最新版，
                          // 为避免误导不在历史条目提供下载按钮。
                          ArtifactCard(
                            artifact: item,
                            onToggleDeliverable: (deliverable) => ref
                                .read(workspaceDetailProvider(workspaceId).notifier)
                                .markDeliverable(item.path, deliverable),
                          ),
                      ],
                    ],
                  );
                },
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class _SectionTitle extends StatelessWidget {
  const _SectionTitle(this.text);

  final String text;

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.only(bottom: 4),
        child: Text(
          text,
          style: Theme.of(context).textTheme.titleSmall,
        ),
      );
}

/// TurnArtifactItem（无 id 轻量形态）→ Artifact，供产物卡片复用。
Artifact _toArtifact(TurnArtifactItem item, String workspaceId) => Artifact(
      id: 'live_${item.path}',
      workspaceId: workspaceId,
      sessionId: '',
      turnId: 0,
      path: item.path,
      op: item.op,
      size: item.size,
      linesAdded: item.linesAdded,
      linesDeleted: item.linesDeleted,
      source: ArtifactSource.live,
      deliverable: item.deliverable,
      createdAt: DateTime.now(),
    );

String _formatCount(int n) => '$n';

/// 历史分组时间：当天显示 `HH:mm`，跨天显示 `MM-dd HH:mm`。
String _formatTime(DateTime t) {
  String two(int n) => n.toString().padLeft(2, '0');
  final hm = '${two(t.hour)}:${two(t.minute)}';
  final now = DateTime.now();
  final sameDay =
      t.year == now.year && t.month == now.month && t.day == now.day;
  return sameDay ? hm : '${two(t.month)}-${two(t.day)} $hm';
}
