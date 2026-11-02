<a id="almide-llm-benchmark-results--historical-typescriptdeno-experiment"></a>

# Almide LLM ベンチマーク結果 — 過去の TypeScript/Deno 実験

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](BENCHMARK.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](BENCHMARK.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](BENCHMARK.zh-CN.md)

> 固定原文の非規範的な履歴参照資料であり、原本を正本とする。自動同期しない。記載のテスト・測定・実装状態・保証は原文の主張であり、独立した再実行や現行の検証ではない。引用コードとコマンドは実行していない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/exercises/BENCHMARK.md)
> 知識ゼロ・十分性の主張はこの過去の演習集合についてのものであり、学習データ除外や普遍的正しさの証拠ではない。旧コードと指示は現行構文の手引きではない。相対 CHEATSHEET リンクはここでは原文 snapshot に固定するが、実験当時の正確な prompt だったとは主張しない。

これは以前の TypeScript transpiler 実験の記録であり、現在の Rust/Wasm コンパイラのベンチマークではない。測定結果と旧構文は証拠として保持する。以下の auto-unwrap、`do`、`++`、`src/almide.ts` の説明は歴史資料であり、現行言語の手引きではない。

<a id="overview"></a>

## 概要

LLM は、[現在の CHEATSHEET.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/CHEATSHEET.md) だけを使い、**事前知識ゼロ**で正しい Almide コードを書けるだろうか。

これを確かめるため、LLM agent（Claude）に CHEATSHEET だけを参考資料として与え、Exercism 形式の演習 14 問を一から解かせた。比較のため同じ演習を Python でも解いた。

<a id="results"></a>

## 結果

<a id="almide-cheatsheet-only-zero-knowledge"></a>

### Almide（CHEATSHEET のみ、知識ゼロ）

| 演習 | テスト | 合格 | 失敗 | 注記 |
|---|---|---|---|---|
| affine-cipher | 16 | 16 | 0 | |
| bob | 25 | 25 | 0 | |
| collatz-conjecture | 6 | 6 | 0 | |
| config-merger | 27 | 27 | 0 | `effect fn` + ファイル I/O エラー伝播 |
| grade-report | 30 | 30 | 0 | 複数関数にまたがるエラー伝播 |
| hamming | 9 | 9 | 0 | |
| isbn-verifier | 14 | 14 | 0 | |
| isogram | 13 | 13 | 0 | |
| pangram | 10 | 10 | 0 | |
| phone-number | 14 | 14 | 0 | |
| pipeline | 36 | 36 | 0 | `do` block の自動エラー伝播 |
| raindrops | 18 | 18 | 0 | |
| roman-numerals | 19 | 19 | 0 | |
| scrabble-score | 11 | 11 | 0 | |
| **合計** | **248** | **248** | **0** | **100%** |

<a id="python-baseline-comparison"></a>

### Python（比較基準）

| 演習 | テスト | 合格 | 注記 |
|---|---|---|---|
| affine-cipher | 16 | 16 | |
| bob | 25 | 25 | |
| collatz-conjecture | 6 | 6 | |
| config-merger | 27 | 27 | |
| grade-report | 30 | 30 | |
| hamming | 9 | 9 | |
| isbn-verifier | 14 | 14 | |
| isogram | 13 | 13 | |
| pangram | 10 | 10 | |
| phone-number | 14 | 14 | |
| pipeline | 36 | 36 | |
| raindrops | 18 | 18 | |
| roman-numerals | 19 | 19 | |
| scrabble-score | 11 | 11 | |
| **合計** | **248** | **248** | **100%** |

<a id="methodology"></a>

## 方法

1. **演習設計**：各演習は `= _`（hole stub）付きの関数署名とテストケースを提供する。LLM は全関数を実装する。
2. **Almide agent**：与えるのは `CHEATSHEET.md` だけ。他の文書、Almide コード例、transpiler ソースへのアクセスはない。
3. **Python agent**：標準的な Python の知識で、特別な制約はない。
4. **Test runner**：Almide ファイルは `src/almide.ts` で TypeScript へ transpile し、`deno test` でテストする。Python ファイルは `pytest` でテストする。
5. **試行は一度**：各演習は再試行やフィードバックループなしの 1 pass で解いた。

