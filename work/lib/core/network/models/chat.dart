/// 原生 chat 模型：保留 content block 结构（text/thinking/image/toolCall），
/// 对齐 Vue chat utils/message.ts 的解析，服务于 Flutter 原生会话 UI。
library;

// ─── 消息 content block ────────────────────────────────────────────────────

/// 一条消息的 content 片段。
sealed class ChatBlock {
  const ChatBlock();
}

/// 正文文本。
class TextBlock extends ChatBlock {
  const TextBlock(this.text);
  final String text;
}

/// 思考内容（流式时独立展示、可折叠）。
class ThinkingBlock extends ChatBlock {
  const ThinkingBlock(this.thinking);
  final String thinking;
}

/// 图片（base64 无 data: 前缀）。
class ImageBlock extends ChatBlock {
  const ImageBlock({required this.data, required this.mimeType});
  final String data;
  final String mimeType;
}

/// 工具调用（折叠进 assistant 消息；运行时由 tool_execution_* 事件同构驱动）。
class ToolCallBlock extends ChatBlock {
  const ToolCallBlock({required this.id, required this.name, required this.args});
  final String id;
  final String name;
  final Map<String, dynamic> args;
}

// ─── 扩展 UI 卡片（extension_ui_request 挂载为 system 消息）──────────────

class ChatExtUi {
  const ChatExtUi({
    required this.id,
    required this.method,
    required this.title,
    this.message,
    this.placeholder,
    this.prefill,
    this.options = const [],
    this.timeout,
    this.answered = false,
    this.result,
    this.createdAt = 0,
  });

  final String id;
  final String method;
  final String title;
  final String? message;
  final String? placeholder;
  final String? prefill;
  final List<Map<String, dynamic>> options;
  final int? timeout;
  final bool answered;
  final dynamic result;
  final int createdAt;

  ChatExtUi copyWith({bool? answered, dynamic result}) => ChatExtUi(
        id: id,
        method: method,
        title: title,
        message: message,
        placeholder: placeholder,
        prefill: prefill,
        options: options,
        timeout: timeout,
        answered: answered ?? this.answered,
        result: result ?? this.result,
        createdAt: createdAt,
      );

  factory ChatExtUi.fromJson(Map<String, dynamic> json) => ChatExtUi(
        id: json['id'] as String? ?? '',
        method: json['method'] as String? ?? 'notify',
        title: json['title'] as String? ?? '',
        message: json['message'] as String?,
        placeholder: json['placeholder'] as String?,
        prefill: json['prefill'] as String?,
        options: (json['options'] as List<dynamic>? ?? const [])
            .whereType<Map<String, dynamic>>()
            .toList(),
        timeout: json['timeout'] as int?,
        answered: json['answered'] as bool? ?? false,
        result: json['result'],
        createdAt: json['createdAt'] as int? ?? 0,
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'method': method,
        'title': title,
        if (message != null) 'message': message,
        if (placeholder != null) 'placeholder': placeholder,
        if (prefill != null) 'prefill': prefill,
        if (options.isNotEmpty) 'options': options,
        if (timeout != null) 'timeout': timeout,
        'answered': answered,
        if (result != null) 'result': result,
        'createdAt': createdAt,
      };
}

// ─── 消息 ──────────────────────────────────────────────────────────────────

class ChatMessage {
  const ChatMessage({
    this.id,
    required this.role,
    this.blocks = const [],
    this.model,
    this.timestamp,
    this.toolCallId,
    this.toolName,
    this.output,
    this.isError = false,
    this.extUi,
  });

  final String? id;

  /// user | assistant | system | toolResult
  final String role;
  final List<ChatBlock> blocks;
  final String? model;
  final int? timestamp;

  // toolResult 平级消息字段（快照里工具结果独立成条，需折叠进对应工具块）。
  final String? toolCallId;
  final String? toolName;
  final String? output;
  final bool isError;

  /// 扩展 UI 卡片（role=system 且带 extUi 的消息）。
  final ChatExtUi? extUi;

  /// 正文聚合文本（渲染用；无正文时为空）。
  String get text {
    final sb = StringBuffer();
    for (final b in blocks) {
      if (b is TextBlock) sb.write(b.text);
    }
    return sb.toString();
  }

  bool get hasThinking => blocks.any((b) => b is ThinkingBlock);
  bool get hasImages => blocks.any((b) => b is ImageBlock);

