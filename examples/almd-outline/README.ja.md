# almd-outline

[English](./README.md) | [日本語](./README.ja.md) | [简体中文](./README.zh-CN.md)

自らの言語を使うデモとして、`almide ide outline` のフォーマッターを Almide 自身で再実装したものです。

主要ツールチェーンの `almide ide outline <file>` は3つの処理を行います。

1. ソースファイルの構文解析と型検査（コンパイラ内部を経由 — Rust）。
2. `ModuleInterface` の抽出（`almide-tools::interface::extract` 経由）。
3. 宣言ごとに1行のテキストとしてインターフェースを描画。

この例が扱うのは **手順3** — 描画 — であり、純粋な Almide で書かれています。
入力は `almide compile --json <file>` が出力する JSON です。

## 実行

```sh
# 1. Generate a ModuleInterface JSON for some source file
almide compile --json /path/to/your.almd > /tmp/iface.json

# 2. Render it with the Almide-written formatter
almide run examples/almd-outline/src/main.almd -- /tmp/iface.json
```

同じソースに対する `almide ide outline /path/to/your.almd` と、出力はビット単位で一致します。

## なぜ重要か

Almide の使命は「LLM が最も正確に書ける言語」です。主要なパターン（JSON の解析、パターンマッチ、再帰的な描画、文字列補間、エフェクトを伴う I/O）を使う実質的な Almide プログラムを素直に書けるなら、それはマイクロベンチマークより強い検証になります。このファイルは約140行で、次を扱います。

- `import json / env / fs` と、エフェクトを持つモジュールを明示的に import する規律
- `Value` + `value.as_*`（Option 形式には `?`）／`json.get_*` による走査
- 文字列タグに対する `match` と、文字列補間を使う分岐
- パイプ連鎖：`xs |> list.map(f) |> list.join(", ")`
- 再帰的な描画（レコードを経由して `render_type` → `render_inner` → `render_type`）
- `effect fn` で早期に戻るための `guard cond else err("msg")`
- `Option`／`Result` を扱う `expr ?? default` と `expr!`

## 作成時に見つかった過去の不足

以下は当初の実装経験の記録であり、現在未解決の問題一覧ではありません。特に [llms.txt](../../llms.txt) は現在存在します。過去の不足を未実装とみなす前に、現在の診断とインターフェースのソースを確認してください。

1. **`guard cond else { println(...) }`**（副作用のみで戻り値なし）は、rustc の `expected Result<(), String>, found ()` として漏れ出すのではなく、検査器の段階で「guard else must return or err」と報告すべきです。[rustc-leak wrap](../../src/cli/mod.rs) が現在動作するのは `almide test` のみで、`almide run`／`almide build` ではありません。拡張すべきです。

2. **`effect` は予約キーワード** — 変数名には使えません。LLM が繰り返し間違えないよう、`expect_ident` の診断一覧に明示する価値があります。

3. **JSON フィールド名だけでは意味が分かりません。** `ModuleInterface` は関数の戻り型を `"return"`（`"ret"` ではない）、定数を `"type"`（`"ty"` ではない）としてシリアライズします。仕様なしで読み取り処理を書くには、推測してから訂正する必要がありました。[crates/almide-tools/src/interface.rs](../../crates/almide-tools/src/interface.rs) の doc コメントと公開 JSON スキーマでスキーマを固定するか、`llms.txt`（Phase 3-3）を公開し、将来の再実装でリバースエンジニアリングを不要にすべきです。

それでも全体としての経験は、**この種の作業に言語が十分応えられる**というものでした。上記の不足を把握した後は、機能が欠けているとも、不自然な回避策が必要だとも感じませんでした。
