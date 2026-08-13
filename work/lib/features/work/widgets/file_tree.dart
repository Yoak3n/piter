/// 文件树：由扁平 FileEntry 列表折叠为目录树。
library;

import 'package:flutter/material.dart';

import '../../../core/network/models/models.dart';

class FileTreeNode {
  const FileTreeNode({required this.name, this.isDir = false, this.entry, this.children = const []});

  final String name;
  final bool isDir;
  final FileEntry? entry;
  final List<FileTreeNode> children;
}

/// 扁平列表 → 树。
List<FileTreeNode> buildFileTree(List<FileEntry> files) {
  final root = <FileTreeNode>[];
  for (final file in files) {
    final segs = file.path.split('/');
    var level = root;
    for (var i = 0; i < segs.length; i++) {
      final seg = segs[i];
      final isLast = i == segs.length - 1;
      final idx = level.indexWhere((n) => n.name == seg && (isLast ? !n.isDir : n.isDir));
      if (idx >= 0) {
        if (isLast) {
          // 同名冲突时更新 entry
          final node = level[idx];
          if (node.entry == null) level[idx] = FileTreeNode(name: seg, isDir: false, entry: file);
        }
        level = level[idx].children;
      } else {
        if (isLast) {
          final node = FileTreeNode(name: seg, entry: file);
          level.add(node);
          break;
        }
        final dir = FileTreeNode(name: seg, isDir: true);
        level.add(dir);
        level = dir.children;
      }
    }
  }
  return root;
}

/// 折叠目录树。
class FileTree extends StatelessWidget {
  const FileTree({super.key, required this.files, this.onDownload});

  final List<FileEntry> files;

  /// 点击文件行下载按钮回调（参数为文件相对路径）。
  final ValueChanged<String>? onDownload;

  @override
  Widget build(BuildContext context) {
    final tree = buildFileTree(files);
    if (tree.isEmpty) {
      return Padding(
        padding: const EdgeInsets.all(16),
        child: Text('暂无文件', style: Theme.of(context).textTheme.bodySmall),
      );
    }
    return ListView.builder(
      padding: const EdgeInsets.symmetric(vertical: 8),
      itemCount: tree.length,
      itemBuilder: (context, i) => _TreeNodeTile(node: tree[i], depth: 0, onDownload: onDownload),
    );
  }
}

class _TreeNodeTile extends StatefulWidget {
  const _TreeNodeTile({required this.node, required this.depth, this.onDownload});

  final FileTreeNode node;
  final int depth;
  final ValueChanged<String>? onDownload;

  @override
  State<_TreeNodeTile> createState() => _TreeNodeTileState();
}

class _TreeNodeTileState extends State<_TreeNodeTile> {
  bool _expanded = false;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final node = widget.node;
    final indent = EdgeInsets.only(left: 8.0 * widget.depth);

    if (!node.isDir) {
      final entry = node.entry!;
      return Padding(
        padding: const EdgeInsets.symmetric(vertical: 2).add(indent),
        child: Row(
          children: [
            const SizedBox(width: 16),
            Icon(Icons.insert_drive_file_outlined, size: 16, color: scheme.outline),
            const SizedBox(width: 8),
            Expanded(
              child: Text(
                node.name,
                style: Theme.of(context).textTheme.bodySmall,
                overflow: TextOverflow.ellipsis,
              ),
            ),
            if (entry.isDeliverable)
              Icon(Icons.star, size: 13, color: scheme.tertiary),
            const SizedBox(width: 4),
            Text(
              _formatSize(entry.size),
              style: Theme.of(context).textTheme.labelSmall?.copyWith(color: scheme.outline),
            ),
            if (widget.onDownload != null)
              IconButton(
                onPressed: () => widget.onDownload!(entry.path),
                icon: Icon(Icons.download_outlined, size: 15, color: scheme.primary),
                visualDensity: VisualDensity.compact,
                tooltip: '下载',
                padding: const EdgeInsets.all(2),
              ),
            const SizedBox(width: 8),
          ],
        ),
      );
    }

    return Column(
      children: [
        InkWell(
          onTap: () => setState(() => _expanded = !_expanded),
          child: Padding(
            padding: const EdgeInsets.symmetric(vertical: 4).add(indent),
            child: Row(
              children: [
                Icon(
                  _expanded ? Icons.folder_open : Icons.folder_outlined,
                  size: 16,
                  color: scheme.primary,
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: Text(
                    node.name,
                    style: Theme.of(context)
                        .textTheme
                        .bodySmall
                        ?.copyWith(color: scheme.primary),
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
                Text(
                  '${node.children.length}',
                  style: Theme.of(context).textTheme.labelSmall?.copyWith(color: scheme.outline),
                ),
                const SizedBox(width: 16),
              ],
            ),
          ),
        ),
        if (_expanded)
          for (final child in node.children)
            _TreeNodeTile(node: child, depth: widget.depth + 1),
      ],
    );
  }
}

String _formatSize(int bytes) {
  if (bytes < 1024) return '$bytes B';
  if (bytes < 1024 * 1024) return '${(bytes / 1024).toStringAsFixed(1)} KB';
  return '${(bytes / (1024 * 1024)).toStringAsFixed(1)} MB';
}
