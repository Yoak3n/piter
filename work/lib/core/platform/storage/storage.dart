/// 平台能力桥：存储抽象（storage）。
///
/// 本阶段 Web / 移动端均使用 shared_preferences（Web 落 localStorage），
/// 后续若接入原生存储（如 file system / secure storage）在此替换实现，UI 零改动。
library;

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// 全局存储实例 provider（各模块 UI 读共享偏好时统一使用）。
final storageServiceProvider = Provider<StorageService>((ref) => createStorageService());

/// 键值存储接口。
abstract class StorageService {
  Future<String?> read(String key);
  Future<void> write(String key, String value);
  Future<void> remove(String key);
}

/// shared_preferences 实现（双端通用）。
class SharedPrefsStorage implements StorageService {
  const SharedPrefsStorage();

  @override
  Future<String?> read(String key) async {
    final prefs = await SharedPreferences.getInstance();
    return prefs.getString(key);
  }

  @override
  Future<void> write(String key, String value) async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(key, value);
  }

  @override
  Future<void> remove(String key) async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.remove(key);
  }
}

/// 创建平台存储实例。
StorageService createStorageService() => const SharedPrefsStorage();
