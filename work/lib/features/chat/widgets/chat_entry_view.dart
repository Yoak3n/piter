/// 原生 chat 消息渲染：Markdown 正文 + 可折叠思考 + 图片 + 工具块 + 扩展卡片。
library;

import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_markdown/flutter_markdown.dart';

import '../../../core/network/models/models.dart';
import '../../work/widgets/tool_block.dart';
import '../providers/chat_session.dart';

/// 渲染一条会话时间线条目（消息 / 工具 / 系统提示 / 扩展卡片）。
class ChatEntryView extends StatelessWidget {
  const ChatEntryView({
    super.key,
    required this.entry,
    this.onExtRespond,
  });

  final ChatEntry entry;

  /// 扩展卡片应答回调（(id, method, value) → sendExtResponse）。
  final void Function(String id, String method, dynamic value)? onExtRespond;

  @override
  Widget build(BuildContext context) {
    return switch (entry) {
      ChatMsgEntry() => ChatMessageView(entry: entry as ChatMsgEntry),
      ChatToolEntry() => _wrap(context, ToolBlock(tool: (entry as ChatToolEntry).tool)),
      ChatNoticeEntry() => _notice(
          context,
          (entry as ChatNoticeEntry).kind,
          (entry as ChatNoticeEntry).message,
        ),
      ChatExtEntry() => ExtensionCard(ui: (entry as ChatExtEntry).ui, onRespond: onExtRespond),
    };
  }

  Widget _wrap(BuildContext context, Widget child) => Padding(
        padding: const EdgeInsets.symmetric(vertical: 2),
        child: child,
      );

  Widget _notice(BuildContext context, String kind, String message) {
    final scheme = Theme.of(context).colorScheme;
    return Align(
      alignment: Alignment.center,
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 4),
        child: Text(
          message,
          textAlign: TextAlign.center,
          style: Theme.of(context)
              .textTheme
              .bodySmall
              ?.copyWith(color: kind == 'error' ? scheme.error : scheme.outline),
        ),
      ),
    );
  }
}

// ─── 消息气泡 ──────────────────────────────────────────────────────────────

class ChatMessageView extends StatelessWidget {
  const ChatMessageView({super.key, required this.entry});

  final ChatMsgEntry entry;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final msg = entry.message;
    final text = entry.streaming ? entry.textBuffer : msg.text;

    switch (msg.role) {
      case 'user':
        return Align(
          alignment: Alignment.centerRight,
          child: Container(
            margin: const EdgeInsets.symmetric(vertical: 4),
            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
            constraints: const BoxConstraints(maxWidth: 640),
            decoration: BoxDecoration(
              color: scheme.primaryContainer,
              borderRadius: BorderRadius.circular(12),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                if (msg.hasImages) ...[
                  for (final b in msg.blocks)
                    if (b is ImageBlock) _image(context, b),
                  if (text.isNotEmpty) const SizedBox(height: 8),
                ],
                if (text.isNotEmpty)
                  SelectableText(text, style: Theme.of(context).textTheme.bodyMedium),
              ],
            ),
          ),
        );
      case 'system':
        final ui = msg.extUi;
        if (ui != null) {
          return ExtensionCard(ui: ui);
        }
        return Align(
          alignment: Alignment.center,
          child: Padding(
            padding: const EdgeInsets.symmetric(vertical: 6),
            child: Text(
              text,
              textAlign: TextAlign.center,
              style: Theme.of(context)
                  .textTheme
                  .bodySmall
                  ?.copyWith(color: scheme.outline),
            ),
          ),
        );
      default:
        return Align(
          alignment: Alignment.centerLeft,
          child: Container(
            margin: const EdgeInsets.symmetric(vertical: 4),
            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
            constraints: const BoxConstraints(maxWidth: 680),
            decoration: BoxDecoration(
              color: scheme.surfaceContainerLow,
              borderRadius: BorderRadius.circular(12),
            ),
            child: _AssistantBody(entry: entry),
          ),
        );
    }
  }

  Widget _image(BuildContext context, ImageBlock block) {
    final bytes = base64Decode(block.data);
    return ClipRRect(
      borderRadius: BorderRadius.circular(8),
      child: Image.memory(bytes, width: 180, fit: BoxFit.cover),
    );
  }
}

