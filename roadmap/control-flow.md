# 制御フロー拡張

## while ループ
現状 `do { guard cond else ok(()); ... }` で代替しているが冗長。

```almide
// 提案
while n > 0 {
  n = n - 1
}

// 現状の回避策
do {
  guard n > 0 else ok(())
  n = n - 1
}
```

## break / continue
for-in ループからの早期脱出ができない。

```almide
for x in items {
  if x == target then break
  process(x)
}
```

guardで部分的に代替可能だが、continueに相当するものがない。

## early return
関数本体が単一式なので、途中で返すにはguardかmatchのネストが必要。

```almide
// 提案
fn find(items: List[Int], target: Int) -> Option[Int] = {
  for i in items {
    if i == target then return some(i)
  }
  none
}
```

## for-range
インデックスベースのループが冗長。

```almide
// 提案
for i in 0..10 { ... }

// 現状: list.fold + カウンタ or var + do ループ
```

## Priority
while > for-range > break/continue > early return
