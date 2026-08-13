/// 原生 chat 会话列表页：项目分组列表 + 新建会话 + 命令面板（搜索/命令）。
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/network/models/models.dart';
import '../../core/platform/storage/storage.dart';
import '../../app/module_switcher.dart';
import '../../app/server_settings_button.dart';
import '../work/providers/data_sources.dart';
import 'chat_session_page.dart';
import 'providers/chat_session_provider.dart';
import 'providers/models_provider.dart';
import 'providers/sessions_provider.dart';

class SessionListPage extends ConsumerStatefulWidget {
  const SessionListPage({super.key, this.currentModule = 0, this.onSwitchModule});

  /// 当前模块（0 = 聊天），传给 AppBar title 的模块切换器。
  final int currentModule;
  final ValueChanged<int>? onSwitchModule;

  @override
  ConsumerState<SessionListPage> createState() => _SessionListPageState();
}

class _SessionListPageState extends ConsumerState<SessionListPage> {
  @override
  void initState() {
    super.initState();
    // 连接 chat-ws（订阅 sessions_list）+ 首拉列表。
    WidgetsBinding.instance.addPostFrameCallback((_) {
      ref.read(chatSessionProvider.notifier).connect();
      ref.read(chatSessionsProvider.notifier).watch();
    });
  }

  @override
  Widget build(BuildContext context) {
    final sessions = ref.watch(chatSessionsProvider);
    return Scaffold(
      appBar: AppBar(
        title: widget.onSwitchModule != null
            ? ModuleSwitcher(current: widget.currentModule, onSwitch: widget.onSwitchModule!)
            : const Text('聊天'),
        actions: [
          IconButton(
            icon: const Icon(Icons.search),
            tooltip: '搜索',
            onPressed: () => _openCommandPalette(context, initialTab: 'search'),
          ),
          IconButton(
            icon: const Icon(Icons.keyboard_command_key_outlined),
            tooltip: '命令面板',
            onPressed: () => _openCommandPalette(context),
          ),
          const ServerSettingsButton(),
        ],
      ),
      body: sessions.when(
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (e, _) => Center(child: Text('加载失败：$e')),
        data: (groups) {
          if (groups.isEmpty) {
            return Center(
              child: Text('暂无会话，点击右下角开始', style: Theme.of(context).textTheme.bodySmall),
            );
          }
          final active = [for (final g in groups) if (!g.archived) g];
          final archived = [for (final g in groups) if (g.archived) g];
          final hasArchived = archived.any((g) => g.sessions.isNotEmpty);
          return ListView(
            padding: const EdgeInsets.only(bottom: 96),
            children: [
              for (final g in active)
                if (g.sessions.isNotEmpty) _ProjectSection(group: g),
              if (hasArchived) _ArchivedSection(groups: archived),
            ],
          );
        },
      ),
      floatingActionButton: FloatingActionButton(
        onPressed: _createSession,
        tooltip: '新建会话',
        child: const Icon(Icons.add),
      ),
    );
  }

  void _createSession() {
    // 项目工作目录选择 + 名称输入 + 模型。
    showModalBottomSheet<_NewSessionSpec>(
      context: context,
      isScrollControlled: true,
      builder: (ctx) => const _NewSessionSheet(),
    ).then((spec) {
      if (spec == null || !mounted) return;
      // 新建会话的模型种子写入当前模型选择（详情页随 prompt 生效）。
      if (spec.modelId != null && spec.modelId!.isNotEmpty) {
        ref.read(currentChatModelProvider.notifier).state =
            ModelInfo(id: spec.modelId!, provider: spec.modelProvider ?? '');
      }
      Navigator.of(context).push(MaterialPageRoute<void>(
        builder: (_) => ChatSessionPage(
          instanceId: '',
          title: spec.name.isEmpty ? '新会话' : spec.name,
          cwd: spec.cwd,
        ),
      ));
    });
  }

  Future<void> _openCommandPalette(BuildContext context, {String initialTab = 'actions'}) {
    return showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      builder: (ctx) => CommandPaletteSheet(initialTab: initialTab),
    );
  }
}

class _GroupHeader extends StatelessWidget {
  const _GroupHeader({required this.name});

