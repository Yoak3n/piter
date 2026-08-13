/// mDNS 服务发现 IO 端 stub。
///
/// 本阶段不接真实插件（bonsoir 留到后续阶段）：返回空流，
/// 由 connection 模块手动添加服务器作为兜底。
library;

import 'discovery.dart';

class StubDiscoveryService implements DiscoveryService {
  const StubDiscoveryService();

  @override
  Stream<DiscoveredServer> browse() => const Stream.empty();

  @override
  void dispose() {}
}

DiscoveryService createDiscoveryService() => const StubDiscoveryService();
