/// 详情页·消息流面板：渲染时间线条目（消息 / 工具块 / 写阻断），流式自动滚底。
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../providers/work_session.dart';
import '../providers/work_session_provider.dart';
import '../../chat/widgets/chat_entry_view.dart';
import '../widgets/message_bubble.dart';
import '../widgets/tool_block.dart';
import '../widgets/write_block_card.dart';

class MessagePanel extends ConsumerStatefulWidget {
  const MessagePanel({super.key, required this.workspaceId});

  final String workspaceId;

  @override
  ConsumerState<MessagePanel> createState() => _MessagePanelState();
}

class _MessagePanelState extends ConsumerState<MessagePanel> {
  final _scroll = ScrollController();
  final _input = TextEditingController();

  void _send() {
    final text = _input.text.trim();
    if (text.isEmpty) return;
    ref.read(workSessionProvider.notifier).sendPrompt(text);
    _input.clear();
  }

  @override
  void initState() {
    super.initState();
    // 进入详情页即连接 WS（gateway /work-ws 事件流驱动消息区）。
    WidgetsBinding.instance.addPostFrameCallback((_) {
      ref.read(workSessionProvider.notifier).startSession(widget.workspaceId);
    });
  }

  @override
  void dispose() {
    _scroll.dispose();
    _input.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final session = ref.watch(workSessionProvider);
    // 新条目到达时滚动到底部（模拟实时消息推送）。
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_scroll.hasClients) {
        _scroll.jumpTo(_scroll.position.maxScrollExtent);
      }
    });

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 12, 16, 4),
          child: Row(
            children: [
              Text('消息', style: Theme.of(context).textTheme.labelLarge),
              const Spacer(),
              if (session.connected)
                Row(
                  children: [
                    Icon(Icons.circle, size: 8, color: Theme.of(context).colorScheme.tertiary),
                    const SizedBox(width: 4),
                    Text(
                      '已连接',
                      style: Theme.of(context).textTheme.labelSmall,
                    ),
                  ],
                )
              else if (session.reconnectFailed)
                Row(
                  children: [
                    Icon(Icons.error_outline, size: 14, color: Theme.of(context).colorScheme.error),
                    const SizedBox(width: 4),
                    Text(
                      '连接失败',
                      style: Theme.of(context)
                          .textTheme
                          .labelSmall
                          ?.copyWith(color: Theme.of(context).colorScheme.error),
                    ),
                  ],
                )
              else
                Row(
                  children: [
                    Icon(Icons.circle, size: 8, color: Theme.of(context).colorScheme.error),
                    const SizedBox(width: 4),
                    Text(
                      '重连中…',
                      style: Theme.of(context)
                          .textTheme
                          .labelSmall
                          ?.copyWith(color: Theme.of(context).colorScheme.error),
                    ),
                  ],
                ),
            ],
          ),
        ),
        const Divider(height: 1),
        Expanded(
          child: session.entries.isEmpty
              ? session.reconnectFailed
                  ? Center(
                      child: Column(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Text(
                            '连接失败，请确认服务端已启动',
                            style: Theme.of(context).textTheme.bodySmall,
                          ),
                          const SizedBox(height: 8),
                          OutlinedButton.icon(
                            onPressed: () =>
                                ref.read(workSessionProvider.notifier).retryConnect(),
                            icon: const Icon(Icons.refresh, size: 16),
                            label: const Text('重新连接'),
                          ),
                        ],
                      ),
                    )
                  : Center(
                      child: Text(
                        session.connected
                            ? '发送一条消息开始对话'
                            : '连接断开，正在重连…',
                        style: Theme.of(context).textTheme.bodySmall,
                      ),
                    )
              : ListView.builder(
                  controller: _scroll,
                  padding: const EdgeInsets.all(12),
                  itemCount: session.entries.length,
                  itemBuilder: (context, i) {
                    final entry = session.entries[i];
                    return switch (entry) {
                      MessageEntry(:final message, :final streaming) =>
                        MessageBubble(message: message, streaming: streaming),
                      ToolEntry(:final tool) => ToolBlock(tool: tool),
                      WriteBlockEntry() => WriteBlockCard(entry: entry),
                      WorkExtEntry(:final ui) => ExtensionCard(
                          ui: ui,
                          onRespond: (id, method, value) => ref
                              .read(workSessionProvider.notifier)
                              .sendExtResponse(id, value: value),
                        ),
                    };
                  },
                ),
        ),
        SafeArea(
          top: false,
          child: Padding(
            padding: const EdgeInsets.fromLTRB(12, 8, 12, 12),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                Expanded(
                  child: TextField(
                    controller: _input,
                    minLines: 1,
                    maxLines: 4,
                    textInputAction: TextInputAction.send,
                    onSubmitted: (_) => _send(),
                    decoration: InputDecoration(
                      hintText: session.instanceId.isEmpty
                          ? '会话建立中…'
                          : '输入消息，回车发送',
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
                const SizedBox(width: 8),
                IconButton.filled(
                  onPressed: (session.connected && session.instanceId.isNotEmpty)
                      ? _send
                      : null,
                  icon: const Icon(Icons.send),
                  tooltip: '发送',
                ),
              ],
            ),
          ),
        ),
      ],
    );
  }
}
