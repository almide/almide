# シンタックスシュガー・利便性

## レンジリテラル
```almide
let xs = 0..10        // [0, 1, 2, ..., 9]
let ys = 0..=10       // [0, 1, 2, ..., 10]
let zs = 10..0..-1    // [10, 9, 8, ..., 1]
```

## リスト内包表記
```almide
let evens = [x for x in 0..100 if x % 2 == 0]
let pairs = [(x, y) for x in xs for y in ys]
```

## デフォルト引数
```almide
fn greet(name: String, greeting: String = "Hello") -> String =
  "${greeting}, ${name}!"
```

## 名前付き引数
```almide
http.response(status: 200, body: "OK")
```

## パターンマッチの網羅性チェック
variant型のmatchで全ケースを網羅しているかコンパイル時に検出。

```almide
type Color = Red | Green | Blue

fn name(c: Color) -> String = match c {
  Red => "red",
  Green => "green",
  // warning: non-exhaustive match, missing: Blue
}
```

## 文字列のraw記法
```almide
let regex_pattern = r"^\d{3}-\d{4}$"
let path = r"C:\Users\test"
```

## ブロックコメント
```almide
/*
  複数行コメント
  現状は // のみ
*/
```

## Priority
レンジリテラル > 網羅性チェック > ブロックコメント > リスト内包表記 > デフォルト引数 > raw文字列
