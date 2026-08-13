/// 连接页：服务器管理（发现 / 手动添加 / 多服务端列表 / 当前切换）。
///
/// 三入口：mDNS 发现列表（本阶段 stub，见 core/platform/discovery）+
/// 扫码（后续阶段） + 手动 IP:端口（已实现）。
library;

import 'package:dio/dio.dart';
import 'package:flutter/foundation.dart' show kIsWeb;
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../core/config/server_config.dart';
import '../../../core/network/probe.dart';
import '../../../core/platform/discovery/discovery.dart';
import 'add_server_dialog.dart';
import 'providers/capability_provider.dart';
import 'providers/servers_provider.dart';

// dart:async 的 StreamSubscription（mDNS 浏览订阅）
import 'dart:async';

class ConnectionPage extends ConsumerStatefulWidget {
  const ConnectionPage({super.key});

  @override
  ConsumerState<ConnectionPage> createState() => _ConnectionPageState();
}

class _ConnectionPageState extends ConsumerState<ConnectionPage> {
  DiscoveryService? _discovery;
  StreamSubscription<DiscoveredServer>? _browseSub;
  Timer? _browseTimer;
  List<DiscoveredServer> _discovered = const [];
  bool _discovering = false;

  @override
  void initState() {
    super.initState();
    _discovery = createDiscoveryService();
    if (kIsWeb) {
      // 浏览器无法发 UDP 组播（禁组播场景）——用 HTTP 探测替代 mDNS：
      // 只探测同源来源（Flutter Web 由服务端托管时，即当前服务端）。
      _httpDiscover();
    } else {
      _startMdnsBrowse();
    }
  }

  /// App 端 mDNS 浏览（bonsoir）：`_piter._tcp` → 增量发现列表。
  ///
  /// mDNS 浏览是持续过程（不会主动结束），"探测中"不能一直挂着：
  /// 首个结果到达或 8s 超时后结束转圈；后续事件继续增量更新列表。
  Future<void> _startMdnsBrowse() async {
    setState(() => _discovering = true);
    var settled = false;
    void settle() {
      if (settled || !mounted) return;
      settled = true;
      _browseTimer?.cancel();
      _browseTimer = null;
      setState(() => _discovering = false);
    }

    _browseTimer = Timer(const Duration(seconds: 8), settle);
    _browseSub = _discovery!.browse().listen(
      (d) {
        if (!mounted) return;
        settle();
        setState(() {
          _discovered = [
            ..._discovered.where((x) => x.server.baseUrl != d.server.baseUrl),
            d,
          ];
        });
      },
      onDone: settle,
      onError: (Object e) {
        // mDNS 不可用（禁组播/插件缺失）不致命——手动添加是保底通路。
        settle();
      },
    );
  }

  /// Web 端 HTTP 自动探测（走 /api/health + /api/mdns/status，gateway CORS 放行）。
  ///
  /// 候选顺序：① 同源 origin（页面由 gateway 分发，即当前服务端）；② 同源
  /// 不可达时降级本机默认端口 31421（flutter dev server / 非 gateway 托管场景，
  /// 桌面端同机开发）。均需真实探测可达才展示，不臆造名称。
  Future<void> _httpDiscover() async {
    setState(() => _discovering = true);
    final results = <DiscoveredServer>[];
    final origin = Uri.base.origin;
    if (origin.isNotEmpty && origin != 'null') {
      await _probeAndCollect(origin, results);
    }
    // 同源不可达（页面不在 gateway 上）→ 降级本机默认端口。
    if (results.isEmpty) {
      await _probeAndCollect(kLocalProbeBaseUrl, results);
    }
    if (!mounted) return;
    setState(() {
      _discovered = results;
      _discovering = false;
    });
  }

  /// 探测单个候选：可达才收集（名字优先 mDNS 实例名，否则用地址 host）。
  Future<void> _probeAndCollect(String base, List<DiscoveredServer> out) async {
    if (out.any((d) => d.server.baseUrl == base)) return;
    try {
      final cap = await probeServer(base);
      if (!cap.reachable) return;
      var name = Uri.parse(base).host;
      try {
        final resp = await Dio().get<Map<String, dynamic>>('$base/api/mdns/status');
        final inst = resp.data?['instanceName'];
        if (inst is String && inst.isNotEmpty) name = inst;
      } catch (_) {}
      out.add(DiscoveredServer(
        server: ServerInfo(
          id: '',
          name: name,
          baseUrl: base,
          wsUrl: '${base.replaceFirst(RegExp(r'^http'), 'ws')}/ws',
        ),
      ));
    } catch (_) {}
  }

  @override
  void dispose() {
    _browseTimer?.cancel();
    _browseSub?.cancel();
    _discovery?.dispose();
    super.dispose();
  }

