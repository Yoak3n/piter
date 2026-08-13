/// 浏览器文件选择与保存（平台分发）。
///
/// Web 目标（JS 运行时）用真实实现 browser_io_web.dart；
/// 其他目标（VM/App 测试等）用 stub（抛 UnsupportedError）。
/// 条件用 `dart.library.html`：仅 Web JS 构建为 true，flutter test（VM）
/// 不会把 package:web（含 dart:js_interop 的 http helper）编进测试。
library;

export 'browser_io_stub.dart' if (dart.library.html) 'browser_io_web.dart';
