/// 能力探测与真 ApiClient 测试（dio fake adapter 模拟 0.2.1 SPA fallback）。
library;

import 'dart:convert';
import 'dart:typed_data';

import 'package:dio/dio.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:piter_work/core/network/api_client.dart';
import 'package:piter_work/core/network/http_api_client.dart';
import 'package:piter_work/core/network/models/models.dart';
import 'package:piter_work/core/network/probe.dart';

/// 按路径返回固定响应的 fake adapter。
/// content-type 由响应体类型推断：Map → JSON，String → text/html。
class FakeAdapter implements HttpClientAdapter {
  FakeAdapter(this.respond);

  final Response Function(RequestOptions) respond;

  @override
  Future<ResponseBody> fetch(
    RequestOptions options,
    Stream<Uint8List>? requestStream,
    Future<void>? cancelFuture,
  ) async {
    final r = respond(options);
    final body = r.data is String ? r.data as String : jsonEncode(r.data);
    final contentType = r.data is Map ? 'application/json' : 'text/html';
    return ResponseBody.fromString(
      body,
      r.statusCode ?? 200,
      headers: {Headers.contentTypeHeader: [contentType]},
    );
  }

  @override
  void close({bool force = false}) {}
}

Dio _dioFor(Response Function(RequestOptions) respond) =>
    Dio(BaseOptions(baseUrl: 'http://127.0.0.1:31421'))
      ..httpClientAdapter = FakeAdapter(respond);

void main() {
  group('probeServer', () {
    test('0.2.1 服务端：health OK，work 不支持（SPA fallback 返回 HTML）', () async {
      final dio = _dioFor((options) {
        if (options.path == '/api/health') {
          return Response(
            requestOptions: options,
            statusCode: 200,
            data: const {
              'status': 'ok',
              'version': '0.2.1',
              'pi_version': '0.83.0',
              'broker_url': 'ws://127.0.0.1:31421/ws',
            },
          );
        }
        // /api/workspaces 未注册 → SPA fallback 返回 200 HTML
        return Response(
          requestOptions: options,
          statusCode: 200,
          data: '<!doctype html><title>Piter Chat</title>',
        );
      });

      final cap = await probeServer('http://127.0.0.1:31421', dio: dio);
      expect(cap.reachable, isTrue);
      expect(cap.health?.version, '0.2.1');
      expect(cap.workSupported, isFalse);
    });

    test('支持 work 的服务端：workspaces 返回 JSON 数组', () async {
      final dio = _dioFor((options) {
        if (options.path == '/api/health') {
          return Response(
            requestOptions: options,
            statusCode: 200,
            data: const {'status': 'ok', 'version': '0.3.0', 'pi_version': '0.83.0'},
          );
        }
        return Response(
          requestOptions: options,
          statusCode: 200,
          data: const {'workspaces': <dynamic>[]},
        );
      });

      final cap = await probeServer('http://127.0.0.1:31421', dio: dio);
      expect(cap.reachable, isTrue);
      expect(cap.workSupported, isTrue);
      expect(cap.health?.version, '0.3.0');
    });

    test('服务端不可达：reachable=false + 可读错误', () async {
      final dio = _dioFor((_) => throw DioException.connectionError(
            requestOptions: RequestOptions(path: '/api/health'),
            reason: 'connection refused',
          ));
      final cap = await probeServer('http://127.0.0.1:31421', dio: dio);
      expect(cap.reachable, isFalse);
      expect(cap.workSupported, isFalse);
      expect(cap.error, isNotNull);
    });
  });

  group('HttpApiClient', () {
    test('listWorkspaces 解析 JSON 列表', () async {
      final dio = _dioFor((options) => Response(
            requestOptions: options,
            statusCode: 200,
            data: const {
              'workspaces': [
                {
                  'id': 'ws_1',
                  'name': '真实空间',
                  'cwd': 'E:/data/piter/workspaces/ws_1/',
                  'createdAt': 1723200000000,
                  'updatedAt': 1723200000000,
                  'fileCount': 3,
                  'sizeBytes': 1024,
                  'mode': 'ask',
                },
              ],
            },
          ));
      final client = HttpApiClient(baseUrl: 'http://127.0.0.1:31421', dio: dio);
      final list = await client.listWorkspaces();
      expect(list.length, 1);
      expect(list.first.name, '真实空间');
      expect(list.first.mode, WorkspaceMode.ask);
    });

    test('SPA fallback（HTML）→ ApiException not_supported', () async {
      final dio = _dioFor((options) => Response(
            requestOptions: options,
            statusCode: 200,
            data: '<!doctype html><title>Piter Chat</title>',
          ));
      final client = HttpApiClient(baseUrl: 'http://127.0.0.1:31421', dio: dio);
      await expectLater(
        client.listWorkspaces(),
        throwsA(isA<ApiException>().having((e) => e.code, 'code', 'not_supported')),
      );
    });

    test('HTTP 错误映射 → ApiException', () async {
      final dio = _dioFor((options) => Response(
            requestOptions: options,
            statusCode: 404,
            data: const {'success': false, 'error': 'workspace_not_found', 'message': '不存在'},
          ));
      final client = HttpApiClient(baseUrl: 'http://127.0.0.1:31421', dio: dio);
      await expectLater(
        client.getWorkspace('ws_x'),
        throwsA(isA<ApiException>()
            .having((e) => e.code, 'code', 'workspace_not_found')
            .having((e) => e.message, 'message', '不存在')),
      );
    });
  });
}
