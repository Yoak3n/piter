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

/// 默认模型（目录加载后：取 pi settings 的 default 模型；无则取目录第一个）。
/// 用于模型选择器未选中时显示真实默认值，而非占位文案。
final chatDefaultModelProvider = Provider<ModelInfo?>((ref) {
  final models = ref.watch(chatModelsProvider).valueOrNull ?? const <ModelInfo>[];
  if (models.isEmpty) return null;
  final settings = ref.watch(chatPiSettingsProvider).valueOrNull;
  final def = settings?.defaultModel;
  if (def != null && def.isNotEmpty) {
    for (final m in models) {
      if (m.id == def) return m;
    }
  }
  return models.first;
});
