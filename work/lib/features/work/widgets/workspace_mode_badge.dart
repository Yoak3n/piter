/// 写边界模式徽标。
library;

import 'package:flutter/material.dart';

import '../../../core/network/models/models.dart';

class WorkspaceModeBadge extends StatelessWidget {
  const WorkspaceModeBadge({super.key, required this.mode});

  final WorkspaceMode mode;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final (bg, fg, label, icon) = switch (mode) {
      WorkspaceMode.ask => (
          scheme.tertiaryContainer,
          scheme.onTertiaryContainer,
          '询问',
          Icons.help_outline,
        ),
      WorkspaceMode.allow => (
          scheme.secondaryContainer,
          scheme.onSecondaryContainer,
          '放行',
          Icons.check_circle_outline,
        ),
      WorkspaceMode.deny => (
          scheme.errorContainer,
          scheme.onErrorContainer,
          '拒绝',
          Icons.block,
        ),
    };
    return Tooltip(
      message: _scopeHint(mode),
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
        decoration: BoxDecoration(
          color: bg,
          borderRadius: BorderRadius.circular(999),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, size: 12, color: fg),
            const SizedBox(width: 4),
            Text(
              label,
              style: Theme.of(context).textTheme.labelSmall?.copyWith(color: fg),
            ),
          ],
        ),
      ),
    );
  }
}

/// 悬停提示：说明当前模式含义，并强调只作用于工作空间外。
String _scopeHint(WorkspaceMode mode) => switch (mode) {
      WorkspaceMode.ask => '越界写入（工作空间外）每次询问；内部路径始终放行',
      WorkspaceMode.allow => '放行全部写入（含工作空间外）；内部路径始终放行',
      WorkspaceMode.deny => '拒绝工作空间外写入；内部路径始终放行',
    };
