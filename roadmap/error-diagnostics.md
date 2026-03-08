# エラー診断の改善

## ソース位置情報（Span）
現状のDiagnosticにはcontextフィールドしかなく、行番号・列番号がない。

```
// 現状
error: list element at index 1 has type String but expected Int
  in list literal
  hint: All list elements must have the same type

// 改善後
error: list element at index 1 has type String but expected Int
  --> src/main.almd:15:12
   |
15 |   [1, "hello", 3]
   |       ^^^^^^^ expected Int, found String
  hint: All list elements must have the same type
```

### 実装ステップ
1. Token に行番号・列番号を追加（lexer.rs）
2. AST Expr/Stmt に Span フィールドを追加
3. Checker が Span を Diagnostic に渡す
4. display() でソース行を表示

## パースエラーの改善
現状のパーサーは最初のエラーで停止。複数エラーの一括報告が望ましい。

## 未使用変数の警告
```
warning: unused variable `temp`
  --> src/main.almd:10:7
  hint: prefix with _ to suppress: `_temp`
```

## 到達不能コードの検出
```
warning: unreachable code after guard
  --> src/main.almd:12:3
```

## Priority
ソース位置情報 > 未使用変数警告 > パースエラー改善 > 到達不能検出
