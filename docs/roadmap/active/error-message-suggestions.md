<!-- description: Fuzzy matching suggestions in error messages (did you mean?) -->
# Error Message Suggestions

未定義の変数・関数に対して Levenshtein 距離ベースの候補を提示する。

## 参考

- **Gleam**: `error.rs` の `did_you_mean()` — 距離 < name.len()/3 で候補表示
- **Elm**: `Reporting/Suggest.hs` — restricted Damerau-Levenshtein + case-insensitive matching

## ゴール

```
error[E003]: undefined variable 'prntln'
  hint: Did you mean `println`?
```

- スコープ内の変数名・関数名・モジュール名を候補として検索
- 大文字小文字を無視したマッチング
- LLM のタイポを自動修正可能にする
