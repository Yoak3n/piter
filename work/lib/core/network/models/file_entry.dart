/// 目录树节点模型，严格对齐 mock-contract.md §1.2。
library;

enum FileEntryType {
  file('file'),
  dir('dir');

  const FileEntryType(this.wire);
  final String wire;

  static FileEntryType fromWire(String? value) =>
      value == 'dir' ? FileEntryType.dir : FileEntryType.file;
}

/// FileEntry（目录树节点，扁平列表含相对路径）。
class FileEntry {
  const FileEntry({
    required this.path,
    required this.type,
    required this.size,
    required this.mtime,
    this.isDeliverable = false,
  });

  /// 相对 real_dir 的路径，如 `src/main.rs`。
  final String path;
  final FileEntryType type;
  final int size;
  final DateTime mtime;

  /// 手动标记过交付物则为 true。
  final bool isDeliverable;

  String get fileName {
    final segs = path.split('/');
    return segs.isEmpty ? path : segs.last;
  }

  String get parentPath {
    final idx = path.lastIndexOf('/');
    return idx < 0 ? '' : path.substring(0, idx);
  }

  factory FileEntry.fromJson(Map<String, dynamic> json) => FileEntry(
        path: json['path'] as String,
        type: FileEntryType.fromWire(json['type'] as String?),
        size: json['size'] as int? ?? 0,
        mtime: json['mtime'] is int
            ? DateTime.fromMillisecondsSinceEpoch(json['mtime'] as int)
            : DateTime.fromMillisecondsSinceEpoch(0),
        isDeliverable: json['isDeliverable'] as bool? ?? false,
      );

  Map<String, dynamic> toJson() => {
        'path': path,
        'type': type.wire,
        'size': size,
        'mtime': mtime.millisecondsSinceEpoch,
        'isDeliverable': isDeliverable,
      };
}
