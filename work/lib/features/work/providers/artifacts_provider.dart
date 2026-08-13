/// 产物区状态（按 turn 分组，新→旧）。
library;

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../core/network/models/models.dart';
import 'data_sources.dart';

final artifactsProvider = AsyncNotifierProvider.family<ArtifactsNotifier, List<ArtifactTurn>,
    String>(ArtifactsNotifier.new);

class ArtifactsNotifier extends FamilyAsyncNotifier<List<ArtifactTurn>, String> {
  @override
  Future<List<ArtifactTurn>> build(String arg) =>
      ref.watch(apiClientProvider).getArtifacts(arg);

  /// 拉取自 sinceTurn 之后的新产物（turn_artifacts 增量同步）。
  Future<void> refreshSince(int sinceTurn) async {
    final fresh = await ref.read(apiClientProvider).getArtifacts(arg, sinceTurn: sinceTurn);
    final existing = state.value ?? const <ArtifactTurn>[];
    final merged = [...existing, ...fresh]
      ..sort((a, b) => b.turnId.compareTo(a.turnId));
    state = AsyncData(merged);
  }
}
