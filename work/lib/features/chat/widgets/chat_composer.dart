/// 原生 chat 输入区：多行输入 + 图片附件 + 斜杠补全 + 发送/停止。
library;

import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:image_picker/image_picker.dart';

import '../../../core/network/models/models.dart';
import '../providers/chat_session_provider.dart';
import '../providers/models_provider.dart';

/// 一条已选附件（发送时转 {type:'image', data, mimeType}）。
class ChatAttachment {
  const ChatAttachment({required this.data, required this.mimeType});
  final String data;
  final String mimeType;
}

class ChatComposer extends ConsumerStatefulWidget {
  const ChatComposer({super.key, required this.cwd});

  /// 会话所属项目工作目录（新建会话时用；空则当前目录）。
  final String cwd;

  @override
  ConsumerState<ChatComposer> createState() => _ChatComposerState();
}

class _ChatComposerState extends ConsumerState<ChatComposer> {
  final _input = TextEditingController();
  final _picker = ImagePicker();
  final List<ChatAttachment> _attachments = [];
  bool _slashOpen = false;

  @override
  void dispose() {
    _input.dispose();
    super.dispose();
  }

  void _onChanged(String text) {
    final open = text.startsWith('/') && !text.contains(' ');
    if (open != _slashOpen) {
      setState(() => _slashOpen = open);
    }
    if (open) {
      // 懒加载斜杠命令。
      final cmds = ref.read(chatSessionProvider).slashCommands;
      if (cmds.isEmpty) {
        ref.read(chatSessionProvider.notifier).getCommands();
      }
    }
  }

  Future<void> _pickImages() async {
    try {
      final picked = await _picker.pickMultiImage(limit: 9);
      if (picked.isEmpty) return;
      for (final f in picked) {
        final bytes = await f.readAsBytes();
        if (bytes.length > 8 * 1024 * 1024) continue;
        setState(() {
          _attachments.add(ChatAttachment(
            data: base64Encode(bytes),
            mimeType: f.mimeType ?? 'image/jpeg',
          ));
        });
      }
    } catch (_) {
      // 选择器不可用（桌面/测试）时静默。
    }
  }

  void _send([String? text]) {
    final content = (text ?? _input.text).trim();
    if (content.isEmpty && _attachments.isEmpty) return;
    final session = ref.read(chatSessionProvider);
    if (!session.connected || session.instanceId.isEmpty) return;

    final model = ref.read(currentChatModelProvider);
    final images = _attachments.isEmpty
        ? null
        : [
            for (final a in _attachments)
              {'type': 'image', 'data': a.data, 'mimeType': a.mimeType},
          ];
    ref.read(chatSessionProvider.notifier).sendPrompt(
          content,
          modelId: model?.id,
          provider: model?.provider,
          images: images,
        );
    _input.clear();
    setState(() {
      _attachments.clear();
      _slashOpen = false;
    });
  }

  void _stop() {
    ref.read(chatSessionProvider.notifier).abort();
  }

