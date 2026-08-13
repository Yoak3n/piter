/// 产物卡片：按 turn 分组展示，deliverable 高亮 + 下载/标记按钮。
library;

import 'package:flutter/material.dart';

import '../../../core/network/models/models.dart';

class ArtifactCard extends StatelessWidget {
  const ArtifactCard({
    super.key,
    required this.artifact,
    this.onDownload,
    this.onToggleDeliverable,
  });

  final Artifact artifact;
  final VoidCallback? onDownload;
  final ValueChanged<bool>? onToggleDeliverable;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final op = artifact.op;
    final (opBg, opFg, opLabel) = switch (op) {
      ArtifactOp.newFile => (scheme.tertiaryContainer, scheme.onTertiaryContainer, '新增'),
      ArtifactOp.modified => (scheme.secondaryContainer, scheme.onSecondaryContainer, '修改'),
      ArtifactOp.deleted => (scheme.errorContainer, scheme.onErrorContainer, '删除'),
    };

    return Container(
      margin: const EdgeInsets.symmetric(vertical: 4),
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
      decoration: BoxDecoration(
        color: scheme.surfaceContainerLow,
        border: Border.all(
          color: artifact.deliverable ? scheme.tertiary : scheme.outlineVariant,
          width: artifact.deliverable ? 1.2 : 1,
        ),
        borderRadius: BorderRadius.circular(10),
      ),
      child: Row(
        children: [
          Icon(
            artifact.deliverable ? Icons.star_rounded : Icons.insert_drive_file_outlined,
            size: 18,
            color: artifact.deliverable ? scheme.tertiary : scheme.outline,
          ),
          const SizedBox(width: 8),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  artifact.path,
                  style: Theme.of(context)
                      .textTheme
                      .bodySmall
                      ?.copyWith(fontFamily: 'monospace'),
                  overflow: TextOverflow.ellipsis,
                ),
                const SizedBox(height: 2),
                Row(
                  children: [
                    Container(
                      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1),
                      decoration: BoxDecoration(
                        color: opBg,
                        borderRadius: BorderRadius.circular(4),
                      ),
                      child: Text(
                        opLabel,
                        style: Theme.of(context).textTheme.labelSmall?.copyWith(color: opFg),
                      ),
                    ),
                    const SizedBox(width: 6),
                    Text(
                      _formatSize(artifact.size),
                      style: Theme.of(context).textTheme.labelSmall?.copyWith(color: scheme.outline),
                    ),
                    if (artifact.linesAdded > 0 || artifact.linesDeleted > 0) ...[
                      const SizedBox(width: 6),
                      Text.rich(
                        TextSpan(
                          children: [
                            if (artifact.linesAdded > 0)
                              TextSpan(
                                text: '+${artifact.linesAdded}',
                                style: const TextStyle(
                                  color: Color(0xFF2E7D32),
                                  fontFamily: 'monospace',
                                ),
                              ),
                            if (artifact.linesDeleted > 0) ...[
                              if (artifact.linesAdded > 0) const TextSpan(text: ' '),
                              TextSpan(
                                text: '-${artifact.linesDeleted}',
                                style: const TextStyle(
                                  color: Color(0xFFC62828),
                                  fontFamily: 'monospace',
                                ),
                              ),
                            ],
                          ],
                        ),
                        style: Theme.of(context).textTheme.labelSmall,
                      ),
                    ],
                    if (artifact.deliverable) ...[
                      const SizedBox(width: 6),
                      Text(
                        '交付物',
                        style: Theme.of(context).textTheme.labelSmall?.copyWith(color: scheme.tertiary),
                      ),
                    ],
                  ],
                ),
              ],
            ),
          ),
          if (onDownload != null)
            IconButton(
              onPressed: onDownload,
              icon: Icon(Icons.download_outlined, size: 18, color: scheme.primary),
              tooltip: '下载',
            ),
          if (onToggleDeliverable != null)
            IconButton(
              onPressed: () => onToggleDeliverable!(!artifact.deliverable),
              icon: Icon(
                artifact.deliverable ? Icons.star : Icons.star_border,
                size: 18,
                color: artifact.deliverable ? scheme.tertiary : scheme.outline,
              ),
              tooltip: '标记/取消交付物',
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
