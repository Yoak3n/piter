/// 工作空间模型，严格对齐 mock-contract.md §1.2。
library;

/// 写边界模式（Workspace.mode）：ask=每次询问 / allow=放行 / deny=拒绝。
enum WorkspaceMode {
  ask('ask'),
  allow('allow'),
  deny('deny');

  const WorkspaceMode(this.wire);

  /// 线上（JSON）表示，snake_case 字符串。
  final String wire;

  static WorkspaceMode fromWire(String? value) => switch (value) {
        'allow' => WorkspaceMode.allow,
        'deny' => WorkspaceMode.deny,
        _ => WorkspaceMode.ask,
      };
}

/// Workspace（= projects 表 type='workspace' 的行）。
class Workspace {
  const Workspace({
    required this.id,
    required this.name,
    required this.cwd,
    required this.createdAt,
    required this.updatedAt,
    required this.fileCount,
    required this.sizeBytes,
    this.mode = WorkspaceMode.ask,
  });

  /// projects.id
  final String id;
  final String name;

  /// real_dir（磁盘工作目录）
  final String cwd;
  final DateTime createdAt;
  final DateTime updatedAt;
  final int fileCount;
  final int sizeBytes;
  final WorkspaceMode mode;

  Workspace copyWith({
    String? name,
    DateTime? updatedAt,
    int? fileCount,
    int? sizeBytes,
    WorkspaceMode? mode,
  }) =>
      Workspace(
        id: id,
        name: name ?? this.name,
        cwd: cwd,
        createdAt: createdAt,
        updatedAt: updatedAt ?? this.updatedAt,
        fileCount: fileCount ?? this.fileCount,
        sizeBytes: sizeBytes ?? this.sizeBytes,
        mode: mode ?? this.mode,
      );

  factory Workspace.fromJson(Map<String, dynamic> json) => Workspace(
        id: json['id'] as String,
        name: json['name'] as String,
        cwd: json['cwd'] as String? ?? '',
        createdAt: _dateTimeFromJson(json['createdAt']),
        updatedAt: _dateTimeFromJson(json['updatedAt']),
        fileCount: json['fileCount'] as int? ?? 0,
        sizeBytes: json['sizeBytes'] as int? ?? 0,
        mode: WorkspaceMode.fromWire(json['mode'] as String?),
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'name': name,
        'cwd': cwd,
        'createdAt': createdAt.millisecondsSinceEpoch,
        'updatedAt': updatedAt.millisecondsSinceEpoch,
        'fileCount': fileCount,
        'sizeBytes': sizeBytes,
        'mode': mode.wire,
      };
}

/// 时间字段兼容两种形态：epoch ms（契约基线）或 RFC3339 字符串（后端 projects 表）。
DateTime _dateTimeFromJson(dynamic value) {
  if (value is int) return DateTime.fromMillisecondsSinceEpoch(value);
  if (value is String) return DateTime.tryParse(value) ?? DateTime.fromMillisecondsSinceEpoch(0);
  return DateTime.fromMillisecondsSinceEpoch(0);
}
