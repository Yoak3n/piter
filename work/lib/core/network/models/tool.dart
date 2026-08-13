/// 工具执行模型（tool_execution_* 事件），对齐 pi_rpc event.rs + Vue ToolCard。
library;

enum ToolExecutionStatus {
  pending('pending'),
  streaming('streaming'),
  complete('complete'),
  error('error');

  const ToolExecutionStatus(this.wire);
  final String wire;
}

/// 一次工具调用（随 tool_execution_start/update/end 演进状态）。
class ToolExecution {
  const ToolExecution({
    required this.toolCallId,
    required this.toolName,
    required this.status,
    this.args = const {},
    this.output = '',
    this.isError = false,
  });

  final String toolCallId;
  final String toolName;
  final ToolExecutionStatus status;
  final Map<String, dynamic> args;
  final String output;
  final bool isError;

  ToolExecution copyWith({
    ToolExecutionStatus? status,
    Map<String, dynamic>? args,
    String? output,
    bool? isError,
  }) =>
      ToolExecution(
        toolCallId: toolCallId,
        toolName: toolName,
        status: status ?? this.status,
        args: args ?? this.args,
        output: output ?? this.output,
        isError: isError ?? this.isError,
      );

  /// edit 工具的 unified diff patch（args.details.patch），供 diff 渲染。
  String? get patch {
    final details = args['details'];
    if (details is Map) return details['patch'] as String?;
    return null;
  }

  /// 预览文本（对齐 Vue getArgsPreview）：path / command / query / url 优先。
  String get argsPreview {
    final v = args['path'] ?? args['command'] ?? args['query'] ?? args['url'];
    if (v is String && v.isNotEmpty) {
      const max = 80;
      return v.length <= max ? v : v.substring(0, max);
    }
    return '';
  }
}
