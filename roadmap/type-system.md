# 型システム拡張

## ユーザー定義ジェネリクス
現状はList[T]やOption[T]など組み込み型のみジェネリック。

```almide
// 提案
type Stack[T] = { items: List[T] }

fn push[T](stack: Stack[T], item: T) -> Stack[T] = {
  { items: stack.items ++ [item] }
}
```

## Trait の本格実装
lexer/parserにキーワードはあるが、型チェックとコード生成が不十分。

```almide
trait Show {
  fn show(self) -> String
}

impl Show for Point {
  fn show(self) -> String = "${self.x}, ${self.y}"
}
```

## Tuple型
レコードだと名前が必要で冗長。

```almide
// 提案
let pair: (Int, String) = (42, "hello")
let (a, b) = pair
```

## 構造化エラー型
現状 Result[T, String] の String 固定で、エラーの種類分けが難しい。

```almide
// 提案
type AppError = NotFound(String) | Unauthorized | Internal(String)
type AppResult[T] = Result[T, AppError]
```
match armでエラー種別による分岐が可能になる。

## 型エイリアス
```almide
type UserId = Int
type Config = Map[String, String]
```
現状 newtype は存在するが限定的。

## Priority
ユーザー定義ジェネリクス > 構造化エラー型 > Trait実装 > Tuple > 型エイリアス
