/// 轻量 unified diff 解析器（渲染 edit 工具 `details.patch`）。
///
/// 支持 `---/+++` 路径头、`@@ -a,b +c,d @@` hunk 头、行内 `+`/`-`/` ` 标记；
/// 输出结构化行供 UI 高亮 add/del。不依赖任何 diff 库。
library;

enum DiffLineKind { header, hunk, context, addition, deletion }

/// diff 中的一行（text 不含标记前缀）。
class DiffLine {
  const DiffLine(this.kind, this.text);

  final DiffLineKind kind;
  final String text;
}

/// 一个 hunk（@@ 块）。
class DiffHunk {
  const DiffHunk({
    required this.oldStart,
    required this.oldCount,
    required this.newStart,
    required this.newCount,
    required this.lines,
  });

  final int oldStart;
  final int oldCount;
  final int newStart;
  final int newCount;
  final List<DiffLine> lines;
}

/// 解析后的 unified diff。
class UnifiedDiff {
  const UnifiedDiff({
    this.oldPath = '',
    this.newPath = '',
    this.hunks = const [],
    this.lines = const [],
  });

  final String oldPath;
  final String newPath;
  final List<DiffHunk> hunks;

  /// 全部行（含 header / hunk 头），用于顺序渲染。
  final List<DiffLine> lines;

  int get additions => lines.where((l) => l.kind == DiffLineKind.addition).length;
  int get deletions => lines.where((l) => l.kind == DiffLineKind.deletion).length;

  bool get isEmpty => hunks.isEmpty;
}

/// 解析 unified diff 文本。非 patch 输入（无 `@@`）返回空 UnifiedDiff。
UnifiedDiff parseUnifiedDiff(String patch) {
  final rawLines = patch.split('\n');
  final oldPathBuf = <String>[];
  final newPathBuf = <String>[];
  final allLines = <DiffLine>[];
  final hunks = <DiffHunk>[];

  DiffHunk? current;
  var currentLines = <DiffLine>[];

  void flush() {
    if (current != null) {
      hunks.add(DiffHunk(
        oldStart: current!.oldStart,
        oldCount: current!.oldCount,
        newStart: current!.newStart,
        newCount: current!.newCount,
        lines: currentLines,
      ));
      currentLines = <DiffLine>[];
      current = null;
    }
  }

  final hunkRe = RegExp(r'^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@');

  for (final raw in rawLines) {
    if (raw.startsWith('--- ')) {
      oldPathBuf.add(_stripPath(raw.substring(4)));
      allLines.add(DiffLine(DiffLineKind.header, raw.substring(4)));
      continue;
    }
    if (raw.startsWith('+++ ')) {
      newPathBuf.add(_stripPath(raw.substring(4)));
      allLines.add(DiffLine(DiffLineKind.header, raw.substring(4)));
      continue;
    }
    final hunkMatch = hunkRe.firstMatch(raw);
    if (hunkMatch != null) {
      flush();
      current = DiffHunk(
        oldStart: int.parse(hunkMatch.group(1)!),
        oldCount: int.tryParse(hunkMatch.group(2) ?? '') ?? 1,
        newStart: int.parse(hunkMatch.group(3)!),
        newCount: int.tryParse(hunkMatch.group(4) ?? '') ?? 1,
        lines: const [],
      );
      allLines.add(DiffLine(DiffLineKind.hunk, raw));
      continue;
    }
    if (raw.startsWith(r'\ ') || raw.startsWith('\\')) {
      // "\ No newline at end of file" 等信息行，忽略（不入 hunk 内容）。
      continue;
    }
    if (raw.isEmpty) {
      // 尾随换行产生的真空行，忽略（context 空行在 unified diff 中为单个空格）。
      continue;
    }

    final kind = _classify(raw);
    // add/del/context 行去掉标记前缀（`+`/`-`/` `），空行视为空 context。
    final text = raw.isEmpty ? '' : raw.substring(1);
    allLines.add(DiffLine(kind, text));
    if (current != null) {
      currentLines.add(DiffLine(kind, text));
    }
  }
  flush();

  return UnifiedDiff(
    oldPath: oldPathBuf.isNotEmpty ? oldPathBuf.first : '',
    newPath: newPathBuf.isNotEmpty ? newPathBuf.first : '',
    hunks: hunks,
    lines: allLines,
  );
}

DiffLineKind _classify(String raw) {
  if (raw.startsWith('+') && !raw.startsWith('+++')) return DiffLineKind.addition;
  if (raw.startsWith('-') && !raw.startsWith('---')) return DiffLineKind.deletion;
  return DiffLineKind.context;
}

String _stripPath(String p) => p.startsWith('a/') || p.startsWith('b/') ? p.substring(2) : p;
