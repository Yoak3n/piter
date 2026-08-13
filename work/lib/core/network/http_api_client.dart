/// HttpApiClient：dio 实现的真实 ApiClient（对齐 mock-contract §2 REST 契约）。
///
/// 与 mock 实现共用 ApiClient 抽象——Provider 按当前服务器切换，
/// 后端实现 work API 后即真实可用；work 能力探测见 probe.dart。
library;

import 'dart:convert';
import 'dart:typed_data';

import 'package:dio/dio.dart';

import 'api_client.dart';
import 'models/models.dart';

class HttpApiClient implements ApiClient {
  HttpApiClient({required String baseUrl, Dio? dio})
      : baseUrl = baseUrl.endsWith('/') ? baseUrl.substring(0, baseUrl.length - 1) : baseUrl,
        _dio = dio ??
            Dio(BaseOptions(
              baseUrl: baseUrl.endsWith('/') ? baseUrl.substring(0, baseUrl.length - 1) : baseUrl,
              connectTimeout: const Duration(seconds: 8),
              receiveTimeout: const Duration(seconds: 15),
              headers: const {'Accept': 'application/json'},
              // Web 端浏览器自动携带 Cookie（同源）；App 端 LAN 鉴权 Cookie 头后续阶段接。
            ));

  final String baseUrl;
  final Dio _dio;

  @override
  Future<List<Workspace>> listWorkspaces() async {
    // 用 plain 响应避免 HTML fallback 触发 JSON 解码异常；
    // 非 JSON（HTML）视为 work 不支持。
    final resp = await _request(() => _dio.get<String>(
          '/api/workspaces',
          options: Options(responseType: ResponseType.plain),
        ));
    final raw = (resp.data ?? '').trim();
    if (!raw.startsWith('{')) {
      throw const ApiException('not_supported', '服务端不支持 work 模块（/api/workspaces 无响应）');
    }
    final decoded = jsonDecode(raw) as Map<String, dynamic>;
    final list = decoded['workspaces'];
    if (list is! List) {
      throw const ApiException('not_supported', '服务端不支持 work 模块（/api/workspaces 无响应）');
    }
    return list
        .map((e) => Workspace.fromJson(e as Map<String, dynamic>))
        .toList();
  }

  @override
  Future<Workspace> createWorkspace(String name) async {
    final json = await _postJson('/api/workspaces', {'name': name});
    final ws = json['workspace'];
    if (ws is! Map<String, dynamic>) throw ApiException('bad_response', '创建响应缺少 workspace');
    return Workspace.fromJson(ws);
  }

  @override
  Future<void> deleteWorkspace(String id) async {
    await _request(() => _dio.delete('/api/workspaces/$id'));
  }

  @override
  Future<Workspace> getWorkspace(String id) async {
    final json = await _getJson('/api/workspaces/$id');
    final ws = json['workspace'];
    if (ws is! Map<String, dynamic>) throw ApiException('bad_response', '响应缺少 workspace');
    return Workspace.fromJson(ws);
  }

  @override
  Future<WorkspaceFiles> getFiles(String id) async {
    final json = await _getJson('/api/workspaces/$id/files');
    return WorkspaceFiles.fromJson(json);
  }

  @override
  Future<List<ArtifactTurn>> getArtifacts(String id, {int? sinceTurn}) async {
    final query = sinceTurn == null ? '' : '?sinceTurn=$sinceTurn';
    final json = await _getJson('/api/workspaces/$id/artifacts$query');
    final turns = json['turns'];
    if (turns is! List) throw ApiException('bad_response', '响应缺少 turns');
    return turns
        .map((e) => ArtifactTurn.fromJson(e as Map<String, dynamic>))
        .toList();
  }

  @override
  Future<List<Artifact>> getDeliverables(String id) async {
    final json = await _getJson('/api/workspaces/$id/deliverables');
    final items = json['items'];
    if (items is! List) throw ApiException('bad_response', '响应缺少 items');
    return items.map((e) => Artifact.fromJson(e as Map<String, dynamic>)).toList();
  }

  @override
  Future<UploadResult> uploadFiles(String id, List<UploadFile> files) async {
    if (files.isEmpty) return const UploadResult(uploaded: [], rejected: []);
    final form = FormData.fromMap({
      'files': [
        for (final f in files) MultipartFile.fromBytes(f.bytes, filename: f.name),
      ],
    });
    final json = await _request(() => _dio.post<Map<String, dynamic>>(
          '/api/workspaces/$id/upload',
          data: form,
        ));
    return UploadResult.fromJson(json.data ?? const {});
  }

  @override
  Future<Uint8List> downloadFile(String id, String path) async {
    final resp = await _request(() => _dio.get<List<int>>(
          '/api/workspaces/$id/download',
          queryParameters: {'path': path},
          options: Options(responseType: ResponseType.bytes),
        ));
    return Uint8List.fromList(resp.data ?? const []);
  }

