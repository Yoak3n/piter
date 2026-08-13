/// 顶部模块切换（聊天 / 工作空间）。App 端两个模块页面的 AppBar title 复用，
/// 点击即可在 IndexedStack 之间切换，替代底部 NavigationBar。
library;

import 'package:flutter/material.dart';

class ModuleSwitcher extends StatelessWidget {
  const ModuleSwitcher({
    super.key,
    required this.current,
    required this.onSwitch,
  });

  /// 0 = 聊天，1 = 工作空间。
  final int current;
  final ValueChanged<int> onSwitch;

  @override
  Widget build(BuildContext context) {
    return SegmentedButton<int>(
      showSelectedIcon: false,
      segments: const [
        ButtonSegment(value: 0, label: Text('聊天')),
        ButtonSegment(value: 1, label: Text('工作空间')),
      ],
      selected: {current},
      onSelectionChanged: (s) => onSwitch(s.first),
      style: const ButtonStyle(
        visualDensity: VisualDensity.compact,
        tapTargetSize: MaterialTapTargetSize.shrinkWrap,
      ),
    );
  }
}
