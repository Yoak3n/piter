/// workspacesProvider 状态流转测试（Riverpod Notifier 核心逻辑）。
///
/// 不含任何样例假数据：仅用无内容的测试桩覆盖"空列表 / 加载失败"两个分支
/// （flutter_test 环境无网络，不注入桩则列表页永远走错误分支）。
library;

import 'dart:typed_data';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:piter_work/core/network/api_client.dart';
import 'package:piter_work/core/network/models/models.dart';
import 'package:piter_work/features/work/providers/data_sources.dart';
import 'package:piter_work/features/work/providers/workspaces_provider.dart';

/// 空数据桩：listWorkspaces 返回空列表，其余方法不触发。
class _EmptyApiClient implements ApiClient {
  @override
  Future<List<Workspace>> listWorkspaces() async => const [];

  @override
  Future<Workspace> createWorkspace(String name) =>
      throw UnimplementedError('not used');

  @override
  Future<void> deleteWorkspace(String id) =>
      throw UnimplementedError('not used');

  @override
  Future<Workspace> getWorkspace(String id) =>
      throw UnimplementedError('not used');

  @override
  Future<WorkspaceFiles> getFiles(String id) =>
      throw UnimplementedError('not used');

  @override
  Future<List<ArtifactTurn>> getArtifacts(String id, {int? sinceTurn}) =>
      throw UnimplementedError('not used');

  @override
  Future<List<Artifact>> getDeliverables(String id) =>
      throw UnimplementedError('not used');

  @override
  Future<UploadResult> uploadFiles(String id, List<UploadFile> files) =>
      throw UnimplementedError('not used');

  @override
  Future<Uint8List> downloadFile(String id, String path) =>
      throw UnimplementedError('not used');

  @override
  Future<Workspace> setWorkspaceMode(String id, WorkspaceMode mode) =>
      throw UnimplementedError('not used');

  @override
  Future<FileEntry> markDeliverable(String id, String path, bool deliverable) =>
      throw UnimplementedError('not used');

  @override
  Future<List<ProjectGroup>> listChatSessions() async => const [];

  @override
  Future<void> deleteChatSession(String instanceId) =>
      throw UnimplementedError('not used');

  @override
  Future<void> renameChatSession(String filePath, String name) =>
      throw UnimplementedError('not used');

  @override
  Future<void> pinChatSession(String id, int pinned) =>
      throw UnimplementedError('not used');

  @override
  Future<PiSettings> piSettings() => throw UnimplementedError('not used');

  @override
  Future<List<ModelInfo>> modelCatalog() async => const [];

  @override
  Future<List<ModelInfo>> rpcAvailableModels() async => const [];

  @override
  Future<List<SearchHit>> searchChat(String q, {int limit = 50}) async => const [];

  @override
  Future<BudgetStatus> budgetStatus() => throw UnimplementedError('not used');
}

/// 失败桩：listWorkspaces 抛统一 API 错误。
class _ThrowingApiClient implements ApiClient {
  @override
  Future<List<Workspace>> listWorkspaces() async =>
      throw const ApiException('query_failed', '加载失败');

  @override
  Future<Workspace> createWorkspace(String name) =>
      throw UnimplementedError('not used');

  @override
  Future<void> deleteWorkspace(String id) =>
      throw UnimplementedError('not used');

  @override
  Future<Workspace> getWorkspace(String id) =>
      throw UnimplementedError('not used');

  @override
  Future<WorkspaceFiles> getFiles(String id) =>
      throw UnimplementedError('not used');

  @override
  Future<List<ArtifactTurn>> getArtifacts(String id, {int? sinceTurn}) =>
      throw UnimplementedError('not used');

  @override
  Future<List<Artifact>> getDeliverables(String id) =>
      throw UnimplementedError('not used');

  @override
  Future<UploadResult> uploadFiles(String id, List<UploadFile> files) =>
      throw UnimplementedError('not used');

  @override
  Future<Uint8List> downloadFile(String id, String path) =>
      throw UnimplementedError('not used');

  @override
  Future<Workspace> setWorkspaceMode(String id, WorkspaceMode mode) =>
      throw UnimplementedError('not used');

  @override
  Future<FileEntry> markDeliverable(String id, String path, bool deliverable) =>
      throw UnimplementedError('not used');

  @override
  Future<List<ProjectGroup>> listChatSessions() async =>
      throw const ApiException('query_failed', '加载失败');

  @override
  Future<void> deleteChatSession(String instanceId) =>
      throw UnimplementedError('not used');

  @override
  Future<void> renameChatSession(String filePath, String name) =>
      throw UnimplementedError('not used');

  @override
  Future<void> pinChatSession(String id, int pinned) =>
      throw UnimplementedError('not used');

  @override
  Future<PiSettings> piSettings() => throw UnimplementedError('not used');

  @override
  Future<List<ModelInfo>> modelCatalog() async => const [];

  @override
  Future<List<ModelInfo>> rpcAvailableModels() async => const [];

  @override
  Future<List<SearchHit>> searchChat(String q, {int limit = 50}) async => const [];

  @override
  Future<BudgetStatus> budgetStatus() => throw UnimplementedError('not used');
}

void main() {
  test('初始加载：空列表（无样例数据）', () async {
    final container = ProviderContainer(overrides: [
      apiClientProvider.overrideWithValue(_EmptyApiClient()),
    ]);
    addTearDown(container.dispose);

    final list = await container.read(workspacesProvider.future);
    expect(list, isEmpty);
  });

  test('加载失败 → 抛 ApiException（错误分支）', () async {
    final container = ProviderContainer(overrides: [
      apiClientProvider.overrideWithValue(_ThrowingApiClient()),
    ]);
    addTearDown(container.dispose);

    await expectLater(
      container.read(workspacesProvider.future),
      throwsA(isA<ApiException>()
          .having((e) => e.code, 'code', 'query_failed')
          .having((e) => e.message, 'message', '加载失败')),
    );
  });
}
