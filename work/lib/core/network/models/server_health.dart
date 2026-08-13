/// 服务端健康/能力模型（GET /api/health）。
library;

/// GET /api/health 响应。
class ServerHealth {
  const ServerHealth({
    required this.status,
    required this.version,
    required this.piVersion,
    this.lanUrls = const [],
    this.brokerUrl = '',
  });

  final String status;

  /// piter 服务端版本，如 "0.2.1"。
  final String version;

  /// pi agent 版本，如 "0.83.0"。
  final String piVersion;
  final List<String> lanUrls;
  final String brokerUrl;

  factory ServerHealth.fromJson(Map<String, dynamic> json) => ServerHealth(
        status: json['status'] as String? ?? '',
        version: json['version'] as String? ?? '',
        piVersion: json['pi_version'] as String? ?? '',
        lanUrls: (json['lan_urls'] as List<dynamic>? ?? const []).cast<String>(),
        brokerUrl: json['broker_url'] as String? ?? '',
      );
}
