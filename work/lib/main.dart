/// piter work 模块入口：kIsWeb 分支决定壳（Web 仅 work / App 双 tab）。
library;

import 'package:flutter/foundation.dart' show kIsWeb;
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'app/piter_app.dart';

void main() {
  runApp(ProviderScope(child: PiterApp(isWeb: kIsWeb)));
}
