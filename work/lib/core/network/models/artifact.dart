/// 产物（Artifact）模型，严格对齐 mock-contract.md §1.2。
library;

/// 快照 diff 状态。
enum ArtifactOp {
  newFile('new'),
  modified('modified'),
  deleted('deleted');

  const ArtifactOp(this.wire);
  final String wire;

  static ArtifactOp fromWire(String? value) => switch (value) {
        'modified' => ArtifactOp.modified,
        'deleted' => ArtifactOp.deleted,
        _ => ArtifactOp.newFile,
      };
}

/// 产物来源。
enum ArtifactSource {
  snapshot('snapshot'),
  live('live');

  const ArtifactSource(this.wire);
  final String wire;

  static ArtifactSource fromWire(String? value) =>
      value == 'live' ? ArtifactSource.live : ArtifactSource.snapshot;
}

/// Artifact（产物条目，按 turn 分组）。
class Artifact {
  const Artifact({
    required this.id,
    required this.workspaceId,
    required this.sessionId,
    required this.turnId,
    required this.path,
    required this.op,
    required this.size,
    this.linesAdded = 0,
    this.linesDeleted = 0,
    required this.source,
    required this.deliverable,
    required this.createdAt,
  });

  final String id;
  final String workspaceId;
  final String sessionId;

  /// 消息 seq / turn 序号。
  final int turnId;
  final String path;
  final ArtifactOp op;
  final int size;

  /// 增删行数（new=全量 / modified=净变化 / deleted=全量删除）。
  final int linesAdded;
  final int linesDeleted;
  final ArtifactSource source;

  /// output/ 内 ∪ save_artifact ∪ 手动标记。
  final bool deliverable;
  final DateTime createdAt;

  factory Artifact.fromJson(Map<String, dynamic> json) => Artifact(
        id: json['id'] as String,
        workspaceId: json['workspaceId'] as String? ?? '',
        sessionId: json['sessionId'] as String? ?? '',
        turnId: json['turnId'] as int? ?? 0,
        path: json['path'] as String,
        op: ArtifactOp.fromWire(json['op'] as String?),
        size: json['size'] as int? ?? 0,
        linesAdded: json['linesAdded'] as int? ?? 0,
        linesDeleted: json['linesDeleted'] as int? ?? 0,
        source: ArtifactSource.fromWire(json['source'] as String?),
        deliverable: json['deliverable'] as bool? ?? false,
        createdAt: json['createdAt'] is int
            ? DateTime.fromMillisecondsSinceEpoch(json['createdAt'] as int)
            : DateTime.fromMillisecondsSinceEpoch(0),
      );

  Map<String, dynamic> toJson() => {
        'id': id,
        'workspaceId': workspaceId,
        'sessionId': sessionId,
        'turnId': turnId,
        'path': path,
        'op': op.wire,
        'size': size,
        'linesAdded': linesAdded,
        'linesDeleted': linesDeleted,
        'source': source.wire,
        'deliverable': deliverable,
        'createdAt': createdAt.millisecondsSinceEpoch,
      };
}

/// turn_artifacts 事件 items 条目（无 id 的轻量形态，对齐 mock-contract §3.3）。
class TurnArtifactItem {
  const TurnArtifactItem({
    required this.path,
    required this.op,
    required this.size,
    this.linesAdded = 0,
    this.linesDeleted = 0,
    required this.deliverable,
  });

  final String path;
  final ArtifactOp op;
  final int size;
  final int linesAdded;
  final int linesDeleted;
  final bool deliverable;

  factory TurnArtifactItem.fromJson(Map<String, dynamic> json) => TurnArtifactItem(
        path: json['path'] as String,
        op: ArtifactOp.fromWire(json['op'] as String?),
        size: json['size'] as int? ?? 0,
        linesAdded: json['linesAdded'] as int? ?? 0,
        linesDeleted: json['linesDeleted'] as int? ?? 0,
        deliverable: json['deliverable'] as bool? ?? false,
      );

  Map<String, dynamic> toJson() => {
        'path': path,
        'op': op.wire,
        'size': size,
        'linesAdded': linesAdded,
        'linesDeleted': linesDeleted,
        'deliverable': deliverable,
      };
}

/// 按 turn 分组的产物（GET /api/workspaces/:id/artifacts 返回 turns，新→旧）。
class ArtifactTurn {
  const ArtifactTurn({
    required this.turnId,
    required this.createdAt,
    required this.items,
  });

  final int turnId;
  final DateTime createdAt;
  final List<Artifact> items;

  factory ArtifactTurn.fromJson(Map<String, dynamic> json) => ArtifactTurn(
        turnId: json['turnId'] as int? ?? 0,
        createdAt: json['createdAt'] is int
            ? DateTime.fromMillisecondsSinceEpoch(json['createdAt'] as int)
            : DateTime.fromMillisecondsSinceEpoch(0),
        items: (json['items'] as List<dynamic>? ?? const [])
            .map((e) => Artifact.fromJson(e as Map<String, dynamic>))
            .toList(),
      );

  Map<String, dynamic> toJson() => {
        'turnId': turnId,
        'createdAt': createdAt.millisecondsSinceEpoch,
        'items': items.map((e) => e.toJson()).toList(),
      };
}
