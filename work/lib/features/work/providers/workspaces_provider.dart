/// 工作空间列表状态。
library;

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../core/network/models/models.dart';
import 'data_sources.dart';

final workspacesProvider =
    AsyncNotifierProvider<WorkspacesNotifier, List<Workspace>>(WorkspacesNotifier.new);

class WorkspacesNotifier extends AsyncNotifier<List<Workspace>> {
  @override
  Future<List<Workspace>> build() => ref.watch(apiClientProvider).listWorkspaces();

  /// POST /api/workspaces（新建后追加本地列表）。
  Future<Workspace> createWorkspace(String name) async {
    final ws = await ref.read(apiClientProvider).createWorkspace(name);
    state = AsyncData([...?state.value, ws]);
    return ws;
  }

  /// DELETE /api/workspaces/:id。
  Future<void> deleteWorkspace(String id) async {
    await ref.read(apiClientProvider).deleteWorkspace(id);
    state = AsyncData([...?state.value]..removeWhere((w) => w.id == id));
  }
}