  /// 发现结果连接：已存在则切换，否则添加并设为当前。
  Future<void> _addDiscovered(ServerInfo server) async {
    final existing = ref.read(serversProvider).valueOrNull ?? const <ServerInfo>[];
    for (final s in existing) {
      if (s.baseUrl == server.baseUrl) {
        await ref.read(currentServerIdProvider.notifier).set(s.id);
        return;
      }
    }
    final added = await ref
        .read(serversProvider.notifier)
        .addServer(name: server.name, baseUrl: server.baseUrl);
    await ref.read(currentServerIdProvider.notifier).set(added.id);
  }

  @override
  Widget build(BuildContext context) {
    final serversAsync = ref.watch(serversProvider);
    final currentId = ref.watch(currentServerIdProvider);
    final capabilityAsync = ref.watch(serverCapabilityProvider);

    return Scaffold(
      appBar: AppBar(title: const Text('连接')),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: () => showAddServerDialog(context),
        icon: const Icon(Icons.add),
        label: const Text('手动添加'),
      ),
      body: serversAsync.when(
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (e, _) => Center(child: Text('加载失败：$e')),
        data: (servers) {
          ServerInfo? current;
          for (final s in servers) {
            if (s.id == currentId) {
              current = s;
              break;
            }
          }
          current ??= servers.isNotEmpty ? servers.first : null;
          return ListView(
            padding: const EdgeInsets.all(12),
            children: [
              _CurrentCard(
                current: current,
                capability: capabilityAsync.valueOrNull,
              ),
              const SizedBox(height: 12),
              _DiscoveryCard(
                discovered: _discovered,
                discovering: _discovering,
                savedBases: servers.map((s) => s.baseUrl).toSet(),
                current: current,
                onConnect: _addDiscovered,
              ),
              const SizedBox(height: 12),
              Text('已保存服务器', style: Theme.of(context).textTheme.titleSmall),
              const SizedBox(height: 6),
              for (final server in servers)
                _ServerTile(
                  server: server,
                  isCurrent: server.id == currentId,
                  onTap: () => ref.read(currentServerIdProvider.notifier).set(server.id),
                  onDelete: () => ref.read(serversProvider.notifier).removeServer(server.id),
                ),
            ],
          );
        },
      ),
    );
  }
}

/// 当前连接摘要（含服务端版本与 work 能力）。
class _CurrentCard extends StatelessWidget {
  const _CurrentCard({this.current, this.capability});

  final ServerInfo? current;
  final ServerCapability? capability;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final cap = capability;
    final (chipBg, chipFg, chipText) = _status(scheme);
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(14),
        child: Row(
          children: [
            Container(
              width: 40,
              height: 40,
              decoration: BoxDecoration(
                color: scheme.primaryContainer,
                shape: BoxShape.circle,
              ),
              child: Icon(Icons.cast_connected, color: scheme.onPrimaryContainer),
            ),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    '当前连接',
                    style: Theme.of(context).textTheme.labelSmall?.copyWith(color: scheme.outline),
                  ),
                  const SizedBox(height: 2),
                  Text(
                    current?.name ?? '未连接',
                    style: Theme.of(context).textTheme.titleSmall,
                  ),
                  Text(
                    current?.baseUrl ?? '请添加服务器后选择',
                    style: Theme.of(context).textTheme.bodySmall?.copyWith(
                          color: scheme.outline,
                          fontFamily: 'monospace',
                        ),
                  ),
                  // 探测失败详情（区分连接超时 / 无法连接 / HTTP 错误）。
                  if (cap != null && !cap.reachable && cap.error != null) ...[
                    const SizedBox(height: 2),
                    Text(
                      cap.error!,
                      style: Theme.of(context)
                          .textTheme
                          .bodySmall
                          ?.copyWith(color: scheme.error),
                    ),
                  ],
                ],
              ),
            ),
            Chip(
              avatar: Icon(Icons.circle, size: 10, color: chipFg),
              label: Text(chipText),
              backgroundColor: chipBg,
              labelStyle: Theme.of(context).textTheme.labelSmall,
            ),
          ],
        ),
      ),
    );
  }

  (Color, Color, String) _status(ColorScheme scheme) {
    final cap = capability;
    if (cap == null) return (scheme.surfaceContainerHighest, scheme.outline, '探测中…');
    if (!cap.reachable) return (scheme.errorContainer, scheme.onErrorContainer, '无法连接');
    final version = cap.health?.version ?? '';
    final suffix = version.isEmpty ? '' : ' · piter $version';
    if (cap.workSupported) {
      return (scheme.secondaryContainer, scheme.onSecondaryContainer, 'work 可用$suffix');
    }
    return (scheme.errorContainer, scheme.onErrorContainer, 'work 不可用$suffix');
  }
}

/// 发现区：Web 用 HTTP 探测（浏览器禁组播，见 _httpDiscover）；App 用 mDNS（当前 stub）。
class _DiscoveryCard extends StatelessWidget {
  const _DiscoveryCard({
    this.discovered = const [],
    this.discovering = false,
    this.savedBases = const {},
    this.current,
    this.onConnect,
  });