class _AssistantBody extends StatelessWidget {
  const _AssistantBody({required this.entry});

  final ChatMsgEntry entry;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final msg = entry.message;
    final thinkingText = entry.streaming ? entry.thinkingBuffer : '';
    final text = entry.streaming ? entry.textBuffer : msg.text;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (msg.hasThinking && !entry.streaming)
          for (final b in msg.blocks)
            if (b is ThinkingBlock)
              _ThinkingFold(
                text: b.thinking,
                autoExpanded: false,
              ),
        if (thinkingText.isNotEmpty || (msg.hasThinking && entry.streaming))
          _ThinkingFold(text: thinkingText, autoExpanded: true),
        if (msg.hasImages)
          for (final b in msg.blocks)
            if (b is ImageBlock) _image(context, b),
        if (text.isNotEmpty)
          MarkdownBody(
            data: text + (entry.streaming ? ' ▍' : ''),
            selectable: true,
            styleSheet: MarkdownStyleSheet.fromTheme(Theme.of(context)).copyWith(
              p: Theme.of(context)
                  .textTheme
                  .bodyMedium
                  ?.copyWith(height: 1.45),
            ),
          )
        else if (entry.streaming)
          Text(
            '▍',
            style: TextStyle(color: scheme.primary),
          ),
        if (entry.streaming)
          const SizedBox(height: 2),
        if (msg.model != null && !entry.streaming)
          Padding(
            padding: const EdgeInsets.only(top: 4),
            child: Text(
              msg.model!,
              style: Theme.of(context).textTheme.labelSmall?.copyWith(color: scheme.outline),
            ),
          ),
      ],
    );
  }

  Widget _image(BuildContext context, ImageBlock block) {
    final bytes = base64Decode(block.data);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: ClipRRect(
        borderRadius: BorderRadius.circular(8),
        child: Image.memory(bytes, width: 180, fit: BoxFit.cover),
      ),
    );
  }
}

/// 思考折叠块（流式时自动展开）。
class _ThinkingFold extends StatefulWidget {
  const _ThinkingFold({required this.text, required this.autoExpanded});

  final String text;
  final bool autoExpanded;

  @override
  State<_ThinkingFold> createState() => _ThinkingFoldState();
}

class _ThinkingFoldState extends State<_ThinkingFold> {
  late bool _expanded = widget.autoExpanded;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Container(
      margin: const EdgeInsets.only(bottom: 6),
      decoration: BoxDecoration(
        color: scheme.surfaceContainerHigh,
        borderRadius: BorderRadius.circular(8),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          InkWell(
            onTap: () => setState(() => _expanded = !_expanded),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
              child: Row(
                children: [
                  Icon(Icons.psychology_outlined, size: 14, color: scheme.primary),
                  const SizedBox(width: 6),
                  Expanded(
                    child: Text(
                      _expanded ? '收起思考' : '展开思考',
                      style: Theme.of(context).textTheme.labelSmall,
                    ),
                  ),
                  Icon(
                    _expanded ? Icons.expand_less : Icons.expand_more,
                    size: 16,
                    color: scheme.outline,
                  ),
                ],
              ),
            ),
          ),
          if (_expanded)
            Container(
              width: double.infinity,
              padding: const EdgeInsets.fromLTRB(10, 0, 10, 8),
              child: SelectableText(
                widget.text,
                style: Theme.of(context).textTheme.bodySmall?.copyWith(color: scheme.outline),
              ),
            ),
        ],
      ),
    );
  }
}

// ─── 扩展 UI 卡片 ──────────────────────────────────────────────────────────

class ExtensionCard extends StatefulWidget {
  const ExtensionCard({super.key, required this.ui, this.onRespond});

  final ChatExtUi ui;

  /// (id, method, value) → sendExtResponse。
  final void Function(String id, String method, dynamic value)? onRespond;

  @override
  State<ExtensionCard> createState() => _ExtensionCardState();
}

class _ExtensionCardState extends State<ExtensionCard> {
  final _input = TextEditingController();

