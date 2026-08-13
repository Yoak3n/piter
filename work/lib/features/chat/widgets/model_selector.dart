/// 模型选择下拉（弹出底部选单；切换只写 per-session 状态，随 prompt 生效）。
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../core/network/models/models.dart';
import '../providers/models_provider.dart';

class ModelSelector extends ConsumerWidget {
  const ModelSelector({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final models = ref.watch(chatModelsProvider).valueOrNull ?? const <ModelInfo>[];
    final current = ref.watch(currentChatModelProvider);
    final scheme = Theme.of(context).colorScheme;

    if (models.isEmpty) return const SizedBox.shrink();

    return PopupMenuButton<ModelInfo>(
      tooltip: '选择模型',
      onSelected: (m) => ref.read(currentChatModelProvider.notifier).state = m,
      itemBuilder: (context) => [
        for (final m in models)
          PopupMenuItem(
            value: m,
            child: Row(
              children: [
                if (m.id == current?.id)
                  Icon(Icons.check, size: 16, color: scheme.primary),
                const SizedBox(width: 8),
                Flexible(
                  child: Text(m.display, overflow: TextOverflow.ellipsis),
                ),
              ],
            ),
          ),
      ],
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 8),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(Icons.smart_toy_outlined, size: 18, color: scheme.primary),
            const SizedBox(width: 4),
            ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 120),
              child: Text(
                current?.id ?? '模型',
                style: Theme.of(context).textTheme.labelMedium,
                overflow: TextOverflow.ellipsis,
              ),
            ),
            Icon(Icons.arrow_drop_down, size: 18, color: scheme.outline),
          ],
        ),
      ),
    );
  }
}
