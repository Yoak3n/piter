/// 写阻断批准条（ask 模式）：展示阻断路径 + 批准/拒绝按钮。
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../providers/work_session.dart';
import '../providers/work_session_provider.dart';

class WriteBlockCard extends ConsumerWidget {
  const WriteBlockCard({super.key, required this.entry});

  final WriteBlockEntry entry;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final scheme = Theme.of(context).colorScheme;
    final block = entry.block;

    return Container(
      margin: const EdgeInsets.symmetric(vertical: 6),
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: scheme.tertiaryContainer.withValues(alpha: 0.35),
        border: Border.all(color: scheme.tertiary.withValues(alpha: 0.5)),
        borderRadius: BorderRadius.circular(12),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(Icons.warning_amber_rounded, size: 18, color: scheme.tertiary),
              const SizedBox(width: 6),
              Text(
                '写阻断 · 需要批准',
                style: Theme.of(context)
                    .textTheme
                    .labelLarge
                    ?.copyWith(color: scheme.onTertiaryContainer),
              ),
            ],
          ),
          const SizedBox(height: 8),
          Text(
            block.path,
            style: Theme.of(context).textTheme.bodySmall?.copyWith(
                  fontFamily: 'monospace',
                  color: scheme.onTertiaryContainer,
                ),
          ),
          const SizedBox(height: 4),
          Text(
            block.reason,
            style: Theme.of(context)
                .textTheme
                .bodySmall
                ?.copyWith(color: scheme.onSurfaceVariant),
          ),
          const SizedBox(height: 10),
          switch (entry.state) {
            WriteBlockState.pending => Row(
                children: [
                  FilledButton.icon(
                    onPressed: () => ref.read(workSessionProvider.notifier).approveWrite(allow: true),
                    icon: const Icon(Icons.check, size: 16),
                    label: const Text('批准'),
                  ),
                  const SizedBox(width: 8),
                  OutlinedButton.icon(
                    onPressed: () => ref.read(workSessionProvider.notifier).approveWrite(allow: false),
                    icon: const Icon(Icons.close, size: 16),
                    label: const Text('拒绝'),
                  ),
                ],
              ),
            WriteBlockState.approved => Row(
                children: [
                  Icon(Icons.check_circle, size: 16, color: scheme.tertiary),
                  const SizedBox(width: 6),
                  Text('已批准，本次写入放行', style: Theme.of(context).textTheme.bodySmall),
                ],
              ),
            WriteBlockState.denied => Row(
                children: [
                  Icon(Icons.cancel_outlined, size: 16, color: scheme.error),
                  const SizedBox(width: 6),
                  Text('已拒绝写入', style: Theme.of(context).textTheme.bodySmall),
                ],
              ),
          },
        ],
      ),
    );
  }
}
