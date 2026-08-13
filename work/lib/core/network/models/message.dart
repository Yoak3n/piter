/// pi 消息模型（当前简化：role + 文本聚合），字段对齐 pi_rpc message.rs。
library;

enum PiMessageRole {
  user('user'),
  assistant('assistant'),
  system('system'),
  toolResult('toolResult');

  const PiMessageRole(this.wire);
  final String wire;

  static PiMessageRole fromWire(String? value) => switch (value) {
        'assistant' => PiMessageRole.assistant,
        'system' => PiMessageRole.system,
        'toolResult' => PiMessageRole.toolResult,
        _ => PiMessageRole.user,
      };
}

/// 会话中的一条消息。
class PiMessage {
  const PiMessage({
    this.id,
    required this.role,
    required this.content,
    this.timestamp,
  });

  final String? id;
  final PiMessageRole role;
  final String content;
  final DateTime? timestamp;

  factory PiMessage.fromJson(Map<String, dynamic> json) => PiMessage(
        id: json['id'] as String?,
        role: PiMessageRole.fromWire(json['role'] as String?),
        content: _contentToString(json['content']),
        timestamp: json['timestamp'] is int
            ? DateTime.fromMillisecondsSinceEpoch(json['timestamp'] as int)
            : null,
      );

  Map<String, dynamic> toJson() => {
        if (id != null) 'id': id,
        'role': role.wire,
        'content': content,
        if (timestamp != null) 'timestamp': timestamp!.millisecondsSinceEpoch,
      };
}

/// content 可能是字符串或 content block 数组（text/thinking/...），当前聚合为纯文本。
String _contentToString(dynamic content) {
  if (content == null) return '';
  if (content is String) return content;
  if (content is List) {
    final parts = <String>[];
    for (final block in content) {
      if (block is Map<String, dynamic>) {
        final type = block['type'];
        final text = block['text'] ?? block['thinking'];
        if (text is String && text.isNotEmpty) {
          parts.add(type == 'thinking' ? '🧠 $text' : text);
        }
      }
    }
    return parts.join('\n');
  }
  return '';
}
