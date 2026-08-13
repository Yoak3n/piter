/// 原生 chat 会话列表：REST 兜底 + WS sessions_list 实时更新。
library;

import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../core/network/models/models.dart';
import '../../../shared/ws_events.dart';
import '../../work/providers/data_sources.dart';

final chatSessionsProvider =
    NotifierProvider<SessionsNotifier, AsyncValue<List<ProjectGroup>>>(
        SessionsNotifier.new);

class SessionsNotifier extends Notifier<AsyncValue<List<ProjectGroup>>> {
  StreamSubscription<dynamic>? _sub;

  @override
  AsyncValue<List<ProjectGroup>> build() {
    ref.onDispose(() {
      _sub?.cancel();
      _sub = null;
    });
    return const AsyncLoading();
  }

  /// 订阅 WS sessions_list + REST 首拉（由 ChatPage 打开时调用，幂等）。
  Future<void> watch() async {
    _sub ??= ref.read(chatWsClientProvider).events.listen((e) {
      if (e is SessionsListEvent) {
        final projects = e.projects
            .whereType<Map<String, dynamic>>()
            .map(ProjectGroup.fromJson)
            .toList();
        state = AsyncData(_filterWorkspace(projects));
      }
    });
    await fetch();
  }

  Future<void> fetch() async {
    try {
      final projects = await ref.read(apiClientProvider).listChatSessions();
      state = AsyncData(_filterWorkspace(projects));
    } catch (e, st) {
      state = AsyncError(e, st);
    }
  }

  /// chat 会话列表不展示 workspace 类型的项目（工作空间是 work 视图的范畴，
  /// 对齐 Vue chat 准备页的过滤语义）。
  static List<ProjectGroup> _filterWorkspace(List<ProjectGroup> groups) =>
      [for (final g in groups) if (g.projectType != 'workspace') g];
}
