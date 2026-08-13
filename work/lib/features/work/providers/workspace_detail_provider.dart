/// 工作空间详情状态（workspace + 文件树）。
library;

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../core/network/api_client.dart';
import '../../../core/network/models/models.dart';
import 'data_sources.dart';

/// 详情页聚合数据：工作空间 + 文件树。
class WorkspaceDetail {
  const WorkspaceDetail({required this.workspace, required this.files});

  final Workspace workspace;
  final WorkspaceFiles files;
}

final workspaceDetailProvider = AsyncNotifierProvider.family<WorkspaceDetailNotifier,
    WorkspaceDetail, String>(WorkspaceDetailNotifier.new);

class WorkspaceDetailNotifier extends FamilyAsyncNotifier<WorkspaceDetail, String> {
  @override
  Future<WorkspaceDetail> build(String arg) async {
    // watch 数据源：切换服务器（currentServerId 变化 → apiClientProvider 重建）
    // 时详情页自动重新加载，family 缓存也随之失效。
    final api = ref.watch(apiClientProvider);
    final ws = await api.getWorkspace(arg);
    final files = await api.getFiles(arg);
    return WorkspaceDetail(workspace: ws, files: files);
  }

  /// PUT /api/workspaces/:id/mode：切换写边界策略并更新本地 workspace。
  Future<void> setMode(WorkspaceMode mode) async {
    final api = ref.read(apiClientProvider);
    final updated = await api.setWorkspaceMode(arg, mode);
    final detail = state.value;
    if (detail == null) return;
    state = AsyncData(WorkspaceDetail(
      workspace: updated,
      files: detail.files,
    ));
  }

  /// POST /api/workspaces/:id/mark-deliverable，更新本地文件树。
  Future<void> markDeliverable(String path, bool deliverable) async {
    final api = ref.read(apiClientProvider);
    final updated = await api.markDeliverable(arg, path, deliverable);
    final detail = state.value;
    if (detail == null) return;
    final next = [
      for (final f in detail.files.files) f.path == path ? updated : f,
    ];
    state = AsyncData(WorkspaceDetail(
      workspace: detail.workspace.copyWith(fileCount: next.length),
      files: WorkspaceFiles(files: next, basePath: detail.files.basePath),
    ));
  }
}
