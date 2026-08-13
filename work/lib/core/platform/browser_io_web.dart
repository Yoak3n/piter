/// 浏览器文件选择与保存（Web 端实现）。
///
/// 仅在 Web 目标编译（`dart.library.html`）；非 Web 端由
/// browser_io_stub.dart 兜底（抛 UnsupportedError）。
library;

import 'dart:async';
import 'dart:js_interop';
import 'dart:typed_data';

import 'package:flutter/foundation.dart' show kIsWeb;
import 'package:web/web.dart' as web;

/// 选中的一个文件（原始字节 + 文件名）。
class PickedFile {
  const PickedFile({required this.name, required this.bytes});

  final String name;
  final Uint8List bytes;
}

/// 打开系统文件选择（多选），返回选中文件；用户取消 → 空列表。
Future<List<PickedFile>> pickFiles() async {
  if (!kIsWeb) throw UnsupportedError('仅 Web 端支持文件上传');
  final completer = Completer<List<PickedFile>>();
  final input = web.HTMLInputElement()
    ..type = 'file'
    ..multiple = true;
  input.addEventListener('change', ((web.Event _) {
    final files = input.files;
    if (files == null) {
      if (!completer.isCompleted) completer.complete(const []);
      return;
    }
    _readFiles(files).then((out) {
      if (!completer.isCompleted) completer.complete(out);
    });
  }).toJS);
  input.click();
  return completer.future;
}

Future<List<PickedFile>> _readFiles(web.FileList files) async {
  final out = <PickedFile>[];
  for (var i = 0; i < files.length; i++) {
    final f = files.item(i);
    if (f == null) continue;
    final buf = await f.arrayBuffer().toDart;
    out.add(PickedFile(name: f.name, bytes: buf.toDart.asUint8List()));
  }
  return out;
}

/// 触发浏览器保存（Blob + `<a download>`）。
void saveBytes(Uint8List bytes, String filename) {
  if (!kIsWeb) throw UnsupportedError('仅 Web 端支持文件下载');
  final blob = web.Blob(<JSAny>[bytes.toJS].toJS);
  final url = web.URL.createObjectURL(blob);
  final anchor = web.HTMLAnchorElement()
    ..href = url
    ..download = filename;
  anchor.click();
  web.URL.revokeObjectURL(url);
}
