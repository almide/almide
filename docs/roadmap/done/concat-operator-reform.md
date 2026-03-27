<!-- description: Reform ++ concatenation operator for strings and lists -->
# Concatenation Operator Reform

**優先度:** High — 1.0 前の breaking change 候補
**リサーチ:** [docs/research/concat-operators.md](../../research/concat-operators.md)

## 現状

`++` が String と List 両方の結合に使われる（Elm/Haskell方式）。

```almide
"Hello, " ++ name ++ "!"   // String
[1, 2] ++ [3, 4]           // List
```

## 問題

1. String結合の大半は補間 `"Hello, ${name}!"` で書ける — `++` は冗長
2. `++` はLLMにとって馴染みが薄い（Elm/Haskell以外で使わない）
3. JSON構築で `'"' ++ key ++ '":"' ++ val ++ '"'` のようなコードが生まれる

## リサーチ結論

- **`+` overload（Python/Kotlin/Swift）**: LLMは慣れてるが `1 + "a"` 問題
- **別演算子（Gleam `<>`, OCaml `^`/`@`, Elixir `<>`/`++`）**: 明確だがLLM学習コスト
- **`++` 維持（Elm/Haskell）**: 型安全、polymorphic。**これを採用した言語で後悔した例はない**
- **補間がある言語では concat operator の重要性が低下** — 全言語共通の傾向

## 選択肢

| 案 | String | List | 互換性 | LLM親和性 |
|---|---|---|---|---|
| A. `++` 維持 | `++` | `++` | 変更なし | 中 |
| B. `++` をList専用、String は補間のみ | 補間 `"${a}${b}"` | `++` | breaking | 高 |
| C. `+` に統一 | `+` | `+` | breaking | 最高 |
| D. Gleam方式 `<>` | `<>` | `++` or `list.concat` | breaking | 中 |

## 推奨: A (維持) or B (List専用)

### A を推す理由
- **`++` を採用して後悔した言語はゼロ**（リサーチ結論）
- 補間があるので String `++` の使用頻度は既に低い
- 1.0 前に breaking change のリスクを取る理由が弱い
- LLM は `++` を「Almide の結合演算子」として学習済み

### B を推す理由
- String 結合を補間に強制することで**1つの正解**を作れる
- `++` のセマンティクスが「List結合」に限定され明確
- ただし `"a" ++ "b"` をコンパイルエラーにすると既存コードが壊れる

## 判断基準

1. 既存の exercises / spec / showcase で `++` が String に使われてる箇所は何件か
2. それら全てが補間で置き換え可能か
3. LLM の初回正答率に影響があるか

## TODO

- [ ] `++` の String 使用箇所を数える
- [ ] 補間で置き換え可能か判定
- [ ] 決定: A or B
- [ ] (B の場合) String `++` を deprecation warning → error に
