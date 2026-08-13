/// 手动添加服务器对话框（IP:端口 → baseUrl + wsUrl）。
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../core/config/server_config.dart';
import 'providers/servers_provider.dart';

/// 展示对话框；添加成功返回 true。
Future<bool?> showAddServerDialog(BuildContext context) {
  return showDialog<bool>(
    context: context,
    builder: (_) => const AddServerDialog(),
  );
}

class AddServerDialog extends ConsumerStatefulWidget {
  const AddServerDialog({super.key});

  @override
  ConsumerState<AddServerDialog> createState() => _AddServerDialogState();
}

class _AddServerDialogState extends ConsumerState<AddServerDialog> {
  final _name = TextEditingController();
  final _host = TextEditingController();
  final _port = TextEditingController(text: '31421');
  bool _submitting = false;

  @override
  void dispose() {
    _name.dispose();
    _host.dispose();
    _port.dispose();
    super.dispose();
  }

  Future<void> _submit() async {
    final host = _host.text.trim();
    final port = int.tryParse(_port.text.trim());
    if (host.isEmpty || port == null) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('请填写有效的 IP 和端口')),
      );
      return;
    }
    setState(() => _submitting = true);
    try {
      final name = _name.text.trim().isEmpty ? host : _name.text.trim();
      await ref.read(serversProvider.notifier).addServer(
            name: name,
            baseUrl: 'http://$host:$port',
          );
      if (mounted) Navigator.of(context).pop(true);
    } finally {
      if (mounted) setState(() => _submitting = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('手动添加服务器'),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          TextField(
            controller: _name,
            decoration: const InputDecoration(labelText: '名称（可选）', hintText: '例如：书房 Piter'),
          ),
          const SizedBox(height: 8),
          TextField(
            controller: _host,
            autofocus: true,
            keyboardType: TextInputType.number,
            decoration: const InputDecoration(labelText: 'IP 地址', hintText: '192.168.1.5'),
          ),
          const SizedBox(height: 8),
          TextField(
            controller: _port,
            keyboardType: TextInputType.number,
            decoration: const InputDecoration(labelText: '端口', hintText: '31421'),
          ),
        ],
      ),
      actions: [
        TextButton(
          onPressed: _submitting ? null : () => Navigator.of(context).pop(false),
          child: const Text('取消'),
        ),
        FilledButton(
          onPressed: _submitting ? null : _submit,
          child: _submitting
              ? const SizedBox(
                  width: 16,
                  height: 16,
                  child: CircularProgressIndicator(strokeWidth: 2),
                )
              : const Text('添加'),
        ),
      ],
    );
  }
}

/// 校验手动输入的服务器地址（供连接逻辑复用）。
ServerInfo? serverFromInput({required String host, required int port, String? name}) {
  if (host.isEmpty) return null;
  return ServerInfo(
    id: '',
    name: (name == null || name.isEmpty) ? host : name,
    baseUrl: 'http://$host:$port',
    wsUrl: 'ws://$host:$port/ws',
  );
}