  final String name;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 14, 16, 4),
      child: Text(
        name,
        style: Theme.of(context)
            .textTheme
            .titleMedium
            ?.copyWith(color: scheme.primary, fontWeight: FontWeight.w600),
      ),
    );
  }
}

/// 单个 project 分组：可折叠（默认展开），标题为 project 名；折叠状态持久化。
class _ProjectSection extends ConsumerStatefulWidget {
  const _ProjectSection({required this.group});

  final ProjectGroup group;

  @override
  ConsumerState<_ProjectSection> createState() => _ProjectSectionState();
}

class _ProjectSectionState extends ConsumerState<_ProjectSection> {
  final _controller = ExpansibleController();
  // 无记录默认展开；'1' 展开，'0' 折叠。
  bool _expanded = true;

  String get _storageKey {
    final g = widget.group;
    return 'chat.collapsed.${g.id.isEmpty ? g.path : g.id}';
  }

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    final v = await ref.read(storageServiceProvider).read(_storageKey);
    if (!mounted) return;
    setState(() => _expanded = v != '0');
    // 同步 controller（异步回调中调用是安全的，不在 build 内）。
    if (_expanded) {
      _controller.expand();
    } else {
      _controller.collapse();
    }
  }

  void _onExpansionChanged(bool expanded) {
    setState(() => _expanded = expanded);
    ref.read(storageServiceProvider).write(_storageKey, expanded ? '1' : '0');
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return ExpansionTile(
      controller: _controller,
      shape: const Border(),
      collapsedShape: const Border(),
      initiallyExpanded: _expanded,
      onExpansionChanged: _onExpansionChanged,
      title: Text(
        widget.group.name.isEmpty ? widget.group.id : widget.group.name,
        style: Theme.of(context)
            .textTheme
            .titleMedium
            ?.copyWith(color: scheme.primary, fontWeight: FontWeight.w600),
      ),
      children: [
        for (final s in widget.group.sessions)
          _SessionTile(group: widget.group, session: s),
      ],
    );
  }
}

/// 已归档项目分区（独立于普通分组，默认折叠）；展开状态持久化。
class _ArchivedSection extends ConsumerStatefulWidget {
  const _ArchivedSection({required this.groups});

  final List<ProjectGroup> groups;

  @override
  ConsumerState<_ArchivedSection> createState() => _ArchivedSectionState();
}

class _ArchivedSectionState extends ConsumerState<_ArchivedSection> {
  static const _storageKey = 'chat.collapsed.archived';
  final _controller = ExpansibleController();
  // 无记录默认折叠；'1' 展开，'0' 折叠。
  bool _expanded = false;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    final v = await ref.read(storageServiceProvider).read(_storageKey);
    if (!mounted) return;
    setState(() => _expanded = v == '1');
    // 同步 controller（异步回调中调用是安全的，不在 build 内）。
    if (_expanded) {
      _controller.expand();
    } else {
      _controller.collapse();
    }
  }

  void _onExpansionChanged(bool expanded) {
    setState(() => _expanded = expanded);
    ref.read(storageServiceProvider).write(_storageKey, expanded ? '1' : '0');
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return ExpansionTile(
      controller: _controller,
      shape: const Border(),
      collapsedShape: const Border(),
      initiallyExpanded: _expanded,
      onExpansionChanged: _onExpansionChanged,
      leading: Icon(Icons.archive_outlined, size: 20, color: scheme.outline),
      title: Text(
        '已归档',
        style: Theme.of(context)
            .textTheme
            .labelMedium
            ?.copyWith(color: scheme.outline),
      ),
      children: [
        for (final g in widget.groups)
          if (g.sessions.isNotEmpty) ...[
            _GroupHeader(name: g.name.isEmpty ? g.id : g.name),
            for (final s in g.sessions) _SessionTile(group: g, session: s),
          ],
      ],
    );
  }
}

class _SessionTile extends ConsumerWidget {
  const _SessionTile({required this.group, required this.session});

