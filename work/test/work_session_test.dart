/// work 会话 reducer 测试（消息组装 / 工具流 / 产物 / 写阻断批准）。
library;

import 'package:flutter_test/flutter_test.dart';

import 'package:piter_work/core/network/models/models.dart';
import 'package:piter_work/features/work/providers/work_session.dart';
import 'package:piter_work/shared/ws_events.dart';

void main() {
  group('reduceWorkSession', () {
    test('流式消息组装：user 整条 → assistant 增量 → 完成', () {
      var s = const WorkSessionState();
      s = reduceWorkSession(s, MessageEvent(
        phase: 'start',
        instanceId: 'ins',
        message: const PiMessage(role: PiMessageRole.user, content: '帮我优化构建'),
      ));
      // assistant 的 message_start 携带 message 才会建流式条目。
      s = reduceWorkSession(s, MessageEvent(
        phase: 'start',
        instanceId: 'ins',
        message: const PiMessage(role: PiMessageRole.assistant, content: ''),
      ));
      s = reduceWorkSession(s, const MessageEvent(phase: 'update', instanceId: 'ins', delta: '好的，'));
      s = reduceWorkSession(s, const MessageEvent(phase: 'update', instanceId: 'ins', delta: '开始处理。'));
      s = reduceWorkSession(s, const MessageEvent(
        phase: 'end',
        instanceId: 'ins',
        message: PiMessage(role: PiMessageRole.assistant, content: '好的，开始处理。'),
      ));

      // 实现意图：user 消息由 sendPrompt 本地回显，reducer 不建 user 条目
      // （避免双写重复），流式组装只产生 assistant 一条。
      expect(s.entries.length, 1);
      final assistant = s.entries[0] as MessageEntry;
      expect(assistant.message.role, PiMessageRole.assistant);
      expect(assistant.streaming, isFalse);
      expect(assistant.message.content, '好的，开始处理。');
    });

    test('工具执行：start → end 状态演进', () {
      var s = const WorkSessionState();
      s = reduceWorkSession(s, ToolEvent(
        phase: 'start',
        instanceId: 'ins',
        toolCallId: 't1',
        toolName: 'edit',
        args: const {'path': 'src/main.rs'},
      ));
      s = reduceWorkSession(s, const ToolEvent(
        phase: 'end',
        instanceId: 'ins',
        toolCallId: 't1',
        toolName: 'edit',
        output: '已更新',
      ));

      final tool = (s.entries.single as ToolEntry).tool;
      expect(tool.toolName, 'edit');
      expect(tool.status, ToolExecutionStatus.complete);
      expect(tool.argsPreview, 'src/main.rs');
    });

    test('edit 工具 patch 提取（diff 渲染数据源）', () {
      final tool = ToolExecution(
        toolCallId: 't1',
        toolName: 'edit',
        status: ToolExecutionStatus.complete,
        args: const {
          'path': 'src/lib.rs',
          'details': {'patch': '@@ -1,7 +1,10 @@'},
        },
      );
      expect(tool.patch, '@@ -1,7 +1,10 @@');
    });

    test('turn_artifacts 累积并按 path 去重', () {
      var s = const WorkSessionState();
      s = reduceWorkSession(s, const TurnArtifactsEvent(
        instanceId: 'ins',
        workspaceId: 'ws',
        turnId: 5,
        items: [
          TurnArtifactItem(path: 'output/a.md', op: ArtifactOp.newFile, size: 100, deliverable: true),
        ],
      ));
      s = reduceWorkSession(s, const TurnArtifactsEvent(
        instanceId: 'ins',
        workspaceId: 'ws',
        turnId: 7,
        items: [
          TurnArtifactItem(path: 'output/b.md', op: ArtifactOp.newFile, size: 200, deliverable: true),
          TurnArtifactItem(path: 'output/a.md', op: ArtifactOp.modified, size: 150, deliverable: true),
        ],
      ));

      expect(s.liveArtifacts.length, 2);
      expect(s.liveArtifacts.first.path, 'output/b.md');
      expect(s.liveArtifacts.first.op, ArtifactOp.newFile);
    });

    test('write_block → gateway_response 批准流转', () {
      var s = const WorkSessionState();
      s = reduceWorkSession(s, const WriteBlockEvent(
        instanceId: 'ins',
        workspaceId: 'ws',
        path: 'E:/other/x.txt',
        reason: '需要批准',
        requestId: 'wb_01',
      ));
      expect(s.entries.single, isA<WriteBlockEntry>());
      expect((s.entries.single as WriteBlockEntry).state, WriteBlockState.pending);

      s = reduceWorkSession(s, const GatewayResponseEvent(
        requestId: 'wb_01',
        success: true,
        data: {'approved': true},
      ));
      expect((s.entries.single as WriteBlockEntry).state, WriteBlockState.approved);
    });

    test('turn_end 递增 turnCount', () {
      var s = const WorkSessionState();
      s = reduceWorkSession(s, const TurnEndEvent(instanceId: 'ins'));
      expect(s.turnCount, 1);
    });

    test('session_snapshot 历史回放为时间线（跳过 toolResult）', () {
      var s = const WorkSessionState();
      s = reduceWorkSession(s, const SessionSnapshotEvent(
        instanceId: 'ins',
        messages: [
          PiMessage(role: PiMessageRole.user, content: '第一问'),
          PiMessage(role: PiMessageRole.assistant, content: '第一答'),
          PiMessage(role: PiMessageRole.toolResult, content: '工具结果'),
          PiMessage(role: PiMessageRole.assistant, content: '第二答'),
        ],
      ));
      expect(s.entries.length, 3);
      expect((s.entries[0] as MessageEntry).message.content, '第一问');
      expect((s.entries[1] as MessageEntry).streaming, isFalse);
      // toolResult 不渲染为气泡，后续消息顺序保持
      expect((s.entries[2] as MessageEntry).message.content, '第二答');
    });
  });
}
