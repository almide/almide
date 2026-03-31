<!-- description: Built-in snapshot testing for output regression detection -->
# Snapshot Testing

出力のスナップショットをファイルに保存し、回帰を自動検出する。

## 参考

- **MoonBit**: `inspect(x, content="expected")` — Show ベース + JSON スナップショット
- **Gleam**: `type_/tests/snapshots/` — テスト結果のスナップショット保存

## ゴール

```almide
test "format output" {
  let result = format_table(data)
  snapshot(result)  // 初回: ファイルに保存。2回目以降: 比較
}
```
