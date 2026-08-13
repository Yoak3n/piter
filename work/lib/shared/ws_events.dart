/// pi / gateway 事件解析（移植 Vue handlePiEvent 的骨架，只解析 work 相关事件）。
///
/// 信封格式（对齐 gateway event_loop.rs）：
/// - lifecycle 事件：`{"type":"event","event":<pi 原始事件>,"instanceId":…}`
/// - 其他事件：`{"type":"<type>","payload":{…},"instanceId":…}`
library;

import 'dart:convert';

import '../core/network/models/models.dart';

/// 解析后的 work 事件（sealed）。
sealed class WsEvent {
  const WsEvent();
}

/// capabilities（连接时，服务端协议能力）。
class CapabilitiesEvent extends WsEvent {
  const CapabilitiesEvent({required this.protocolVersion, required this.clientId});

  final String protocolVersion;
  final int clientId;
}

/// sessions_list（会话/项目列表广播）。
class SessionsListEvent extends WsEvent {
  const SessionsListEvent(this.projects);

  final List<dynamic> projects;
}

/// message_start / message_update / message_end。
class MessageEvent extends WsEvent {
  const MessageEvent({
    required this.phase,
    required this.instanceId,
    this.delta = '',
    this.deltaType = 'text',
    this.message,
    this.rawMessage,
  });

  /// start | update | end
  final String phase;
  final String instanceId;

  /// 流式增量文本（update 时来自 assistantMessageEvent.delta）。
  final String delta;

  /// 增量类型：text | thinking（chat 区分正文与思考流；work 忽略）。
  final String deltaType;

  /// 完整消息（start/end 时可能携带，work 用聚合形态）。
  final PiMessage? message;

  /// 完整消息原始 JSON（chat 需保留 content block 结构解析）。
  final Map<String, dynamic>? rawMessage;
}

/// tool_execution_start / update / end。
class ToolEvent extends WsEvent {
  const ToolEvent({
    required this.phase,
    required this.instanceId,
    required this.toolCallId,
    required this.toolName,
    this.args = const {},
    this.output = '',
    this.isError = false,
  });

  /// start | update | end
  final String phase;
  final String instanceId;
  final String toolCallId;
  final String toolName;
  final Map<String, dynamic> args;
  final String output;
  final bool isError;
}

/// turn_artifacts（turn_end 后推送，驱动产物区刷新）。
class TurnArtifactsEvent extends WsEvent {
  const TurnArtifactsEvent({
    required this.instanceId,
    required this.workspaceId,
    required this.turnId,
    required this.items,
  });

  final String instanceId;
  final String workspaceId;
  final int turnId;
  final List<TurnArtifactItem> items;
}

/// write_block（写阻断请求，ask 模式）。
class WriteBlockEvent extends WsEvent {
  const WriteBlockEvent({
    required this.instanceId,
    required this.workspaceId,
    required this.path,
    required this.reason,
    required this.requestId,
  });

  final String instanceId;
  final String workspaceId;
  final String path;
  final String reason;
  final String requestId;
}

/// turn_end。
class TurnEndEvent extends WsEvent {
  const TurnEndEvent({required this.instanceId});

  final String instanceId;
}

/// agent_end。
class AgentEndEvent extends WsEvent {
  const AgentEndEvent({required this.instanceId});

  final String instanceId;
}

/// gateway_response（gateway_command 应答，如 approve_write）。
class GatewayResponseEvent extends WsEvent {
  const GatewayResponseEvent({
    required this.requestId,
    required this.success,
    this.data,
    this.error,
  });

  final String requestId;
  final bool success;
  final Map<String, dynamic>? data;
  final String? error;
}

/// session_snapshot（恢复历史：create_workspace_session 复用已有会话时回推，
/// 携带该会话完整消息，按文件序即时间线顺序）。
class SessionSnapshotEvent extends WsEvent {
  const SessionSnapshotEvent({
    required this.instanceId,
    required this.messages,
    this.rawMessages = const [],
    this.messageSeq = 0,
  });

  final String instanceId;

