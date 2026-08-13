/// unified diff 解析器测试（edit 工具 details.patch 渲染骨架）。
library;

import 'package:flutter_test/flutter_test.dart';

import 'package:piter_work/core/utils/diff_parser.dart';

void main() {
  group('parseUnifiedDiff', () {
    test('标准 patch：路径头 + hunk + add/del/context', () {
      const patch = '''
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,7 +1,10 @@
 pub fn format_number(n: u32) -> String {
     format!("{n}")
 }
 
+/// 将数字格式化为千分位分隔
+pub fn format_currency(n: u64) -> String {
+    format!("{:>10}", n)
+}
+
 #[cfg(test)]
 mod tests {
     use super::*;
''';
      final diff = parseUnifiedDiff(patch);
      expect(diff.oldPath, 'src/lib.rs');
      expect(diff.newPath, 'src/lib.rs');
      expect(diff.hunks.length, 1);
      expect(diff.hunks.single.oldStart, 1);
      expect(diff.hunks.single.newCount, 10);
      expect(diff.additions, 5); // 含一个 `+` 空行（新增空行）
      expect(diff.deletions, 0);
      expect(diff.isEmpty, isFalse);
    });

    test('含删除行与多个 hunk', () {
      const patch = '''
--- a/a.txt
+++ b/b.txt
@@ -1,3 +1,3 @@
-old line
 context
+new line
@@ -10,2 +10,1 @@
-removed
''';
      final diff = parseUnifiedDiff(patch);
      expect(diff.hunks.length, 2);
      expect(diff.deletions, 2);
      expect(diff.additions, 1);
      // 行前缀已剥离
      final addLine = diff.lines.firstWhere((l) => l.kind == DiffLineKind.addition);
      expect(addLine.text, 'new line');
      final delLine = diff.lines.firstWhere((l) => l.kind == DiffLineKind.deletion);
      expect(delLine.text, 'old line');
    });

    test('非 patch 输入返回空', () {
      expect(parseUnifiedDiff('no diff here').isEmpty, isTrue);
      expect(parseUnifiedDiff('').isEmpty, isTrue);
    });

    test(r'\ No newline 信息行被忽略', () {
      const patch = r'''
--- a/x
+++ b/x
@@ -1,1 +1,1 @@
-old
+new
\ No newline at end of file
''';
      final diff = parseUnifiedDiff(patch);
      expect(diff.hunks.single.lines.length, 2);
    });
  });
}
