/// mDNS 服务发现 IO 端（bonsoir 插件）：浏览 `_piter._tcp`。
///
/// 服务端以 `mdns-sd` 广播（crates/pi_server/src/gateway/mdns.rs），
/// TXT 记录 port/proto/name（mock-contract §4）。App 端浏览应答 →
/// 解析 IP/端口/TXT → DiscoveredServer，connection 页一键连接。
library;

import 'package:bonsoir/bonsoir.dart';

import '../../config/server_config.dart';
import 'discovery.dart';

class BonsoirDiscoveryService implements DiscoveryService {
  BonsoirDiscovery? _discovery;

  /// 已推送过的服务器 key（host:port），去重——bonsoir 可能对同一服务
  /// 多次 resolved（网络波动/重广播）。
  final Set<String> _seen = {};

  @override
  Stream<DiscoveredServer> browse() async* {
    final discovery = BonsoirDiscovery(type: '_piter._tcp');
    _discovery = discovery;
    await discovery.initialize();
    await discovery.start();

    yield* discovery.eventStream!
        // NSD/DNS-SD 的 found 事件不含 IP/端口——必须对每个发现的服务显式
        // resolve（bonsoir 不会自动 resolve），resolved 事件才带 host/port。
        .where((e) =>
            e is BonsoirDiscoveryServiceFoundEvent ||
            e is BonsoirDiscoveryServiceResolvedEvent)
        .map((e) {
          if (e is BonsoirDiscoveryServiceFoundEvent) {
            final service = e.service;
            discovery.serviceResolver.resolveService(service);
          }
          return e;
        })
        .where((e) => e is BonsoirDiscoveryServiceResolvedEvent)
        .map((e) => (e as BonsoirDiscoveryServiceResolvedEvent).service)
        .map(_toDiscovered)
        .where((d) => d != null && _seen.add(d.server.baseUrl))
        .cast<DiscoveredServer>();
  }

  @override
  void dispose() {
    _discovery?.stop();
    _discovery = null;
  }
}

DiscoveredServer? _toDiscovered(BonsoirService s) {
  // 7.x：host 拆分为 hostAddress（IP，首个）与 hostname（mDNS 主机名，带
  // 尾点）。Android（NSD）返回 IP；iOS（DNSServiceResolve）返回主机名。
  var host = s.hostAddress ?? s.hostname ?? '';
  // 统一去尾点，保留原样用于拼 URL。
  while (host.endsWith('.')) {
    host = host.substring(0, host.length - 1);
  }
  if (host.isEmpty || s.port <= 0) return null;
  final txt = s.attributes;
  // TXT `name` 为服务端可读实例名（如 "Yoa 的 Piter"），优先于服务实例名。
  final displayName = (txt['name']?.trim().isNotEmpty ?? false) ? txt['name']! : s.name;
  final base = 'http://$host:${s.port}';
  return DiscoveredServer(
    server: ServerInfo(
      id: '',
      name: displayName,
      baseUrl: base,
      wsUrl: '${base.replaceFirst(RegExp(r'^http'), 'ws')}/ws',
    ),
    txRecord: txt,
  );
}

DiscoveryService createDiscoveryService() => BonsoirDiscoveryService();
