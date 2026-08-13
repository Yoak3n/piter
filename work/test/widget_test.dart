/// 冒烟测试：Web 壳（仅 work）启动 → 列表页渲染（空态，无样例数据）。
///
/// flutter_test 环境所有 HTTP 请求返回 400，因此用空数据桩 + 能力桩注入，
/// 仅断言页面骨架与空态视图；不含任何编造的工作空间内容。
library;

import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:piter_work/app/piter_app.dart';
import 'package:piter_work/core/network/api_client.dart';
import 'package:piter_work/core/network/models/models.dart';
import 'package:piter_work/features/connection/providers/capability_provider.dart';
import 'package:piter_work/features/work/providers/data_sources.dart';

/// 空数据桩：列表页仅加载，不做增删。
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

/// 能力桩：直接判定 work 可用（不走真实探测）。
class _FakeCapabilityNotifier extends ServerCapabilityNotifier {
  @override
  Future<ServerCapability> build() async =>
      const ServerCapability(reachable: true, workSupported: true);
}

void main() {
  testWidgets('Web 端启动后渲染工作空间列表（空态）', (tester) async {
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          apiClientProvider.overrideWithValue(_EmptyApiClient()),
          serverCapabilityProvider.overrideWith(_FakeCapabilityNotifier.new),
        ],
        child: const PiterApp(isWeb: true),
      ),
    );
    await tester.pump(); // 首帧
    await tester.pump(const Duration(milliseconds: 200)); // 等待数据源

    // AppBar 标题
    expect(find.text('工作空间'), findsOneWidget);
    // 空态视图（无样例数据）
    expect(find.text('还没有工作空间'), findsOneWidget);
    expect(find.text('新建工作空间'), findsOneWidget);
    // AppBar 新建按钮存在（空态按钮含 add 图标，故用 findsWidgets）
    expect(find.byIcon(Icons.add), findsWidgets);
  });
}