  /// work 用聚合形态（PiMessage）。
  final List<PiMessage> messages;

  /// chat 用原始块结构（保留 thinking/toolCall/image/toolResult）。
  final List<Map<String, dynamic>> rawMessages;
  final int messageSeq;
}

/// response（broker_command 应答：new_session / get_commands / set_model 等）。
class CommandResponseEvent extends WsEvent {
  const CommandResponseEvent({
    required this.command,
    required this.success,
    this.data,
    this.error,
    this.instanceId = '',
  });

  final String command;
  final bool success;
  final Map<String, dynamic>? data;
  final String? error;
  final String instanceId;
}

/// extension_ui_request（扩展阻塞 UI 请求：select/confirm/input/editor/notify…）。
class ExtUiRequestEvent extends WsEvent {
  const ExtUiRequestEvent({
    required this.instanceId,
    required this.method,
    required this.id,
    this.title = '',
    this.message,
    this.placeholder,
    this.prefill,
    this.options = const [],
    this.timeout,
    this.notifyType,
  });

  final String instanceId;
  final String method;
  final String id;
  final String title;
  final String? message;
  final String? placeholder;
  final String? prefill;
  final List<Map<String, dynamic>> options;
  final int? timeout;

  /// notify 方法的类型（info/warn/error）。
  final String? notifyType;
}

/// queue_update（pi 插队队列，只读展示）。
class QueueUpdateEvent extends WsEvent {
  const QueueUpdateEvent({required this.instanceId, required this.steering});

  final String instanceId;
  final List<String> steering;
}

/// session_status（running | idle）。
class SessionStatusEvent extends WsEvent {
  const SessionStatusEvent({required this.instanceId, required this.status});

  final String instanceId;
  final String status;
}

/// error（pi 运行错误 / abort）。
class PiErrorEvent extends WsEvent {
  const PiErrorEvent({
    required this.instanceId,
    this.error,
    this.message,
    this.reason,
    this.aborted = false,
  });

  final String instanceId;
  final String? error;
  final String? message;
  final String? reason;
  final bool aborted;
}

/// auto_retry_start / auto_retry_end（自动重试提示）。
class AutoRetryEvent extends WsEvent {
  const AutoRetryEvent({
    required this.instanceId,
    required this.phase,
    this.attempt = 0,
    this.maxAttempts = 0,
    this.delayMs = 0,
    this.errorMessage,
    this.success = false,
    this.finalError,
  });

  final String instanceId;

  /// start | end
  final String phase;
  final int attempt;
  final int maxAttempts;
  final int delayMs;
  final String? errorMessage;
  final bool success;
  final String? finalError;
}

/// 系统提示消息（extension_error / extension_load_failed / pi_startup_failed /
/// command_undeliverable / fork_error / fork_warn / disconnected 等）。
class SystemNoticeEvent extends WsEvent {
  const SystemNoticeEvent({required this.kind, required this.message, this.instanceId = ''});

  final String kind;
  final String message;
  final String instanceId;
}

/// 未识别 / 本阶段不处理的事件。
class UnknownEvent extends WsEvent {
  const UnknownEvent({required this.type});

  final String type;
}

