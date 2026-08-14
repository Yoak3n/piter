/// 详情页·文件树面板：文件树 + 上传/下载。
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../core/network/api_client.dart';
import '../../../core/platform/browser_io.dart';
import '../providers/data_sources.dart';
import '../providers/workspace_detail_provider.dart';
import '../widgets/file_tree.dart';

class FileTreePanel extends ConsumerStatefulWidget {
  const FileTreePanel({super.key, required this.workspaceId});

  final String workspaceId;

  @override
  ConsumerState<FileTreePanel> createState() => _FileTreePanelState();
}

class _FileTreePanelState extends ConsumerState<FileTreePanel> {
  bool _uploading = false;

  Future<void> _upload() async {
    final messenger = ScaffoldMessenger.of(context);
    try {
      final picked = await pickFiles();
      if (picked.isEmpty) return;
      setState(() => _uploading = true);
      final api = ref.read(apiClientProvider);
      final result = await api.uploadFiles(widget.workspaceId, [
        for (final f in picked) UploadFile(name: f.name, bytes: f.bytes),
      ]);
      // 刷新文件树（上传内容已并入快照基线，不误报产物）。
      ref.invalidate(workspaceDetailProvider(widget.workspaceId));
      if (result.uploaded.isNotEmpty) {
        messenger.showSnackBar(SnackBar(content: Text('已上传 ${result.uploaded.length} 个文件')));
      }
      if (result.rejected.isNotEmpty) {
        final reasons = result.rejected
            .map((r) => '${r.path}（${r.reason}）')
            .join('\n');
        messenger.showSnackBar(SnackBar(content: Text('部分文件被拒绝：\n$reasons')));
      }
    } catch (e) {
      messenger.showSnackBar(SnackBar(content: Text('上传失败：$e')));
    } finally {
      if (mounted) setState(() => _uploading = false);
    }
  }

  Future<void> _download(String path) async {
    final messenger = ScaffoldMessenger.of(context);
    try {
      final api = ref.read(apiClientProvider);
      final bytes = await api.downloadFile(widget.workspaceId, path);
      final name = path.split('/').last;
      saveBytes(bytes, name);
      messenger.showSnackBar(SnackBar(content: Text('已开始下载 $name')));
    } catch (e) {
      messenger.showSnackBar(SnackBar(content: Text('下载失败：$e')));
    }
  }

  @override
  Widget build(BuildContext context) {
    final detail = ref.watch(workspaceDetailProvider(widget.workspaceId));
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 12, 8, 4),
          child: Row(
            children: [
              Text('文件', style: Theme.of(context).textTheme.labelLarge),
              const Spacer(),
              // 手动刷新（agent 新增/修改文件后，未触发 turn_artifacts 时兜底）。
              IconButton(
                onPressed: () =>
                    ref.invalidate(workspaceDetailProvider(widget.workspaceId)),
                icon: const Icon(Icons.refresh),
                tooltip: '刷新文件列表',
              ),
              IconButton(
                onPressed: _uploading ? null : _upload,
                icon: _uploading
                    ? const SizedBox(
                        width: 16,
                        height: 16,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      )
                    : const Icon(Icons.upload_outlined),
                tooltip: '上传文件',
              ),
            ],
          ),
        ),
        const Divider(height: 1),
        Expanded(
          child: detail.when(
            loading: () => const Center(child: CircularProgressIndicator()),
            error: (e, _) => Center(child: Text('加载失败：$e')),
            data: (d) => FileTree(files: d.files.files, onDownload: _download),
          ),
        ),
      ],
    );
  }
}
