/// 契约模型 JSON 解析测试（对齐 mock-contract.md §1.2 / §3.3）。
library;

import 'package:flutter_test/flutter_test.dart';

import 'package:piter_work/core/network/api_client.dart';
import 'package:piter_work/core/network/models/models.dart';
import 'package:piter_work/shared/ws_events.dart';

void main() {
  group('Workspace.fromJson', () {
    test('完整字段解析', () {
      final ws = Workspace.fromJson(const {
        'id': 'ws_ab12cd',
        'name': '我的工作空间',
        'cwd': 'E:/data/piter/workspaces/ws_ab12cd/',
        'createdAt': 1723200000000,
        'updatedAt': 1723200000000,
        'fileCount': 12,
        'sizeBytes': 3428000,
        'mode': 'ask',
      });
      expect(ws.id, 'ws_ab12cd');
      expect(ws.mode, WorkspaceMode.ask);
      expect(ws.fileCount, 12);
      expect(ws.createdAt.millisecondsSinceEpoch, 1723200000000);
    });

    test('mode 三态映射', () {
      expect(Workspace.fromJson(const {'id': 'a', 'name': 'n', 'mode': 'allow'}).mode,
          WorkspaceMode.allow);
      expect(Workspace.fromJson(const {'id': 'a', 'name': 'n', 'mode': 'deny'}).mode,
          WorkspaceMode.deny);
      // 未知/缺失回退 ask
      expect(Workspace.fromJson(const {'id': 'a', 'name': 'n'}).mode, WorkspaceMode.ask);
    });

    test('时间字段兼容 RFC3339 字符串', () {
      final ws = Workspace.fromJson(const {
        'id': 'a',
        'name': 'n',
        'createdAt': '2024-08-09T10:00:00Z',
        'updatedAt': '2024-08-09T10:00:00Z',
      });
      expect(ws.createdAt.year, 2024);
    });
  });

  group('FileEntry.fromJson', () {
    test('file 与 dir 解析', () {
      final f = FileEntry.fromJson(const {
        'path': 'src/main.rs',
        'type': 'file',
        'size': 1024,
        'mtime': 1723200000000,
        'isDeliverable': false,
      });
      expect(f.type, FileEntryType.file);
      expect(f.size, 1024);
      expect(f.isDeliverable, isFalse);
      expect(f.fileName, 'main.rs');
      expect(f.parentPath, 'src');

      final d = FileEntry.fromJson(const {'path': 'src', 'type': 'dir'});
      expect(d.type, FileEntryType.dir);
    });
  });

  group('Artifact.fromJson', () {
    test('op/source/deliverable 解析', () {
      final a = Artifact.fromJson(const {
        'id': 'art_01',
        'workspaceId': 'ws_ab12cd',
        'sessionId': 'session-uuid',
        'turnId': 7,
        'path': 'output/report.md',
        'op': 'new',
        'size': 2048,
        'linesAdded': 12,
        'linesDeleted': 0,
        'source': 'snapshot',
        'deliverable': true,
        'createdAt': 1723200000000,
      });
      expect(a.op, ArtifactOp.newFile);
      expect(a.source, ArtifactSource.snapshot);
      expect(a.deliverable, isTrue);
      expect(a.turnId, 7);
      expect((a.linesAdded, a.linesDeleted), (12, 0));
    });

    test('modified 行数统计（净变化）', () {
      final a = Artifact.fromJson(const {
        'id': 'art_02',
        'path': 'src/lib.rs',
        'op': 'modified',
        'linesAdded': 3,
        'linesDeleted': 1,
        'size': 9001,
      });
      expect(a.op, ArtifactOp.modified);
      expect((a.linesAdded, a.linesDeleted), (3, 1));
      // 缺省为 0（旧数据兼容）
      final old = Artifact.fromJson(const {'id': 'x', 'path': 'p', 'op': 'new'});
      expect((old.linesAdded, old.linesDeleted), (0, 0));
    });

    test('modified / deleted 映射', () {
      expect(Artifact.fromJson(const {'id': 'a', 'path': 'p', 'op': 'modified'}).op,
          ArtifactOp.modified);
      expect(Artifact.fromJson(const {'id': 'a', 'path': 'p', 'op': 'deleted'}).op,
          ArtifactOp.deleted);
    });

    test('ArtifactTurn 按 turn 分组', () {
      final turn = ArtifactTurn.fromJson(const {
        'turnId': 7,
        'createdAt': 1723200000000,
        'items': [
          {'id': 'x', 'path': 'output/report.md', 'op': 'new', 'deliverable': true},
          {'id': 'y', 'path': 'src/lib.rs', 'op': 'modified', 'deliverable': false},
        ],
      });
      expect(turn.turnId, 7);
      expect(turn.items.length, 2);
      expect(turn.items.first.deliverable, isTrue);
    });
  });

  group('PiMessage.fromJson', () {
    test('字符串 content', () {
      final m = PiMessage.fromJson(const {'role': 'user', 'content': '你好'});
      expect(m.role, PiMessageRole.user);
      expect(m.content, '你好');
    });

    test('content block 数组聚合文本', () {
      final m = PiMessage.fromJson(const {
        'role': 'assistant',
        'content': [
          {'type': 'text', 'text': '第一段'},
          {'type': 'thinking', 'thinking': '内部思考'},
        ],
      });
      expect(m.content, contains('第一段'));
      expect(m.content, contains('内部思考'));
    });
  });

  group('UploadResult.fromJson', () {
    test('uploaded / rejected 解析', () {
      final r = UploadResult.fromJson(const {
        'uploaded': ['a.txt', 'docs/b.md'],
        'rejected': [
          {'path': 'output/x.md', 'reason': 'output 目录不可上传'},
          {'path': '../evil.txt', 'reason': 'invalid path'},
        ],
      });
      expect(r.uploaded, ['a.txt', 'docs/b.md']);
      expect(r.rejected.length, 2);
      expect(r.rejected.first.path, 'output/x.md');
      expect(r.rejected.last.reason, contains('invalid'));
      expect(r.hasAny, isTrue);
    });

    test('空响应缺省', () {
      final r = UploadResult.fromJson(const {});
      expect(r.uploaded, isEmpty);
      expect(r.rejected, isEmpty);
      expect(r.hasAny, isFalse);
    });
  });

  group('parseWsEvent', () {
    test('capabilities', () {
      final e = parseWsEvent(const {
        'type': 'capabilities',
        'payload': {'protocolVersion': '1.0', 'client_id': 42},
      });
      expect(e, isA<CapabilitiesEvent>());
      final cap = e as CapabilitiesEvent;
      expect(cap.protocolVersion, '1.0');
      expect(cap.clientId, 42);
    });

    test('lifecycle 信封内 message_update 增量', () {
      final e = parseWsEvent(const {
        'type': 'event',
        'instanceId': 'ins-1',
        'event': {
          'type': 'message_update',
          'assistantMessageEvent': {'type': 'text_delta', 'delta': '增量文本'},
        },
      });
      expect(e, isA<MessageEvent>());
      final m = e as MessageEvent;
      expect(m.phase, 'update');
      expect(m.delta, '增量文本');
      expect(m.instanceId, 'ins-1');
    });

    test('lifecycle 信封内 tool_execution_end（camelCase 字段）', () {
      final e = parseWsEvent(const {
        'type': 'event',
        'instanceId': 'ins-1',
        'event': {
          'type': 'tool_execution_end',
          'toolCallId': 't1',
          'toolName': 'edit',
          'result': 'ok',
        },
      });
      final t = e as ToolEvent;
      expect(t.phase, 'end');
      expect(t.toolCallId, 't1');
      expect(t.toolName, 'edit');
      expect(t.output, 'ok');
    });

    test('turn_artifacts（payload 形态 + 顶层兼容）', () {
      final e = parseWsEvent(const {
        'type': 'turn_artifacts',
        'instanceId': 'ins-1',
        'payload': {
          'workspaceId': 'ws_ab12cd',
          'turnId': 7,
          'items': [
            {'path': 'output/report.md', 'op': 'new', 'size': 2048, 'deliverable': true},
          ],
        },
      });
      final t = e as TurnArtifactsEvent;
      expect(t.workspaceId, 'ws_ab12cd');
      expect(t.turnId, 7);
      expect(t.items.single.path, 'output/report.md');
      expect(t.items.single.deliverable, isTrue);
    });

    test('write_block', () {
      final e = parseWsEvent(const {
        'type': 'write_block',
        'payload': {
          'workspaceId': 'ws_ab12cd',
          'path': 'E:/other/x.txt',
          'reason': '写入位置应在工作空间内',
          'requestId': 'wb_01',
        },
      });
      final w = e as WriteBlockEvent;
      expect(w.requestId, 'wb_01');
      expect(w.reason, contains('工作空间'));
    });

    test('gateway_response', () {
      final e = parseWsEvent(const {
        'type': 'gateway_response',
        'payload': {'requestId': 'wb_01', 'success': true, 'data': {'approved': true}},
      });
      final g = e as GatewayResponseEvent;
      expect(g.success, isTrue);
      expect(g.data?['approved'], isTrue);
    });

    test('session_snapshot（历史消息恢复）', () {
      final e = parseWsEvent(const {
        'type': 'session_snapshot',
        'instanceId': 'ins-1',
        'messages': [
          {'role': 'user', 'content': '帮我优化构建'},
          {'role': 'assistant', 'content': '好的，开始处理。'},
        ],
        'messageSeq': 2,
      });
      final s = e as SessionSnapshotEvent;
      expect(s.instanceId, 'ins-1');
      expect(s.messageSeq, 2);
      expect(s.messages.length, 2);
      expect(s.messages.first.role, PiMessageRole.user);
      expect(s.messages.last.content, '好的，开始处理。');
    });

    test('未知事件', () {
      final e = parseWsEvent(const {'type': 'whatever'});
      expect(e, isA<UnknownEvent>());
    });
  });
}