/// 解析一条原始 WS 信封 JSON。
WsEvent parseWsEvent(Map<String, dynamic> json) {
  final type = json['type'] as String? ?? '';
  final instanceId = json['instanceId'] as String? ?? '';

  if (type == 'event') {
    final inner = json['event'];
    if (inner is Map<String, dynamic>) {
      return _parseLifecycle(inner, instanceId);
    }
    return UnknownEvent(type: type);
  }

  // 非 lifecycle 事件：优先 payload，兼容契约示例的顶层形态。
  final payload = _payloadOf(json);

  return switch (type) {
    'capabilities' => CapabilitiesEvent(
        protocolVersion: payload['protocolVersion'] as String? ?? '',
        clientId: payload['client_id'] as int? ?? 0,
      ),
    'sessions_list' => SessionsListEvent(
        payload['projects'] as List<dynamic>? ?? const [],
      ),
    'turn_artifacts' => TurnArtifactsEvent(
        instanceId: instanceId,
        workspaceId: payload['workspaceId'] as String? ?? '',
        turnId: payload['turnId'] as int? ?? 0,
        items: (payload['items'] as List<dynamic>? ?? const [])
            .map((e) => TurnArtifactItem.fromJson(e as Map<String, dynamic>))
            .toList(),
      ),
    'write_block' => WriteBlockEvent(
        instanceId: instanceId,
        workspaceId: payload['workspaceId'] as String? ?? '',
        path: payload['path'] as String? ?? '',
        reason: payload['reason'] as String? ?? '',
        requestId: payload['requestId'] as String? ?? '',
      ),
    'gateway_response' => GatewayResponseEvent(
        requestId: payload['requestId'] as String? ?? '',
        success: payload['success'] as bool? ?? false,
        data: payload['data'] as Map<String, dynamic>?,
        error: payload['error'] as String?,
      ),
    'session_snapshot' => SessionSnapshotEvent(
        instanceId: instanceId,
        messages: (payload['messages'] as List<dynamic>? ?? const [])
            .whereType<Map<String, dynamic>>()
            .map(PiMessage.fromJson)
            .toList(),
        rawMessages: (payload['messages'] as List<dynamic>? ?? const [])
            .whereType<Map<String, dynamic>>()
            .toList(),
        messageSeq: payload['messageSeq'] as int? ?? 0,
      ),
    'response' => CommandResponseEvent(
        command: payload['command'] as String? ?? '',
        success: payload['success'] as bool? ?? false,
        data: payload['data'] as Map<String, dynamic>?,
        error: payload['error'] as String?,
        instanceId: payload['instanceId'] as String? ?? instanceId,
      ),
    'command_undeliverable' => SystemNoticeEvent(
        kind: 'command_undeliverable',
        message: payload['reason'] as String? ?? '命令投递失败',
        instanceId: instanceId,
      ),
    'fork_error' => SystemNoticeEvent(
        kind: 'fork_error',
        message: payload['message'] as String? ?? '撤回失败',
        instanceId: instanceId,
      ),
    'fork_warn' => SystemNoticeEvent(
        kind: 'fork_warn',
        message: payload['message'] as String? ?? '',
        instanceId: instanceId,
      ),
    'extension_ui_request' => _parseExtUi(payload, instanceId),
    'turn_end' => TurnEndEvent(instanceId: instanceId),
    'agent_end' => AgentEndEvent(instanceId: instanceId),
    _ => UnknownEvent(type: type),
  };
}

/// 解析 lifecycle 子事件（type: "event" 的内层）。
WsEvent _parseLifecycle(Map<String, dynamic> json, String instanceId) {
  final event = json['type'] as String? ?? '';
  return switch (event) {
    'message_start' => _parseMessage('start', json, instanceId),
    'message_update' => _parseMessage('update', json, instanceId),
    'message_end' => _parseMessage('end', json, instanceId),
    'tool_execution_start' => _parseTool('start', json, instanceId),
    'tool_execution_update' => _parseTool('update', json, instanceId),
    'tool_execution_end' => _parseTool('end', json, instanceId),
    'turn_end' => TurnEndEvent(instanceId: instanceId),
    'agent_end' => AgentEndEvent(instanceId: instanceId),
    'queue_update' => QueueUpdateEvent(
        instanceId: instanceId,
        steering: (json['steering'] as List<dynamic>? ?? const [])
            .whereType<String>()
            .toList(),
      ),
    'session_status' => SessionStatusEvent(
        instanceId: instanceId,
        status: json['status'] as String? ?? 'idle',
      ),
    'error' => PiErrorEvent(
        instanceId: instanceId,
        error: _asString(json['error']),
        message: _asString(json['message']),
        reason: _asString(json['reason']),
        aborted: json['aborted'] as bool? ?? false,
      ),
    'auto_retry_start' => AutoRetryEvent(
        instanceId: instanceId,
        phase: 'start',
        attempt: json['attempt'] as int? ?? 0,
        maxAttempts: json['maxAttempts'] as int? ?? 0,
        delayMs: json['delayMs'] as int? ?? 0,
        errorMessage: json['errorMessage'] as String?,
      ),
    'auto_retry_end' => AutoRetryEvent(
        instanceId: instanceId,
        phase: 'end',
        success: json['success'] as bool? ?? false,
        finalError: json['finalError'] as String?,
      ),
    'extension_ui_request' => _parseExtUi(json, instanceId),
    'extension_error' => SystemNoticeEvent(
        kind: 'extension_error',
        message: _asString(json['error']) ?? '扩展错误',
        instanceId: instanceId,
      ),
    'extension_load_failed' => SystemNoticeEvent(
        kind: 'extension_load_failed',
        message: _asString(json['error']) ?? '扩展加载失败',
        instanceId: instanceId,
      ),
    'pi_startup_failed' => SystemNoticeEvent(
        kind: 'pi_startup_failed',
        message: _asString(json['error']) ?? '启动失败',
        instanceId: instanceId,
      ),
    'pi_started' => SessionStatusEvent(instanceId: instanceId, status: 'running'),
    'pi_exited' || 'disconnected' => SystemNoticeEvent(
        kind: 'pi_exited',
        message: '进程已退出',
        instanceId: instanceId,
      ),
    _ => UnknownEvent(type: 'event:$event'),
  };
}

