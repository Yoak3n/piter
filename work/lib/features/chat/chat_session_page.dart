/// 原生 chat 会话详情页：消息流（Markdown/思考/工具/扩展卡片）+ 输入区。
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/network/models/models.dart';
import 'providers/chat_session.dart';
import 'providers/chat_session_provider.dart';
import 'widgets/chat_composer.dart';
import 'widgets/chat_entry_view.dart';
import 'widgets/model_selector.dart';

class ChatSessionPage extends ConsumerStatefulWidget {
  const ChatSessionPage({
    super.key,
    required this.instanceId,
    this.title = '',
    this.cwd = '',
  });

  /// 会话运行时 id（打开已有会话）或空串（新建后回填）。
  final String instanceId;
  final String title;
  final String cwd;

  @override
  ConsumerState<ChatSessionPage> createState() => _ChatSessionPageState();
}

class _ChatSessionPageState extends ConsumerState<ChatSessionPage> {
  final _scroll = ScrollController();
  late String _title;
  bool _isAtBottom = true;

  @override
  void initState() {
    super.initState();
    _title = widget.title;
    // 滚动监听：离开底部 → 显示“直达底部”FAB；回到底部 → 隐藏。
    _scroll.addListener(_onScroll);
    // 进入即连接 + 打开会话（新建时先建会话再等 instanceId 回填）。
    WidgetsBinding.instance.addPostFrameCallback((_) {
      final notifier = ref.read(chatSessionProvider.notifier);
      notifier.connect().then((_) {
        if (widget.instanceId.isEmpty) {
          notifier.newSession(cwd: widget.cwd.isEmpty ? null : widget.cwd, name: _title);
        } else {
          notifier.switchSession(widget.instanceId);
        }
        // 打开（或切换）会话后：无论历史消息多长，直接跳到底部。
        WidgetsBinding.instance.addPostFrameCallback((_) => _jumpToBottom());
      });
    });
  }

  void _onScroll() {
    if (!_scroll.hasClients) return;
    final atBottom =
        _scroll.position.pixels >= _scroll.position.maxScrollExtent - 120;
    if (atBottom != _isAtBottom) {
      setState(() => _isAtBottom = atBottom);
    }
  }

  void _jumpToBottom() {
    if (_scroll.hasClients) {
      _scroll.jumpTo(_scroll.position.maxScrollExtent);
    }
    _isAtBottom = true;
  }

  @override
  void dispose() {
    _scroll.removeListener(_onScroll);
    // 离开会话：通知后端移除订阅者；连接保留（会话列表页继续用）。
    ref.read(chatSessionProvider.notifier).deactivate();
    _scroll.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final session = ref.watch(chatSessionProvider);
    final pendingExt = _pendingExtCards(session);
    // 新条目到达时：若用户贴近底部则自动跟随滚动（流式输出）；
    // 上滑阅读历史时暂停跟随（与 web 版 sticky 语义一致）。
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_scroll.hasClients &&
          _scroll.position.pixels >= _scroll.position.maxScrollExtent - 120) {
        _scroll.jumpTo(_scroll.position.maxScrollExtent);
      }
    });

    return Scaffold(
      appBar: AppBar(
        title: Text(_title.isEmpty ? '会话' : _title),
        actions: const [ModelSelector()],
      ),
      body: Column(
        children: [
          Expanded(
            child: Stack(
              children: [
                session.entries.isEmpty
                    ? Center(
                        child: Text(
                          session.connected ? '发送一条消息开始对话' : '连接中…',
                          style: Theme.of(context).textTheme.bodySmall,
                        ),
                      )
                    : ListView.builder(
                        controller: _scroll,
                        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                        itemCount: session.entries.length,
                        itemBuilder: (context, i) => ChatEntryView(
                          entry: session.entries[i],
                          onExtRespond: (id, method, value) => ref
                              .read(chatSessionProvider.notifier)
                              .sendExtResponse(id, value: value),
                        ),
                      ),
                // 直达底部浮动按钮（上滑离开底部时显示）。
                if (!_isAtBottom)
                  Positioned(
                    right: 16,
                    bottom: 12,
                    child: FloatingActionButton.small(
                      heroTag: 'chat-jump-bottom',
                      onPressed: _jumpToBottom,
                      tooltip: '回到底部',
                      child: const Icon(Icons.keyboard_arrow_down),
                    ),
                  ),
              ],
            ),
          ),
          if (pendingExt.isNotEmpty) _ExtDock(cards: pendingExt),
          ChatComposer(cwd: widget.cwd),
        ],
      ),
    );
  }

  List<ChatExtUi> _pendingExtCards(ChatSessionState session) =>
      session.entries
          .whereType<ChatExtEntry>()
          .map((e) => e.ui)
          .where((u) => !u.answered)
          .take(2)
          .toList();
}

/// 未应答扩展卡片 dock（Composer 上方，最多 2 张）。
class _ExtDock extends StatelessWidget {
  const _ExtDock({required this.cards});

  final List<ChatExtUi> cards;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Container(
      width: double.infinity,
      color: scheme.surfaceContainerLowest,
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            '待处理请求',
            style: Theme.of(context).textTheme.labelSmall?.copyWith(color: scheme.primary),
          ),
          for (final c in cards)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 2),
              child: Row(
                children: [
                  Icon(Icons.extension_outlined, size: 14, color: scheme.primary),
                  const SizedBox(width: 6),
                  Expanded(
                    child: Text(
                      c.title,
                      style: Theme.of(context).textTheme.bodySmall,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                ],
              ),
            ),
        ],
      ),
    );
  }
}
