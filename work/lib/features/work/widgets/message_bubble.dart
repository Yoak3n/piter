/// 消息气泡（user 右对齐 / assistant 左对齐 / system 居中）。
library;

import 'package:flutter/material.dart';

import '../../../core/network/models/models.dart';

class MessageBubble extends StatelessWidget {
  const MessageBubble({super.key, required this.message, this.streaming = false});

  final PiMessage message;
  final bool streaming;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    switch (message.role) {
      case PiMessageRole.user:
        return Align(
          alignment: Alignment.centerRight,
          child: Container(
            margin: const EdgeInsets.symmetric(vertical: 4),
            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
            constraints: const BoxConstraints(maxWidth: 560),
            decoration: BoxDecoration(
              color: scheme.primaryContainer,
              borderRadius: BorderRadius.circular(12),
            ),
            child: Text(message.content, style: Theme.of(context).textTheme.bodyMedium),
          ),
        );
      case PiMessageRole.system:
        return Align(
          alignment: Alignment.center,
          child: Padding(
            padding: const EdgeInsets.symmetric(vertical: 6),
            child: Text(
              message.content,
              style: Theme.of(context).textTheme.bodySmall?.copyWith(color: scheme.outline),
            ),
          ),
        );
      case PiMessageRole.assistant:
      case PiMessageRole.toolResult:
        return Align(
          alignment: Alignment.centerLeft,
          child: Container(
            margin: const EdgeInsets.symmetric(vertical: 4),
            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
            constraints: const BoxConstraints(maxWidth: 640),
            decoration: BoxDecoration(
              color: scheme.surfaceContainerLow,
              borderRadius: BorderRadius.circular(12),
            ),
            child: _AssistantContent(message: message, streaming: streaming),
          ),
        );
    }
  }
}

class _AssistantContent extends StatelessWidget {
  const _AssistantContent({required this.message, required this.streaming});

  final PiMessage message;
  final bool streaming;

  @override
  Widget build(BuildContext context) {
    final text = message.content.isEmpty ? '…' : message.content;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (text.isNotEmpty) Text(text, style: Theme.of(context).textTheme.bodyMedium),
        if (streaming)
          Padding(
            padding: const EdgeInsets.only(top: 4),
            child: Text(
              '●',
              style: TextStyle(fontSize: 10, color: Theme.of(context).colorScheme.primary),
            ),
          ),
      ],
    );
  }
}