<a id="key-findings"></a>

## 主な知見

<a id="the-cheatsheet-is-sufficient"></a>

### CHEATSHEET だけで十分
LLM は、340 行の簡易リファレンスだけを使い、知識ゼロから正しい Almide コードを書ける。248/248 テスト合格（100%）。

<a id="result-erasure--fold-resolved"></a>

### Result erasure + fold：解決済み
`let result = list.fold(...); match result { err(_) => ... }` パターンは以前失敗していた。fold callback 内の `err()` が `Array.reduce` の外へ throw し、代入を妨げていたためである。codegen で `let x = expr; match x { err(...) => ... }` パターンを検出し、式を match 対象に inline 化することで修正した。これにより try-catch wrapper が throw を正しく捕捉する。

<a id="almide-advantages-visible-in-complex-exercises"></a>

### 複雑な演習に見える Almide の利点

**`do` block の自動エラー伝播**（pipeline 演習）：
```
fn run_pipeline(input: String) -> Result[String, String] = do {
  let pairs = parse_pairs(input)       // auto-unwrap Result
  let pairs2 = validate_keys(pairs)    // error → propagate
  let pairs3 = validate_values(pairs2) // error → propagate
  let pairs4 = transform(pairs3)       // error → propagate
  format_output(pairs4)
}
```
Python との対比（暗黙の例外伝播で、型レベルのエラー契約はない）：
```python
def run_pipeline(text):
    pairs = parse_pairs(text)
    pairs = validate_keys(pairs)
    pairs = validate_values(pairs)
    pairs = transform(pairs)
    return format_output(pairs)
```

**`err(e)` パターンを使う `match`**（grade-report 演習）：
```
match parse_student(line) {
  ok(student) => { ... },
  err(e) => err("line " ++ line_num ++ ": " ++ e),  // catch and re-wrap
}
```

**ファイル I/O エラー伝播を伴う `effect fn`**（config-merger 演習）：
```
effect fn merge_and_save(paths: List[String], output: String) -> Result[Int, String] = {
  match merge_files(paths) {       // file read errors propagate automatically
    err(e) => err(e),
    ok(pairs) => {
      save_config(output, pairs);  // file write errors propagate
      ok(list.len(pairs))
    }
  }
}
```
Python との対比（例外は暗黙で、型レベルの契約はない）：
```python
def merge_and_save(paths, output):
    merged = merge_files(paths)     # FileNotFoundError? ValueError? Unknown
    save_config(output, merged)     # IOError? Unknown
    return len(merged)
```

<a id="result-erasure-codegen-auto-inlining"></a>

### Result erasure：codegen の自動 inline 化
codegen は `let x = expr; match x { ..., err(e) => ... }` を検出し、`expr` を match 対象へ自動 inline 化して try-catch で囲む。これで直接 match（`match fn() { err(e) => ... }`）も let-then-match パターンも正しく動作する。エラーケースのテストには `assert_eq(expr, err("msg"))` を使う。

<a id="stdlib-note"></a>

### stdlib の注記
`string.to_int`、`string.to_upper`、`string.to_lower` が stdlib に入り、以前の演習にあった定型記述を減らしている。

<a id="historical-runner-commands"></a>

## 過去の runner コマンド

```bash
# Run a single exercise
bash exercises/run_exercise.sh exercises/pipeline/pipeline.almd

# Run all exercises
for f in exercises/*/*.almd; do
  echo "=== $(basename $(dirname $f)) ==="
  bash exercises/run_exercise.sh "$f"
done
```

> コード内コメントの訳（コード自体は原文保持）：Result を自動 unwrap する／エラーを伝播する／捕捉して包み直す／ファイル読み取りエラーは自動伝播する／書き込みエラーは伝播する。Python 例の注記は「FileNotFoundError か ValueError か不明」「IOError か不明」。runner の注記は「単一演習を実行」「全演習を実行」。