  @override
  void initState() {
    super.initState();
    _input.text = widget.ui.prefill ?? '';
  }

  @override
  void dispose() {
    _input.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final ui = widget.ui;
    final answered = ui.answered;

    return Container(
      margin: const EdgeInsets.symmetric(vertical: 6),
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: scheme.surfaceContainerLow,
        border: Border.all(color: scheme.primary.withValues(alpha: 0.4)),
        borderRadius: BorderRadius.circular(10),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(Icons.extension_outlined, size: 16, color: scheme.primary),
              const SizedBox(width: 6),
              Expanded(
                child: Text(
                  ui.title,
                  style: Theme.of(context).textTheme.labelLarge,
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              if (answered)
                Text(
                  '已应答',
                  style: Theme.of(context)
                      .textTheme
                      .labelSmall
                      ?.copyWith(color: scheme.outline),
                ),
            ],
          ),
          if (ui.message != null && ui.message!.isNotEmpty) ...[
            const SizedBox(height: 6),
            Text(ui.message!, style: Theme.of(context).textTheme.bodySmall),
          ],
          const SizedBox(height: 8),
          ..._body(context, ui),
        ],
      ),
    );
  }

  List<Widget> _body(BuildContext context, ChatExtUi ui) {
    if (ui.answered) {
      return [
        SelectableText(
          _resultText(ui),
          style: Theme.of(context).textTheme.bodySmall,
        ),
      ];
    }
    switch (ui.method) {
      case 'confirm':
        return [
          Row(
            mainAxisAlignment: MainAxisAlignment.end,
            children: [
              TextButton(
                onPressed: () => widget.onRespond?.call(ui.id, ui.method, false),
                child: const Text('取消'),
              ),
              const SizedBox(width: 8),
              FilledButton(
                onPressed: () => widget.onRespond?.call(ui.id, ui.method, true),
                child: const Text('确认'),
              ),
            ],
          ),
        ];
      case 'input':
        return [
          TextField(
            controller: _input,
            decoration: InputDecoration(
              hintText: ui.placeholder,
              isDense: true,
              border: const OutlineInputBorder(),
            ),
            onSubmitted: (v) =>
                widget.onRespond?.call(ui.id, ui.method, v),
          ),
          const SizedBox(height: 8),
          Row(
            mainAxisAlignment: MainAxisAlignment.end,
            children: [
              TextButton(
                onPressed: () =>
                    widget.onRespond?.call(ui.id, ui.method, null),
                child: const Text('取消'),
              ),
              const SizedBox(width: 8),
              FilledButton(
                onPressed: () => widget.onRespond
                    ?.call(ui.id, ui.method, _input.text.trim()),
                child: const Text('提交'),
              ),
            ],
          ),
        ];
      case 'select':
        return [
          for (final opt in ui.options)
            ListTile(
              dense: true,
              contentPadding: EdgeInsets.zero,
              title: Text(
                opt['label'] as String? ?? opt['value']?.toString() ?? '',
                style: Theme.of(context).textTheme.bodySmall,
              ),
              onTap: () => widget.onRespond
                  ?.call(ui.id, ui.method, opt['value'] ?? opt['label']),
            ),
        ];
      default:
        // editor 或未识别：多行输入。
        return [
          TextField(
            controller: _input,
            minLines: 3,
            maxLines: 8,
            decoration: InputDecoration(
              hintText: ui.placeholder ?? ui.message,
              border: const OutlineInputBorder(),
            ),
          ),
          const SizedBox(height: 8),
          Row(
            mainAxisAlignment: MainAxisAlignment.end,
            children: [
              TextButton(
                onPressed: () =>
                    widget.onRespond?.call(ui.id, ui.method, null),
                child: const Text('取消'),
              ),
              const SizedBox(width: 8),
              FilledButton(
                onPressed: () => widget.onRespond
                    ?.call(ui.id, ui.method, _input.text.trim()),
                child: const Text('提交'),
              ),
            ],
          ),
        ];
    }
  }

  String _resultText(ChatExtUi ui) {
    final r = ui.result;
    if (r == null) return '（已取消）';
    return r.toString();
  }
}