  final ProjectGroup group;
  final SessionInfo session;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final scheme = Theme.of(context).colorScheme;
    return ListTile(
      leading: Icon(
        session.busy ? Icons.sync : Icons.chat_bubble_outline,
        size: 20,
        color: session.busy ? scheme.primary : scheme.outline,
      ),
      title: Text(
        session.label,
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        style: Theme.of(context).textTheme.bodyMedium,
      ),
      subtitle: Text(
        session.preview.isEmpty ? _timeText(session) : session.preview,
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        style: Theme.of(context).textTheme.bodySmall?.copyWith(color: scheme.outline),
      ),
      trailing: IconButton(
        icon: Icon(
          session.pinned == 1 ? Icons.star : Icons.star_border,
          size: 18,
          color: session.pinned == 1 ? scheme.tertiary : scheme.outlineVariant,
        ),
        tooltip: session.pinned == 1 ? '取消置顶' : '置顶',
        onPressed: () async {
          try {
            await ref.read(apiClientProvider).pinChatSession(session.runtimeId, session.pinned == 1 ? 0 : 1);
            ref.read(chatSessionsProvider.notifier).fetch();
          } catch (_) {}
        },
      ),
      onTap: () => Navigator.of(context).push(MaterialPageRoute<void>(
        builder: (_) => ChatSessionPage(
          instanceId: session.runtimeId,
          title: session.label,
          cwd: session.cwd,
        ),
      )),
      onLongPress: () => _showActions(context, ref),
    );
  }

  String _timeText(SessionInfo s) {
    if (s.updatedAt <= 0) return '暂无消息';
    final t = DateTime.fromMillisecondsSinceEpoch(s.updatedAt * 1000);
    final now = DateTime.now();
    final sameDay = t.year == now.year && t.month == now.month && t.day == now.day;
    String two(int n) => n.toString().padLeft(2, '0');
    final hm = '${two(t.hour)}:${two(t.minute)}';
    return sameDay ? '今天 $hm' : '${two(t.month)}-${two(t.day)} $hm';
  }

  void _showActions(BuildContext context, WidgetRef ref) {
    showModalBottomSheet<void>(
      context: context,
      builder: (ctx) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            ListTile(
              title: Text(session.label, maxLines: 1, overflow: TextOverflow.ellipsis),
              subtitle: Text(session.runtimeId),
            ),
            const Divider(height: 1),
            ListTile(
              leading: const Icon(Icons.edit_outlined),
              title: const Text('重命名'),
              onTap: () {
                Navigator.pop(ctx);
                _rename(context, ref);
              },
            ),
            ListTile(
              leading: const Icon(Icons.delete_outline),
              title: const Text('删除'),
              onTap: () {
                Navigator.pop(ctx);
                _delete(context, ref);
              },
            ),
          ],
        ),
      ),
    );
  }

  Future<void> _rename(BuildContext context, WidgetRef ref) async {
    final controller = TextEditingController(text: session.label);
    final name = await showDialog<String>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('重命名会话'),
        content: TextField(controller: controller, autofocus: true),
        actions: [
          TextButton(onPressed: () => Navigator.pop(ctx), child: const Text('取消')),
          FilledButton(onPressed: () => Navigator.pop(ctx, controller.text.trim()), child: const Text('保存')),
        ],
      ),
    );
    if (name == null || name.isEmpty) return;
    try {
      await ref.read(apiClientProvider).renameChatSession(session.filePath, name);
      ref.read(chatSessionsProvider.notifier).fetch();
    } catch (_) {}
  }

  Future<void> _delete(BuildContext context, WidgetRef ref) async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('删除会话'),
        content: Text('确定删除「${session.label}」？此操作不可恢复。'),
        actions: [
          TextButton(onPressed: () => Navigator.pop(ctx, false), child: const Text('取消')),
          FilledButton(
            style: FilledButton.styleFrom(backgroundColor: Theme.of(ctx).colorScheme.error),
            onPressed: () => Navigator.pop(ctx, true),
            child: const Text('删除'),
          ),
        ],
      ),
    );
    if (ok != true) return;
    try {
      await ref.read(apiClientProvider).deleteChatSession(session.runtimeId);
      ref.read(chatSessionsProvider.notifier).fetch();
    } catch (_) {}
  }
}

// ─── 新建会话 ──────────────────────────────────────────────────────────────

class _NewSessionSpec {
  const _NewSessionSpec({
    required this.cwd,
    required this.name,
    this.modelId,
    this.modelProvider,
  });

  final String cwd;
  final String name;
  final String? modelId;
  final String? modelProvider;
}

