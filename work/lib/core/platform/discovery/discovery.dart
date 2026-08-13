/// 平台能力桥：mDNS 服务发现（discovery）抽象。
///
/// 服务类型 `_piter._tcp`，TXT：port/proto/name（mock-contract §4）。
/// App 端（IO）用 bonsoir 插件浏览；Web 端（浏览器禁组播）用 HTTP 探测替代；
/// 工厂经条件导入选择实现，未来替换平台实现时 UI 零改动。
library;

import '../../config/server_config.dart';
import 'discovery_stub.dart'
    if (dart.library.io) 'discovery_io.dart'
    if (dart.library.html) 'discovery_web.dart' as impl;

/// 发现到的一台可连接服务器。
class DiscoveredServer {
  const DiscoveredServer({required this.server, this.txRecord = const {}});

  final ServerInfo server;

  /// mDNS TXT 记录（port/proto/name）。
  final Map<String, String> txRecord;
}

/// mDNS 服务发现接口。
abstract class DiscoveryService {
  /// 开始浏览，返回增量发现的服务器流（本阶段 stub 为空流）。
  Stream<DiscoveredServer> browse();

  void dispose();
}

/// 创建平台实现。
DiscoveryService createDiscoveryService() => impl.createDiscoveryService();