  factory ChatMessage.fromSnapshotJson(Map<String, dynamic> json) {
    final role = json['role'] as String? ?? 'user';
    final extUiRaw = json['extUi'];
    return ChatMessage(
      id: json['id'] as String?,
      role: role,
      blocks: parseContentBlocks(json['content']),
      model: json['model'] as String?,
      timestamp: json['timestamp'] is int ? json['timestamp'] as int : null,
      toolCallId: json['toolCallId'] as String?,
      toolName: json['toolName'] as String?,
      output: json['output'] is String ? json['output'] as String : null,
      isError: json['isError'] as bool? ?? false,
      extUi: extUiRaw is Map<String, dynamic> ? ChatExtUi.fromJson(extUiRaw) : null,
    );
  }
}

/// content 可能是字符串或 block 数组。
List<ChatBlock> parseContentBlocks(dynamic content) {
  if (content == null) return const [];
  if (content is String) {
    return content.isEmpty ? const [] : [TextBlock(content)];
  }
  if (content is List) {
    final blocks = <ChatBlock>[];
    for (final raw in content) {
      if (raw is! Map<String, dynamic>) continue;
      switch (raw['type']) {
        case 'text':
          final t = raw['text'];
          if (t is String && t.isNotEmpty) blocks.add(TextBlock(t));
        case 'thinking':
          final t = raw['thinking'];
          if (t is String && t.isNotEmpty) blocks.add(ThinkingBlock(t));
        case 'image':
          blocks.add(ImageBlock(
            data: raw['data'] as String? ?? '',
            mimeType: raw['mimeType'] as String? ?? 'image/png',
          ));
        case 'toolCall':
          blocks.add(ToolCallBlock(
            id: raw['id'] as String? ?? '',
            name: raw['name'] as String? ?? '',
            args: (raw['arguments'] as Map<String, dynamic>?) ?? const {},
          ));
      }
    }
    return blocks;
  }
  return const [];
}

/// content → 纯文本（流式缓冲 / 预览用，thinking 块拼接但加分隔）。
String chatContentToText(dynamic content) {
  if (content == null) return '';
  if (content is String) return content;
  if (content is List) {
    final parts = <String>[];
    for (final raw in content) {
      if (raw is! Map<String, dynamic>) continue;
      switch (raw['type']) {
        case 'text':
          final t = raw['text'];
          if (t is String) parts.add(t);
        case 'thinking':
          final t = raw['thinking'];
          if (t is String) parts.add(t);
      }
    }
    return parts.join('\n');
  }
  return '';
}

// ─── 会话 / 项目分组（GET /api/sessions + sessions_list）──────────────────

class SessionInfo {
  const SessionInfo({
    required this.id,
    this.label = '',
    this.createdAt = '',
    this.filePath = '',
    this.updatedAt = 0,
    this.preview = '',
    this.cwd = '',
    this.instanceId,
    this.state = 'idle',
    this.model,
    this.modelProvider,
    this.thinkingLevel,
    this.messageCount = 0,
    this.messageSeq = 0,
    this.pinned = 0,
  });

  final String id;
  final String label;
  final String createdAt;
  final String filePath;
  final int updatedAt;
  final String preview;
  final String cwd;
  final String? instanceId;
  final String state;
  final String? model;
  final String? modelProvider;
  final String? thinkingLevel;
  final int messageCount;
  final int messageSeq;
  final int pinned;

  /// 操作用的运行时 id（instanceId ?? id）。
  String get runtimeId => instanceId ?? id;

  bool get busy => state == 'busy';

  factory SessionInfo.fromJson(Map<String, dynamic> json) => SessionInfo(
        id: json['id'] as String? ?? '',
        label: json['label'] as String? ?? '',
        createdAt: json['createdAt'] as String? ?? '',
        filePath: json['filePath'] as String? ?? '',
        updatedAt: json['updatedAt'] as int? ?? 0,
        preview: json['preview'] as String? ?? '',
        cwd: json['cwd'] as String? ?? '',
        instanceId: json['instanceId'] as String?,
        state: json['state'] as String? ?? 'idle',
        model: json['model'] as String?,
        modelProvider: json['modelProvider'] as String?,
        thinkingLevel: json['thinkingLevel'] as String?,
        messageCount: json['messageCount'] as int? ?? 0,
        messageSeq: json['messageSeq'] as int? ?? 0,
        pinned: json['pinned'] as int? ?? 0,
      );
}

/// 项目分组（会话树顶层）。
class ProjectGroup {
  const ProjectGroup({
    required this.id,
    this.path = '',
    this.name = '',
    this.projectType = '',
    this.pinned = 0,
    this.archived = false,
    this.sessions = const [],
  });

  final String id;
  final String path;
  final String name;
  final String projectType;
  final int pinned;
  final bool archived;
  final List<SessionInfo> sessions;

