/// ApiClient 抽象（对应 mock-contract §2 REST 契约）。
/// 真实实现为 HttpApiClient（dio），经 data_sources.dart 注入。
library;

import 'dart:typed_data';

import 'models/models.dart';

/// GET /api/workspaces/:id/files 响应：扁平文件列表 + 基准路径。
class WorkspaceFiles {
  const WorkspaceFiles({required this.files, required this.basePath});

  final List<FileEntry> files;
  final String basePath;

  factory WorkspaceFiles.fromJson(Map<String, dynamic> json) => WorkspaceFiles(
        files: (json['files'] as List<dynamic>? ?? const [])
            .map((e) => FileEntry.fromJson(e as Map<String, dynamic>))
            .toList(),
        basePath: json['basePath'] as String? ?? '',
      );
}

/// 上传的一个文件（原始字节 + 文件名，文件名即工作空间内相对路径）。
class UploadFile {
  const UploadFile({required this.name, required this.bytes});

  final String name;
  final List<int> bytes;
}

/// POST /api/workspaces/:id/upload 响应：成功落盘的相对路径 + 被拒列表。
class UploadResult {
  const UploadResult({required this.uploaded, required this.rejected});

  final List<String> uploaded;
  final List<({String path, String reason})> rejected;

  bool get hasAny => uploaded.isNotEmpty || rejected.isNotEmpty;

  factory UploadResult.fromJson(Map<String, dynamic> json) => UploadResult(
        uploaded: (json['uploaded'] as List<dynamic>? ?? const [])
            .whereType<String>()
            .toList(),
        rejected: (json['rejected'] as List<dynamic>? ?? const [])
            .whereType<Map<String, dynamic>>()
            .map((e) => (
                  path: e['path'] as String? ?? '',
                  reason: e['reason'] as String? ?? '',
                ))
            .toList(),
      );
}

/// 数据源接口。所有方法返回 Future，未来真实实现可同步替换。
abstract class ApiClient {
  /// GET /api/workspaces
  Future<List<Workspace>> listWorkspaces();

  /// POST /api/workspaces（创建 real_dir + 基线快照）
  Future<Workspace> createWorkspace(String name);

  /// DELETE /api/workspaces/:id
  Future<void> deleteWorkspace(String id);

  /// GET /api/workspaces/:id
  Future<Workspace> getWorkspace(String id);

  /// GET /api/workspaces/:id/files
  Future<WorkspaceFiles> getFiles(String id);

  /// GET /api/workspaces/:id/artifacts（按 turn 分组，新→旧）
  Future<List<ArtifactTurn>> getArtifacts(String id, {int? sinceTurn});

  /// GET /api/workspaces/:id/deliverables（仅 deliverable=true）
  Future<List<Artifact>> getDeliverables(String id);

  /// POST /api/workspaces/:id/upload（multipart，单文件 ≤50MB；拒绝 output/ 与穿越）
  Future<UploadResult> uploadFiles(String id, List<UploadFile> files);

  /// GET /api/workspaces/:id/download?path=`<rel>`（attachment 文件流）
  Future<Uint8List> downloadFile(String id, String path);

  /// PUT /api/workspaces/:id/mode（ask | allow | deny 写边界）
  Future<Workspace> setWorkspaceMode(String id, WorkspaceMode mode);

  /// POST /api/workspaces/:id/mark-deliverable
  Future<FileEntry> markDeliverable(String id, String path, bool deliverable);

  // ─── chat（原生 chat 0.3.2，对齐 Vue chat REST 契约）────────────────────

  /// GET /api/sessions → 会话树（项目分组）。
  Future<List<ProjectGroup>> listChatSessions();

  /// GET /api/delete-session?instanceId=
  Future<void> deleteChatSession(String instanceId);

  /// POST /api/sessions/rename（body {path: filePath, name}）
  Future<void> renameChatSession(String filePath, String name);

  /// POST /api/sessions/:id/pin（body {pinned: 0|1}，id = instanceId ?? id）
  Future<void> pinChatSession(String id, int pinned);

  /// GET /api/pi/settings（默认模型等）
  Future<PiSettings> piSettings();

  /// GET /api/pi/model-catalog（磁盘缓存模型目录，不起进程）
  Future<List<ModelInfo>> modelCatalog();

  /// POST /api/rpc `get_available_models`（一问一答，30s 超时）
  Future<List<ModelInfo>> rpcAvailableModels();

  /// GET /api/search?q=&limit=
  Future<List<SearchHit>> searchChat(String q, {int limit = 50});

  /// GET /api/budget/status（预算用量/档位）
  Future<BudgetStatus> budgetStatus();
}

/// 统一 API 错误（对齐 REST 错误格式 `{"success":false,"error":"<code>","message":…}`）。
class ApiException implements Exception {
  const ApiException(this.code, this.message);

  final String code;
  final String message;

  @override
  String toString() => 'ApiException($code): $message';
}