class _NewSessionSheet extends ConsumerStatefulWidget {
  const _NewSessionSheet();

  @override
  ConsumerState<_NewSessionSheet> createState() => _NewSessionSheetState();
}

class _NewSessionSheetState extends ConsumerState<_NewSessionSheet> {
  final _name = TextEditingController();
  String _cwd = '';
  String? _modelId;
  String? _modelProvider;

  @override
  void initState() {
    super.initState();
    final groups = ref.read(chatSessionsProvider).valueOrNull ?? const <ProjectGroup>[];
    if (groups.isNotEmpty) _cwd = groups.first.path;
  }

  @override
  void dispose() {
    _name.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final groups = ref.watch(chatSessionsProvider).valueOrNull ?? const <ProjectGroup>[];
    final models = ref.watch(chatModelsProvider).valueOrNull ?? const <ModelInfo>[];
    return Padding(
      padding: EdgeInsets.only(
        left: 16,
        right: 16,
        top: 16,
        bottom: MediaQuery.of(context).viewInsets.bottom + 16,
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('新建会话', style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 12),
          TextField(
            controller: _name,
            decoration: const InputDecoration(
              labelText: '名称（可选）',
              isDense: true,
              border: OutlineInputBorder(),
            ),
          ),
          const SizedBox(height: 12),
          if (groups.isNotEmpty)
            DropdownButtonFormField<String>(
              initialValue: _cwd,
              decoration: const InputDecoration(
                labelText: '工作目录',
                isDense: true,
                border: OutlineInputBorder(),
              ),
              items: [
                for (final g in groups)
                  DropdownMenuItem(value: g.path, child: Text(g.name.isEmpty ? g.id : g.name)),
              ],
              onChanged: (v) => setState(() => _cwd = v ?? ''),
            ),
          if (models.isNotEmpty) ...[
            const SizedBox(height: 12),
            DropdownButtonFormField<String>(
              initialValue: _modelId,
              decoration: const InputDecoration(
                labelText: '模型',
                isDense: true,
                border: OutlineInputBorder(),
              ),
              items: [
                const DropdownMenuItem(value: null, child: Text('默认')),
                for (final m in models)
                  DropdownMenuItem(value: m.id, child: Text(m.display)),
              ],
              onChanged: (v) {
                ModelInfo? m;
                for (final e in models) {
                  if (e.id == v) m = e;
                }
                setState(() {
                  _modelId = v;
                  _modelProvider = m?.provider;
                });
              },
            ),
          ],
          const SizedBox(height: 16),
          SizedBox(
            width: double.infinity,
            child: FilledButton(
              onPressed: () => Navigator.pop(
                context,
                _NewSessionSpec(
                  cwd: _cwd,
                  name: _name.text.trim(),
                  modelId: _modelId,
                  modelProvider: _modelProvider,
                ),
              ),
              child: const Text('创建'),
            ),
          ),
        ],
      ),
    );
  }
}

// ─── 命令面板（搜索 / 斜杠命令 / 会话切换）───────────────────────────────

class CommandPaletteSheet extends ConsumerStatefulWidget {
  const CommandPaletteSheet({super.key, this.initialTab = 'actions'});

  final String initialTab;

  @override
  ConsumerState<CommandPaletteSheet> createState() => _CommandPaletteSheetState();
}

class _CommandPaletteSheetState extends ConsumerState<CommandPaletteSheet> {
  late String _tab;
  final _search = TextEditingController();
  List<SearchHit> _hits = [];
  bool _searching = false;

  @override
  void initState() {
    super.initState();
    _tab = widget.initialTab;
  }

  @override
  void dispose() {
    _search.dispose();
    super.dispose();
  }

