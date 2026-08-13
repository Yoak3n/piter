/// ServerConfig：服务器列表 + token 持久化（shared_preferences，Web 落 localStorage）。
///
/// 服务器记录只由用户经连接页新增（手动/发现）；无记录就是空列表——
/// 不臆造"本机服务器"（没有连接服务器就是没有服务器，不误导）。
library;

import 'dart:convert';

import 'package:shared_preferences/shared_preferences.dart';

/// 一条服务器连接记录。
class ServerInfo {
  const ServerInfo({
    required this.id,
    required this.name,
    required this.baseUrl,
    required this.wsUrl,
  });

  final String id;
  final String name;

  /// http://IP:PORT
  final String baseUrl;

  /// ws://IP:PORT/ws
  final String wsUrl;

  Map<String, dynamic> toJson() => {'id': id, 'name': name, 'baseUrl': baseUrl, 'wsUrl': wsUrl};

  static ServerInfo? fromJson(Map<String, dynamic> json) {
    final id = json['id'];
    final name = json['name'];
    final baseUrl = json['baseUrl'];
    final wsUrl = json['wsUrl'];
    if (id is! String || name is! String || baseUrl is! String || wsUrl is! String) return null;
    return ServerInfo(id: id, name: name, baseUrl: baseUrl, wsUrl: wsUrl);
  }
}

/// 服务器列表 + token 的读写。
class ServerConfig {
  static const _kServersKey = 'piter.servers';
  static const _kTokenKey = 'piter.lanToken';
  static const _kCurrentServerKey = 'piter.currentServerId';

  /// 读取服务器列表；无记录时返回空列表（不做默认服务器兜底）。
  static Future<List<ServerInfo>> loadServers() async {
    final prefs = await SharedPreferences.getInstance();
    final raw = prefs.getString(_kServersKey);
    if (raw == null || raw.isEmpty) return [];
    final list = (jsonDecode(raw) as List<dynamic>? ?? const [])
        .whereType<Map<String, dynamic>>()
        .map(ServerInfo.fromJson)
        .whereType<ServerInfo>()
        .toList();
    return list;
  }

  static Future<void> saveServers(List<ServerInfo> servers) async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(_kServersKey, jsonEncode(servers.map((s) => s.toJson()).toList()));
  }

  /// 当前选中的服务器 id；无记录时回退第一个。
  static Future<String> loadCurrentServerId(List<ServerInfo> servers) async {
    final prefs = await SharedPreferences.getInstance();
    final id = prefs.getString(_kCurrentServerKey);
    if (id != null && servers.any((s) => s.id == id)) return id;
    return servers.isNotEmpty ? servers.first.id : '';
  }

  static Future<void> saveCurrentServerId(String id) async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(_kCurrentServerKey, id);
  }

  static Future<String?> loadToken() async {
    final prefs = await SharedPreferences.getInstance();
    return prefs.getString(_kTokenKey);
  }

  static Future<void> saveToken(String token) async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(_kTokenKey, token);
  }

  static Future<void> clearToken() async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.remove(_kTokenKey);
  }
}
