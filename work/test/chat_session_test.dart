/// 原生 chat reducer 状态流转测试（快照回放 / 流式组装 / 命令应答 / 扩展卡片）。
library;

import 'package:flutter_test/flutter_test.dart';

import 'package:piter_work/core/network/models/models.dart';
import 'package:piter_work/features/chat/providers/chat_session.dart';
import 'package:piter_work/shared/ws_events.dart';

void main() {
  group('快照回放', () {
    test('消息 + toolCall/toolResult 折叠', () {
      final state = reduceChatSession(
        const ChatSessionState(),
        SessionSnapshotEvent(
          instanceId: 'ins-1',
          messages: const [],
          rawMessages: [
            {
              'role': 'user',
              'content': '帮我画个流程图',
              'timestamp': 1000,
            },
            {
              'role': 'assistant',
              'content': [
                {'type': 'thinking', 'thinking': '先规划步骤'},
                {
                  'type': 'toolCall',
                  'id': 'call_1',
                  'name': 'edit',
                  'arguments': {'path': 'a.md'},
                },
                {'type': 'text', 'text': '完成'},
              ],
            },
            {
              'role': 'toolResult',
              'toolCallId': 'call_1',
              'toolName': 'edit',
              'output': 'ok',
              'isError': false,
            },
          ],
        ),
      );

      expect(state.instanceId, 'ins-1');
      expect(state.entries.length, 3);
      expect(state.entries[0], isA<ChatMsgEntry>());
      expect((state.entries[0] as ChatMsgEntry).message.role, 'user');
      expect((state.entries[0] as ChatMsgEntry).message.text, '帮我画个流程图');
      // toolCall → ChatToolEntry（输出由 toolResult 折叠填充）
      final tool = state.entries[1] as ChatToolEntry;
      expect(tool.tool.toolCallId, 'call_1');
      expect(tool.tool.output, 'ok');
      expect(tool.tool.status, ToolExecutionStatus.complete);
      // assistant 文本保留
      final asst = state.entries[2] as ChatMsgEntry;
      expect(asst.message.hasThinking, isTrue);
      expect(asst.message.text, '完成');
    });

    test('扩展卡片 system 消息挂载为 ChatExtEntry', () {
      final state = reduceChatSession(
        const ChatSessionState(),
        SessionSnapshotEvent(
          instanceId: 'ins-1',
          messages: const [],
          rawMessages: [
            {
              'role': 'system',
              'content': '',
              'extUi': {
                'id': 'req_1',
                'method': 'confirm',
                'title': '确认删除?',
                'answered': false,
              },
            },
          ],
        ),
      );
      expect(state.entries.single, isA<ChatExtEntry>());
      expect((state.entries.single as ChatExtEntry).ui.title, '确认删除?');
    });
  });

  group('流式组装', () {
    test('text_delta 增量 → message_end 定型', () {
      var state = reduceChatSession(
        const ChatSessionState(),
        MessageEvent(
          phase: 'start',
          instanceId: 'ins-1',
          rawMessage: {'role': 'assistant', 'content': ''},
        ),
      );
      state = reduceChatSession(
        state,
        const MessageEvent(phase: 'update', instanceId: 'ins-1', delta: '你好'),
      );
      state = reduceChatSession(
        state,
        const MessageEvent(phase: 'update', instanceId: 'ins-1', delta: '，世界'),
      );
      final msg = state.entries.single as ChatMsgEntry;
      expect(msg.streaming, isTrue);
      expect(msg.textBuffer, '你好，世界');
      expect(state.streaming, isTrue);

      state = reduceChatSession(
        state,
        MessageEvent(
          phase: 'end',
          instanceId: 'ins-1',
          rawMessage: {
            'role': 'assistant',
            'content': [{'type': 'text', 'text': '你好，世界！'}],
          },
        ),
      );
      final done = state.entries.single as ChatMsgEntry;
      expect(done.streaming, isFalse);
      expect(done.message.text, '你好，世界！');
    });

    test('thinking_delta 与 text_delta 分池累积', () {
      var state = const ChatSessionState();
      state = reduceChatSession(
        state,
        const MessageEvent(phase: 'update', instanceId: 'ins-1', delta: '思考中…'),
      );
      state = reduceChatSession(
        state,
        const MessageEvent(
          phase: 'update',
          instanceId: 'ins-1',
          delta: '正文',
          deltaType: 'thinking',
        ),
      );
      // 第一条 update 无流式条目时新建 assistant 条目
      final msg = state.entries.single as ChatMsgEntry;
      expect(msg.thinkingBuffer, '正文');
      expect(msg.textBuffer, '思考中…');
    });

    test('user 消息 start 不建条目（本地回显防双写）', () {
      final state = reduceChatSession(
        const ChatSessionState(),
        MessageEvent(
          phase: 'start',
          instanceId: 'ins-1',
          rawMessage: {'role': 'user', 'content': 'hi'},
        ),
      );
      expect(state.entries, isEmpty);
    });
  });

  group('命令应答与扩展', () {
    test('new_session 应答回填 instanceId', () {
      final state = reduceChatSession(
        const ChatSessionState(),
        const CommandResponseEvent(
          command: 'new_session',
          success: true,
          data: {'instanceId': 'ins-9'},
        ),
      );
      expect(state.instanceId, 'ins-9');
    });

    test('get_commands 应答缓存斜杠命令', () {
      final state = reduceChatSession(
        const ChatSessionState(),
        const CommandResponseEvent(
          command: 'get_commands',
          success: true,
          data: {
            'commands': [
              {'name': 'run', 'description': '运行测试', 'source': 'skill'},
            ],
          },
        ),
      );
      expect(state.slashCommands.single.name, 'run');
      expect(state.slashCommands.single.source, 'skill');
    });

    test('extension_ui_request 阻塞方法进消息流，notify 不进', () {
      final blocked = reduceChatSession(
        const ChatSessionState(),
        ExtUiRequestEvent(
          instanceId: 'ins-1',
          method: 'confirm',
          id: 'r1',
          title: '确认',
        ),
      );
      expect(blocked.entries.single, isA<ChatExtEntry>());

      final notify = reduceChatSession(
        const ChatSessionState(),
        ExtUiRequestEvent(
          instanceId: 'ins-1',
          method: 'notify',
          id: 'n1',
          title: '提示',
          notifyType: 'info',
        ),
      );
      expect(notify.entries, isEmpty);
    });

    test('agent_end 结束流式，abort 保留已输出', () {
      var state = reduceChatSession(
        const ChatSessionState(),
        const MessageEvent(phase: 'update', instanceId: 'ins-1', delta: '部分输出'),
      );
      state = reduceChatSession(state, const AgentEndEvent(instanceId: 'ins-1'));
      expect(state.streaming, isFalse);
      final msg = state.entries.single as ChatMsgEntry;
      expect(msg.streaming, isFalse);
      // 定型：缓冲文本迁入 message.blocks。
      expect(msg.textBuffer, isEmpty);
      expect(msg.message.text, '部分输出');
    });
  });

  group('会话模型解析', () {
    test('SessionInfo / ProjectGroup 字段', () {
      final g = ProjectGroup.fromJson(const {
        'id': 'proj_1',
        'path': '/data/proj',
        'name': '项目A',
        'projectType': 'project',
        'sessions': [
          {
            'id': 's1',
            'label': '优化构建',
            'filePath': '/data/proj/s1.jsonl',
            'cwd': '/data/proj',
            'updatedAt': 1723200000,
            'state': 'busy',
            'model': 'gpt-4o',
            'modelProvider': 'openai',
            'messageCount': 3,
            'messageSeq': 5,
            'pinned': 1,
          },
        ],
      });
      expect(g.name, '项目A');
      final s = g.sessions.single;
      expect(s.label, '优化构建');
      expect(s.busy, isTrue);
      expect(s.model, 'gpt-4o');
      expect(s.runtimeId, 's1');
      expect(s.pinned, 1);
    });

    test('SearchHit 解析', () {
      final h = SearchHit.fromJson(const {
        'sessionId': 's1',
        'label': '会话',
        'snippet': '摘要',
        'timestamp': 1723200000000,
      });
      expect(h.sessionId, 's1');
      expect(h.timestamp, 1723200000000);
    });
  });
}