  @override
  Widget build(BuildContext context) {
    final session = ref.watch(chatSessionProvider);
    final scheme = Theme.of(context).colorScheme;
    final canSend = session.connected && session.instanceId.isNotEmpty;

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        // 附件缩略图行。
        if (_attachments.isNotEmpty)
          SizedBox(
            height: 56,
            child: ListView.separated(
              scrollDirection: Axis.horizontal,
              padding: const EdgeInsets.symmetric(horizontal: 12),
              itemCount: _attachments.length,
              separatorBuilder: (_, _) => const SizedBox(width: 8),
              itemBuilder: (context, i) {
                final a = _attachments[i];
                return Stack(
                  children: [
                    ClipRRect(
                      borderRadius: BorderRadius.circular(8),
                      child: Image.memory(
                        base64Decode(a.data),
                        width: 48,
                        height: 48,
                        fit: BoxFit.cover,
                      ),
                    ),
                    Positioned(
                      top: 0,
                      right: 0,
                      child: InkWell(
                        onTap: () => setState(() => _attachments.removeAt(i)),
                        child: Container(
                          decoration: const BoxDecoration(
                            color: Colors.black54,
                            shape: BoxShape.circle,
                          ),
                          child: const Icon(Icons.close, size: 14, color: Colors.white),
                        ),
                      ),
                    ),
                  ],
                );
              },
            ),
          ),
        // 斜杠补全浮层。
        if (_slashOpen)
          _SlashMenu(
            commands: session.slashCommands,
            onSelect: (cmd) {
              _input.text = '/${cmd.name} ';
              _input.selection = TextSelection.collapsed(offset: _input.text.length);
              setState(() => _slashOpen = false);
            },
          ),
        SafeArea(
          top: false,
          child: Padding(
            padding: const EdgeInsets.fromLTRB(8, 6, 8, 8),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                IconButton(
                  onPressed: _pickImages,
                  icon: Icon(Icons.image_outlined, color: scheme.primary),
                  tooltip: '添加图片',
                ),
                Expanded(
                  child: TextField(
                    controller: _input,
                    onChanged: _onChanged,
                    minLines: 1,
                    maxLines: 5,
                    textInputAction: TextInputAction.send,
                    onSubmitted: (_) => _send(),
                    decoration: InputDecoration(
                      hintText: canSend ? '输入消息，/ 查看命令' : '会话建立中…',
                      isDense: true,
                      border: OutlineInputBorder(
                        borderRadius: BorderRadius.circular(20),
                      ),
                      contentPadding: const EdgeInsets.symmetric(
                        horizontal: 16,
                        vertical: 10,
                      ),
                    ),
                  ),
                ),
                const SizedBox(width: 6),
                if (session.streaming)
                  IconButton.filled(
                    onPressed: _stop,
                    icon: const Icon(Icons.stop),
                    tooltip: '停止',
                  )
                else
                  IconButton.filled(
                    onPressed: canSend ? () => _send() : null,
                    icon: const Icon(Icons.send),
                    tooltip: '发送',
                  ),
              ],
            ),
          ),
        ),
        // outbox 排队提示。
        if (session.outbox.isNotEmpty)
          Padding(
            padding: const EdgeInsets.only(bottom: 4),
            child: Row(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                Icon(Icons.schedule, size: 12, color: scheme.outline),
                const SizedBox(width: 4),
                Text(
                  '当前回合结束后将发送 ${session.outbox.length} 条排队消息',
                  style: Theme.of(context)
                      .textTheme
                      .labelSmall
                      ?.copyWith(color: scheme.outline),
                ),
              ],
            ),
          ),
      ],
    );
  }
}

/// 斜杠命令浮层。
class _SlashMenu extends StatelessWidget {
  const _SlashMenu({required this.commands, required this.onSelect});

  final List<SlashCommand> commands;
  final ValueChanged<SlashCommand> onSelect;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    if (commands.isEmpty) {
      return Padding(
        padding: const EdgeInsets.symmetric(horizontal: 16),
        child: Text(
          '加载命令中…',
          style: Theme.of(context).textTheme.labelSmall?.copyWith(color: scheme.outline),
        ),
      );
    }
    return Container(
      margin: const EdgeInsets.symmetric(horizontal: 8),
      constraints: const BoxConstraints(maxHeight: 240),
      decoration: BoxDecoration(
        color: scheme.surfaceContainerLow,
        borderRadius: BorderRadius.circular(10),
        border: Border.all(color: scheme.outlineVariant),
      ),
      child: ListView.builder(
        shrinkWrap: true,
        itemCount: commands.length,
        itemBuilder: (context, i) {
          final cmd = commands[i];
          return ListTile(
            dense: true,
            leading: Text(
              '/${cmd.name}',
              style: Theme.of(context).textTheme.bodySmall?.copyWith(
                    color: scheme.primary,
                    fontFamily: 'monospace',
                  ),
            ),
            title: Text(
              cmd.description,
              style: Theme.of(context).textTheme.bodySmall,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
            ),
            trailing: Text(
              cmd.source,
              style: Theme.of(context).textTheme.labelSmall?.copyWith(color: scheme.outline),
            ),
            onTap: () => onSelect(cmd),
          );
        },
      ),
    );
  }
}