  Future<void> _doSearch(String q) async {
    if (q.trim().length < 3) {
      setState(() => _hits = []);
      return;
    }
    setState(() => _searching = true);
    try {
      final hits = await ref.read(apiClientProvider).searchChat(q.trim());
      if (!mounted) return;
      setState(() => _hits = hits);
    } catch (_) {
      if (mounted) setState(() => _hits = []);
    } finally {
      if (mounted) setState(() => _searching = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final session = ref.watch(chatSessionProvider);
    final groups = ref.watch(chatSessionsProvider).valueOrNull ?? const <ProjectGroup>[];
    return Padding(
      padding: EdgeInsets.only(bottom: MediaQuery.of(context).viewInsets.bottom),
      child: DraggableScrollableSheet(
        expand: false,
        initialChildSize: 0.8,
        minChildSize: 0.4,
        maxChildSize: 0.95,
        builder: (context, scrollController) => Column(
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 12, 16, 0),
              child: TextField(
                controller: _search,
                autofocus: true,
                onChanged: _doSearch,
                decoration: InputDecoration(
                  hintText: '搜索会话内容（≥3 字符）…',
                  isDense: true,
                  border: OutlineInputBorder(borderRadius: BorderRadius.circular(12)),
                  suffixIcon: _searching
                      ? const Padding(
                          padding: EdgeInsets.all(12),
                          child: SizedBox(
                            width: 16,
                            height: 16,
                            child: CircularProgressIndicator(strokeWidth: 2),
                          ),
                        )
                      : null,
                ),
              ),
            ),
            const SizedBox(height: 8),
            Row(
              children: [
                _tabChip('actions', '操作'),
                _tabChip('slash', '命令'),
                _tabChip('search', '搜索'),
                _tabChip('sessions', '会话'),
              ],
            ),
            Expanded(
              child: _search.text.trim().length >= 3 && _tab == 'search'
                  ? ListView(
                      controller: scrollController,
                      children: [
                        for (final h in _hits)
                          ListTile(
                            dense: true,
                            leading: const Icon(Icons.search, size: 18),
                            title: Text(h.label, maxLines: 1, overflow: TextOverflow.ellipsis),
                            subtitle: Text(h.snippet, maxLines: 2, overflow: TextOverflow.ellipsis),
                            onTap: () {
                              Navigator.pop(context);
                              _jumpToSession(h.sessionId);
                            },
                          ),
                      ],
                    )
                  : ListView(
                      controller: scrollController,
                      children: [
                        if (_tab == 'actions') ...[
                          ListTile(
                            leading: const Icon(Icons.add),
                            title: const Text('新建会话'),
                            onTap: () {
                              Navigator.pop(context);
                              ref.read(chatSessionsProvider.notifier).fetch();
                            },
                          ),
                        ],
                        if (_tab == 'slash')
                          for (final c in session.slashCommands)
                            ListTile(
                              dense: true,
                              leading: Text('/${c.name}',
                                  style: TextStyle(
                                    color: Theme.of(context).colorScheme.primary,
                                    fontFamily: 'monospace',
                                  )),
                              title: Text(c.description, maxLines: 1, overflow: TextOverflow.ellipsis),
                              trailing: Text(c.source,
                                  style: Theme.of(context).textTheme.labelSmall),
                              onTap: () => Navigator.pop(context),
                            ),
                        if (_tab == 'sessions')
                          for (final g in groups)
                            for (final s in g.sessions)
                              ListTile(
                                dense: true,
                                leading: Icon(
                                  s.busy ? Icons.sync : Icons.chat_bubble_outline,
                                  size: 18,
                                ),
                                title: Text(s.label, maxLines: 1, overflow: TextOverflow.ellipsis),
                                subtitle: Text(s.preview, maxLines: 1, overflow: TextOverflow.ellipsis),
                                onTap: () {
                                  Navigator.pop(context);
                                  Navigator.of(context).push(MaterialPageRoute<void>(
                                    builder: (_) => ChatSessionPage(
                                      instanceId: s.runtimeId,
                                      title: s.label,
                                      cwd: s.cwd,
                                    ),
                                  ));
                                },
                              ),
                      ],
                    ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _tabChip(String id, String label) {
    final scheme = Theme.of(context).colorScheme;
    final active = _tab == id;
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 4),
      child: ChoiceChip(
        label: Text(label, style: Theme.of(context).textTheme.labelSmall),
        selected: active,
        onSelected: (_) => setState(() => _tab = id),
        visualDensity: VisualDensity.compact,
        selectedColor: scheme.primaryContainer,
      ),
    );
  }

  void _jumpToSession(String sessionId) {
    Navigator.of(context).push(MaterialPageRoute<void>(
      builder: (_) => ChatSessionPage(
        instanceId: sessionId,
        title: '搜索结果',
      ),
    ));
  }
}
