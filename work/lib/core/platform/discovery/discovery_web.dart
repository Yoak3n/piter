/// mDNS 服务发现 Web 端 stub。
///
/// 浏览器环境无 mDNS 权限（禁组播场景），本阶段返回空流并提示降级；
/// 后续如接入 WebRTC/网关代理方案在此替换实现。
library;

import 'discovery.dart';

class WebDiscoveryService implements DiscoveryService {
  const WebDiscoveryService();

  @override
  Stream<DiscoveredServer> browse() => const Stream.empty();

  @override
  void dispose() {}
}

DiscoveryService createDiscoveryService() => const WebDiscoveryService();
