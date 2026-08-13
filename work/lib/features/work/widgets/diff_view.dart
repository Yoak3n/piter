/// unified diff 渲染骨架（edit 工具 `details.patch`）。
/// 自写轻量解析（diff_parser）→ 行内 add/del 高亮。
library;

import 'package:flutter/material.dart';

import '../../../core/utils/diff_parser.dart';

class DiffView extends StatelessWidget {
  const DiffView({super.key, required this.patch});

  final String patch;

  @override
  Widget build(BuildContext context) {
    final diff = parseUnifiedDiff(patch);
    if (diff.isEmpty) {
      return Padding(
        padding: const EdgeInsets.all(12),
        child: Text(
          '（无有效 diff）',
          style: Theme.of(context).textTheme.bodySmall,
        ),
      );
    }
    final scheme = Theme.of(context).colorScheme;
    final mono = Theme.of(context).textTheme.bodySmall?.copyWith(fontFamily: 'monospace');
    final children = <Widget>[];
    // header（--- / +++）
    for (final line in diff.lines) {
      if (line.kind != DiffLineKind.header) break;
      children.add(Container(
        width: double.infinity,
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
        color: scheme.surfaceContainerHighest,
        child: Text('  ${line.text}', style: mono?.copyWith(color: scheme.onSurfaceVariant)),
      ));
    }
    for (final hunk in diff.hunks) {
      children.add(Container(
        width: double.infinity,
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
        color: scheme.primaryContainer.withValues(alpha: 0.4),
        child: Text(
          '@@ -${hunk.oldStart},${hunk.oldCount} +${hunk.newStart},${hunk.newCount} @@',
          style: mono?.copyWith(color: scheme.primary),
        ),
      ));
      for (final line in hunk.lines) {
        final (bg, fg, mark) = switch (line.kind) {
          DiffLineKind.addition => (scheme.tertiaryContainer.withValues(alpha: 0.45), scheme.onSurface, '+'),
          DiffLineKind.deletion => (scheme.errorContainer.withValues(alpha: 0.45), scheme.onSurface, '-'),
          _ => (Colors.transparent, scheme.onSurfaceVariant, ' '),
        };
        children.add(Container(
          width: double.infinity,
          color: bg,
          padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 1),
          child: Text(
            '$mark ${line.text}',
            style: mono?.copyWith(color: fg),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
          ),
        ));
      }
    }
    return Container(
      decoration: BoxDecoration(
        border: Border.all(color: scheme.outlineVariant),
        borderRadius: BorderRadius.circular(8),
      ),
      clipBehavior: Clip.antiAlias,
      child: SingleChildScrollView(
        scrollDirection: Axis.horizontal,
        child: ConstrainedBox(
          constraints: const BoxConstraints(minWidth: 480),
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: children),
        ),
      ),
    );
  }
}
