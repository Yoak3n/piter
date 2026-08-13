/// 服务端能力探测（对齐移动端规划：调 work 接口失败/无响应即不支持）。
///
/// 注意：0.2.x SPA fallback 会把未注册的 /api/* 路径返回 200 HTML，
/// 因此"不支持"不能只看状态码——需校验 content-type 与 JSON 结构。
library;

import 'package:dio/dio.dart';

import 'models/server_health.dart';

/// 探测结果。
class ServerCapability {
  const ServerCapability({
    this.health,
    this.workSupported = false,
    this.reachable = false,
    this.error,
  });

  /// 连接成功时的 /api/health 信息。
  final ServerHealth? health;

  /// work 模块是否可用（/api/workspaces 返回 JSON 且含 workspaces 数组）。
  final bool workSupported;

  /// 服务端是否可达（health 至少返回 JSON）。
  final bool reachable;

  /// 探测失败原因（不可达时为可读信息）。
  final String? error;
}

/// 本机默认 gateway 地址（与后端 DEFAULT_HTTP_PORT=31421 一致）。
///
/// 仅作为 Web 端「同源探测失败后」的降级起点（桌面端同机开发场景）——
/// 必须真实探测可达才展示，不臆造名称；它本身不构成"已保存服务器"。
const String kLocalProbeBaseUrl = 'http://127.0.0.1:31421';

/// 探测指定服务端（可注入 dio 供测试）。
Future<ServerCapability> probeServer(String baseUrl, {Dio? dio}) async {
  final client = dio ??
      Dio(BaseOptions(
        baseUrl: baseUrl,
        connectTimeout: const Duration(seconds: 5),
        receiveTimeout: const Duration(seconds: 5),
        headers: const {'Accept': 'application/json'},
      ));

  // 1) /api/health
  ServerHealth? health;
  var reachable = false;
  String? error;
  try {
    final resp = await client.get<Map<String, dynamic>>('/api/health');
    health = ServerHealth.fromJson(resp.data ?? const {});
    reachable = true;
  } on DioException catch (e) {
    error = switch (e.type) {
      DioExceptionType.connectionTimeout ||
      DioExceptionType.sendTimeout ||
      DioExceptionType.receiveTimeout =>
        '连接超时',
      DioExceptionType.connectionError => '无法连接',
      _ => 'HTTP 错误 ${e.response?.statusCode ?? '未知'}',
    };
  } catch (_) {
    error = '响应格式异常';
  }

  // 2) work 能力：/api/workspaces 必须返回 JSON 且含 workspaces 数组
  //    （SPA fallback 会返回 200 HTML，此处通过 content-type/结构区分）。
  var workSupported = false;
  if (reachable) {
    try {
      final resp = await client.get<Map<String, dynamic>>('/api/workspaces');
      final data = resp.data;
      if (data is Map<String, dynamic> && data['workspaces'] is List) {
        workSupported = true;
      }
    } catch (_) {
      workSupported = false;
    }
  }

  return ServerCapability(
    health: health,
    workSupported: workSupported,
    reachable: reachable,
    error: error,
  );
}
