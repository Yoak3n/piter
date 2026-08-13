/// 工具块：可展开，edit 工具展示 args + output + unified diff。
library;

import 'package:flutter/material.dart';

import '../../../core/network/models/models.dart';
import 'diff_view.dart';

class ToolBlock extends StatefulWidget {
  const ToolBlock({super.key, required this.tool});

  final ToolExecution tool;

  @override
  State<ToolBlock> createState() => _ToolBlockState();
}

class _ToolBlockState extends State<ToolBlock> {
  bool _expanded = false;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final tool = widget.tool;
    final (statusBg, statusFg, statusLabel) = switch (tool.status) {
      ToolExecutionStatus.pending => (scheme.surfaceContainerHighest, scheme.outline, 'pending'),
      ToolExecutionStatus.streaming => (scheme.primaryContainer, scheme.primary, 'streaming'),
      ToolExecutionStatus.complete => (scheme.secondaryContainer, scheme.onSecondaryContainer, 'complete'),
      ToolExecutionStatus.error => (scheme.errorContainer, scheme.onErrorContainer, 'error'),
    };

    return Container(
      margin: const EdgeInsets.symmetric(vertical: 4),
      decoration: BoxDecoration(
        color: scheme.surfaceContainerLow,
        border: Border.all(color: scheme.outlineVariant),
        borderRadius: BorderRadius.circular(10),
      ),
      clipBehavior: Clip.antiAlias,
      child: Column(
        children: [
          InkWell(
            onTap: () => setState(() => _expanded = !_expanded),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
              child: Row(
                children: [
                  Icon(
                    _expanded ? Icons.expand_more : Icons.chevron_right,
                    size: 16,
                    color: scheme.outline,
                  ),
                  const SizedBox(width: 6),
                  Text(
                    tool.toolName,
                    style: Theme.of(context)
                        .textTheme
                        .labelMedium
                        ?.copyWith(color: scheme.primary, fontFamily: 'monospace'),
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      tool.argsPreview,
                      style: Theme.of(context)
                          .textTheme
                          .labelSmall
                          ?.copyWith(color: scheme.outline, fontFamily: 'monospace'),
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                  Container(
                    padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
                    decoration: BoxDecoration(
                      color: statusBg,
                      borderRadius: BorderRadius.circular(999),
                    ),
                    child: Text(
                      statusLabel,
                      style: Theme.of(context).textTheme.labelSmall?.copyWith(color: statusFg),
                    ),
                  ),
                ],
              ),
            ),
          ),
          if (_expanded) ...[
            const Divider(height: 1),
            Padding(
              padding: const EdgeInsets.all(10),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  if (tool.args.isNotEmpty) ...[
                    const _Label('参数'),
                    _MonoText(_formatArgs(tool.args)),
                  ],
                  if (tool.patch != null) ...[
                    const SizedBox(height: 8),
                    const _Label('变更 diff'),
                    const SizedBox(height: 4),
                    DiffView(patch: tool.patch!),
                  ],
                  if (tool.output.isNotEmpty) ...[
                    const SizedBox(height: 8),
                    const _Label('结果'),
                    _MonoText(tool.output),
                  ],
                ],
              ),
            ),
          ],
        ],
      ),
    );
  }
}

class _Label extends StatelessWidget {
  const _Label(this.text);

  final String text;

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.only(bottom: 4),
        child: Text(
          text,
          style: Theme.of(context).textTheme.labelSmall?.copyWith(
                color: Theme.of(context).colorScheme.outline,
              ),
        ),
      );
}

class _MonoText extends StatelessWidget {
  const _MonoText(this.text);

  final String text;

  @override
  Widget build(BuildContext context) => Text(
        text,
        style: Theme.of(context).textTheme.bodySmall?.copyWith(fontFamily: 'monospace'),
      );
}

String _formatArgs(Map<String, dynamic> args) {
  try {
    // 简化展示：路径优先，其余平铺。
    final parts = <String>[];
    for (final entry in args.entries) {
      if (entry.value is Map || entry.value is List) continue;
      parts.add('${entry.key}: ${entry.value}');
    }
    return parts.join('\n');
  } catch (_) {
    return args.toString();
  }
}