  @override
  Future<Workspace> setWorkspaceMode(String id, WorkspaceMode mode) async {
    final json = await _putJson('/api/workspaces/$id/mode', {'mode': mode.wire});
    final ws = json['workspace'];
    if (ws is! Map<String, dynamic>) throw ApiException('bad_response', '响应缺少 workspace');
    return Workspace.fromJson(ws);
  }

  @override
  Future<FileEntry> markDeliverable(String id, String path, bool deliverable) async {
    final json = await _postJson('/api/workspaces/$id/mark-deliverable', {
      'path': path,
      'deliverable': deliverable,
    });
    final entry = json['entry'];
    if (entry is! Map<String, dynamic>) throw ApiException('bad_response', '响应缺少 entry');
    return FileEntry.fromJson(entry);
  }

  // ─── chat（原生 chat 0.3.2）─────────────────────────────────────────────

  @override
  Future<List<ProjectGroup>> listChatSessions() async {
    final json = await _getJson('/api/sessions');
    final list = json['projects'];
    if (list is! List) throw const ApiException('bad_response', '会话列表响应缺少 projects');
    return list
        .whereType<Map<String, dynamic>>()
        .map(ProjectGroup.fromJson)
        .toList();
  }

  @override
  Future<void> deleteChatSession(String instanceId) async {
    await _getJson('/api/delete-session?instanceId=$instanceId');
  }

  @override
  Future<void> renameChatSession(String filePath, String name) async {
    await _postJson('/api/sessions/rename', {'path': filePath, 'name': name});
  }

  @override
  Future<void> pinChatSession(String id, int pinned) async {
    await _postJson('/api/sessions/$id/pin', {'pinned': pinned});
  }

  @override
  Future<PiSettings> piSettings() async {
    final json = await _getJson('/api/pi/settings');
    return PiSettings.fromJson(json);
  }

  @override
  Future<List<ModelInfo>> modelCatalog() async {
    final json = await _getJson('/api/pi/model-catalog');
    final list = json['models'];
    if (list is! List) return const [];
    return list
        .whereType<Map<String, dynamic>>()
        .map(ModelInfo.fromCatalogJson)
        .toList();
  }

  @override
  Future<List<ModelInfo>> rpcAvailableModels() async {
    final json = await _postJson('/api/rpc', {'type': 'get_available_models'});
    final data = json['data'];
    final list = data is Map<String, dynamic> ? data['models'] : null;
    if (list is! List) return const [];
    return list
        .whereType<Map<String, dynamic>>()
        .map(ModelInfo.fromRpcJson)
        .toList();
  }

  @override
  Future<List<SearchHit>> searchChat(String q, {int limit = 50}) async {
    final json = await _getJson('/api/search?q=${Uri.encodeQueryComponent(q)}&limit=$limit');
    final list = json['results'];
    if (list is! List) return const [];
    return list
        .whereType<Map<String, dynamic>>()
        .map(SearchHit.fromJson)
        .toList();
  }

  @override
  Future<BudgetStatus> budgetStatus() async {
    final json = await _getJson('/api/budget/status');
    return BudgetStatus.fromJson(json);
  }

  Future<Map<String, dynamic>> _getJson(String path) async {
    return _request(() async {
      final resp = await _dio.get<Map<String, dynamic>>(path);
      return resp.data ?? const {};
    });
  }

  Future<Map<String, dynamic>> _postJson(String path, Map<String, dynamic> body) async {
    return _request(() async {
      final resp = await _dio.post<Map<String, dynamic>>(path, data: body);
      return resp.data ?? const {};
    });
  }

  Future<Map<String, dynamic>> _putJson(String path, Map<String, dynamic> body) async {
    return _request(() async {
      final resp = await _dio.put<Map<String, dynamic>>(path, data: body);
      return resp.data ?? const {};
    });
  }

  Future<T> _request<T>(Future<T> Function() run) async {
    try {
      return await run();
    } on DioException catch (e) {
      throw _toApiException(e);
    }
  }

  ApiException _toApiException(DioException e) {
    // SPA fallback：未注册的 /api/* 路径返回 200 HTML → 视为 work 不支持。
    final contentType = e.response?.headers.value(Headers.contentTypeHeader) ?? '';
    if (contentType.contains('text/html')) {
      return const ApiException('not_supported', '服务端不支持 work 模块（接口无响应）');
    }
    final data = e.response?.data;
    if (data is Map<String, dynamic>) {
      final code = data['error'];
      final message = data['message'];
      if (code is String) {
        return ApiException(code, message is String ? message : code);
      }
    }
    switch (e.type) {
      case DioExceptionType.connectionTimeout:
      case DioExceptionType.sendTimeout:
      case DioExceptionType.receiveTimeout:
        return ApiException('timeout', '连接服务端超时：$baseUrl');
      case DioExceptionType.connectionError:
        return ApiException('unreachable', '无法连接服务端：$baseUrl');
      case DioExceptionType.badResponse:
        return ApiException('http_${e.response?.statusCode ?? 0}', 'HTTP 错误 ${e.response?.statusCode}');
      default:
        return ApiException('network_error', '网络错误：${e.message}');
    }
  }
}
