/// 浏览器文件选择与保存（非 Web 端兜底：暂不支持，抛 UnsupportedError）。
library;

import 'dart:typed_data';

/// 选中的一个文件（原始字节 + 文件名）。
class PickedFile {
  const PickedFile({required this.name, required this.bytes});

  final String name;
  final Uint8List bytes;
}

/// 非 Web 端暂不支持文件选择。
Future<List<PickedFile>> pickFiles() async =>
    throw UnsupportedError('文件上传仅支持 Web 端');

/// 非 Web 端暂不支持保存。
void saveBytes(Uint8List bytes, String filename) =>
    throw UnsupportedError('文件下载仅支持 Web 端');