  factory ProjectGroup.fromJson(Map<String, dynamic> json) => ProjectGroup(
        id: json['id'] as String? ?? '',
        path: json['path'] as String? ?? '',
        name: json['name'] as String? ?? '',
        projectType: json['projectType'] as String? ?? '',
        pinned: json['pinned'] as int? ?? 0,
        archived: json['archived'] as bool? ?? false,
        sessions: (json['sessions'] as List<dynamic>? ?? const [])
            .whereType<Map<String, dynamic>>()
            .map(SessionInfo.fromJson)
            .toList(),
      );
}

// ─── 搜索（GET /api/search）────────────────────────────────────────────────

class SearchHit {
  const SearchHit({
    this.sessionId = '',
    this.projectName = '',
    this.label = '',
    this.role = '',
    this.snippet = '',
    this.entryId,
    this.timestamp = 0,
  });

  final String sessionId;
  final String projectName;
  final String label;
  final String role;
  final String snippet;
  final String? entryId;
  final int timestamp;

  factory SearchHit.fromJson(Map<String, dynamic> json) => SearchHit(
        sessionId: json['sessionId'] as String? ?? '',
        projectName: json['projectName'] as String? ?? '',
        label: json['label'] as String? ?? '',
        role: json['role'] as String? ?? '',
        snippet: json['snippet'] as String? ?? '',
        entryId: json['entryId'] as String?,
        timestamp: json['timestamp'] as int? ?? 0,
      );
}

// ─── 预算（GET /api/budget/status）────────────────────────────────────────

class BudgetStatus {
  const BudgetStatus({
    this.used = 0,
    this.budget = 0,
    this.percent = 0,
    this.tier = 0,
    this.resetDay = '',
    this.cycleStart = 0,
    this.cycleEnd = 0,
  });

  final double used;
  final double budget;
  final double percent;
  final int tier;
  final String resetDay;
  final int cycleStart;
  final int cycleEnd;

  factory BudgetStatus.fromJson(Map<String, dynamic> json) => BudgetStatus(
        used: (json['used'] as num?)?.toDouble() ?? 0,
        budget: (json['budget'] as num?)?.toDouble() ?? 0,
        percent: (json['percent'] as num?)?.toDouble() ?? 0,
        tier: json['tier'] as int? ?? 0,
        resetDay: json['resetDay'] as String? ?? '',
        cycleStart: json['cycleStart'] as int? ?? 0,
        cycleEnd: json['cycleEnd'] as int? ?? 0,
      );
}

// ─── 模型（GET /api/pi/model-catalog + /api/rpc get_available_models）──────

class ModelInfo {
  const ModelInfo({required this.id, this.provider = '', this.supportsImage = false});

  final String id;
  final String provider;
  final bool supportsImage;

  String get display => provider.isEmpty ? id : '$id · $provider';

  factory ModelInfo.fromCatalogJson(Map<String, dynamic> json) => ModelInfo(
        id: json['id'] as String? ?? '',
        provider: json['provider'] as String? ?? '',
        supportsImage: (json['input'] as List<dynamic>? ?? const [])
            .whereType<String>()
            .any((e) => e.contains('image')),
      );

  /// /api/rpc get_available_models 的条目形态。
  factory ModelInfo.fromRpcJson(Map<String, dynamic> json) => ModelInfo(
        id: json['id'] as String? ?? json['model'] as String? ?? '',
        provider: json['provider'] as String? ?? '',
        supportsImage: json['supportsImage'] as bool? ?? false,
      );
}

/// GET /api/pi/settings 的默认模型信息。
class PiSettings {
  const PiSettings({
    this.defaultModel = '',
    this.defaultProvider = '',
    this.defaultThinkingLevel,
  });

  final String defaultModel;
  final String defaultProvider;
  final int? defaultThinkingLevel;

  factory PiSettings.fromJson(Map<String, dynamic> json) => PiSettings(
        defaultModel: json['default_model'] as String? ?? '',
        defaultProvider: json['default_provider'] as String? ?? '',
        defaultThinkingLevel: json['default_thinking_level'] as int?,
      );
}

// ─── 斜杠命令（get_commands 响应）────────────────────────────────────────

class SlashCommand {
  const SlashCommand({
    required this.name,
    this.description = '',
    this.source = 'prompt',
    this.sourceInfo = '',
  });

  final String name;
  final String description;

  /// extension | prompt | skill
  final String source;
  final String sourceInfo;

  factory SlashCommand.fromJson(Map<String, dynamic> json) => SlashCommand(
        name: json['name'] as String? ?? '',
        description: json['description'] as String? ?? '',
        source: json['source'] as String? ?? 'prompt',
        sourceInfo: json['sourceInfo'] as String? ?? '',
      );
}