ExtUiRequestEvent _parseExtUi(Map<String, dynamic> json, String instanceId) => ExtUiRequestEvent(
      instanceId: instanceId,
      method: json['method'] as String? ?? 'notify',
      id: json['id'] as String? ?? '',
      title: json['title'] as String? ?? '',
      message: json['message'] as String?,
      placeholder: json['placeholder'] as String?,
      prefill: json['prefill'] as String?,
      options: (json['options'] as List<dynamic>? ?? const [])
          .whereType<Map<String, dynamic>>()
          .toList(),
      timeout: json['timeout'] as int?,
      notifyType: json['notifyType'] as String?,
    );

MessageEvent _parseMessage(String phase, Map<String, dynamic> json, String instanceId) {
  String delta = '';
  String deltaType = 'text';
  if (phase == 'update') {
    final ame = json['assistantMessageEvent'];
    if (ame is Map<String, dynamic>) {
      delta = ame['delta'] as String? ?? '';
      deltaType = ame['type'] == 'thinking_delta' ? 'thinking' : 'text';
    }
  }
  PiMessage? message;
  final rawMessage = json['message'];
  if (rawMessage is Map<String, dynamic>) {
    message = PiMessage.fromJson(rawMessage);
  }
  return MessageEvent(
    phase: phase,
    instanceId: instanceId,
    delta: delta,
    deltaType: deltaType,
    message: message,
    rawMessage: rawMessage is Map<String, dynamic> ? rawMessage : null,
  );
}

ToolEvent _parseTool(String phase, Map<String, dynamic> json, String instanceId) {
  final args = (json['args'] as Map<String, dynamic>?) ?? const <String, dynamic>{};
  final isError = phase == 'end' && (json['isError'] as bool? ?? false);
  final rawOutput = phase == 'end' ? json['result'] : json['partialResult'];
  return ToolEvent(
    phase: phase,
    instanceId: instanceId,
    toolCallId: json['toolCallId'] as String? ?? '',
    toolName: json['toolName'] as String? ?? '',
    args: args,
    output: _outputToString(rawOutput),
    isError: isError,
  );
}

/// partialResult / result → 展示文本。
String _outputToString(dynamic value) {
  if (value == null) return '';
  if (value is String) return value;
  return const JsonEncoder.withIndent('  ').convert(value);
}

/// 非 lifecycle 事件的数据载体：`payload` 优先，缺省回退顶层（兼容契约 §3.3 示例）。
Map<String, dynamic> _payloadOf(Map<String, dynamic> json) {
  final payload = json['payload'];
  if (payload is Map<String, dynamic>) return payload;
  return json;
}

/// 宽松转字符串（error 等字段可能是任意 JSON 类型）。
String? _asString(dynamic value) => value?.toString();
