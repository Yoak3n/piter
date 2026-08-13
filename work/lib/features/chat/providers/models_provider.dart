/// 原生 chat 模型目录 / 默认模型 / 当前会话模型选择。
library;

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../core/network/models/models.dart';
import '../../work/providers/data_sources.dart';

/// 模型目录（GET /api/pi/model-catalog，磁盘缓存，不起进程）。
final chatModelsProvider = FutureProvider<List<ModelInfo>>((ref) async {
  return ref.watch(apiClientProvider).modelCatalog();
});

/// 默认模型设置（GET /api/pi/settings）。
final chatPiSettingsProvider = FutureProvider<PiSettings>((ref) async {
  return ref.watch(apiClientProvider).piSettings();
});

/// 当前会话选中的模型（per-session，随 prompt desiredModel 生效；
/// 对齐 Vue：切换模型不立即发 set_model）。
final currentChatModelProvider = StateProvider<ModelInfo?>((ref) => null);