  final List<DiscoveredServer> discovered;
  final bool discovering;

  /// 已保存服务器的 baseUrl 集合（用于"已连接/切换"标识）。
  final Set<String> savedBases;

  /// 当前选中的服务器（baseUrl 与之相同者标记"已连接"）。
  final ServerInfo? current;

  final ValueChanged<ServerInfo>? onConnect;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final Widget status;
    // 有结果优先展示列表——mDNS 浏览是持续过程，"探测中"只在尚无结果时出现。
    if (discovered.isNotEmpty) {
      status = Column(
        children: [
          for (final d in discovered)
            ListTile(
              dense: true,
              leading: Icon(Icons.wifi, color: scheme.primary),
              title: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Flexible(child: Text(d.server.name, overflow: TextOverflow.ellipsis)),
                  if (d.server.baseUrl == current?.baseUrl) ...[
                    const SizedBox(width: 6),
                    Icon(Icons.check_circle, size: 14, color: scheme.tertiary),
                  ],
                ],
              ),
              subtitle: Text(
                d.server.baseUrl,
                style: Theme.of(context).textTheme.bodySmall?.copyWith(fontFamily: 'monospace'),
              ),
              trailing: _trailing(d.server, context),
            ),
        ],
      );
    } else if (discovering) {
      status = const ListTile(
        leading: SizedBox(
          width: 20,
          height: 20,
          child: CircularProgressIndicator(strokeWidth: 2),
        ),
        title: Text('正在探测服务端…'),
      );
    } else {
      status = _degraded(scheme, context);
    }
    return Card(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 12, 16, 0),
            child: Row(
              children: [
                Icon(Icons.wifi_tethering, size: 18, color: scheme.primary),
                const SizedBox(width: 8),
                Text(
                  kIsWeb ? '服务端探测（HTTP）' : '局域网发现（mDNS）',
                  style: Theme.of(context).textTheme.titleSmall,
                ),
              ],
            ),
          ),
          status,
        ],
      ),
    );
  }

  /// 嗅探结果的右侧操作：已连接 → 标识；已保存 → 切换；否则 → 连接。
  Widget _trailing(ServerInfo server, BuildContext context) {
    if (server.baseUrl == current?.baseUrl) {
      return Padding(
        padding: const EdgeInsets.symmetric(horizontal: 8),
        child: Text(
          '已连接',
          style: Theme.of(context)
              .textTheme
              .labelMedium
              ?.copyWith(color: Theme.of(context).colorScheme.tertiary),
        ),
      );
    }
    if (savedBases.contains(server.baseUrl)) {
      return TextButton(
        onPressed: () => onConnect?.call(server),
        child: const Text('切换'),
      );
    }
    return TextButton(
      onPressed: () => onConnect?.call(server),
      child: const Text('连接'),
    );
  }

  Widget _degraded(ColorScheme scheme, BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 4, 16, 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            kIsWeb ? '未探测到服务端' : '未发现服务器',
            style: Theme.of(context).textTheme.bodyMedium?.copyWith(color: scheme.onSurfaceVariant),
          ),
          const SizedBox(height: 4),
          Text(
            kIsWeb
                ? '浏览器不支持 mDNS 组播，已改用 HTTP 探测当前页面来源与本机默认端口 31421；若桌面端未启动，请先启动后重试，或点击「手动添加」。'
                : '未发现 Piter 服务端。mDNS 依赖组播，以下场景不可用：访客/公共 WiFi、蜂窝网络、企业/校园网（VLAN 隔离）、VPN。\n请确认桌面端已启动，或使用「手动添加」作为保底通路。',
            style: Theme.of(context).textTheme.bodySmall?.copyWith(color: scheme.outline),
          ),
        ],
      ),
    );
  }
}

/// 单个已保存服务器条目。
class _ServerTile extends StatelessWidget {
  const _ServerTile({
    required this.server,
    required this.isCurrent,
    required this.onTap,
    required this.onDelete,
  });

  final ServerInfo server;
  final bool isCurrent;
  final VoidCallback onTap;
  final VoidCallback onDelete;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Card(
      color: isCurrent ? scheme.primaryContainer.withValues(alpha: 0.35) : null,
      child: ListTile(
        onTap: onTap,
        leading: Icon(
          isCurrent ? Icons.radio_button_checked : Icons.dns_outlined,
          color: isCurrent ? scheme.primary : scheme.outline,
        ),
        title: Text(server.name),
        subtitle: Text(
          server.baseUrl,
          style: Theme.of(context).textTheme.bodySmall?.copyWith(fontFamily: 'monospace'),
        ),
        trailing: isCurrent
            ? Icon(Icons.check_circle, color: scheme.primary)
            : IconButton(
                icon: Icon(Icons.delete_outline, color: scheme.outline),
                tooltip: '删除',
                onPressed: onDelete,
              ),
      ),
    );
  }
}
