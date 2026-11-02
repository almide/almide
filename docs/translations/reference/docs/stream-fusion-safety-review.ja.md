<a id="stream-fusion-safety-review-2026-09-09"></a>

# Stream fusion の安全性レビュー（2026-09-09）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](stream-fusion-safety-review.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](stream-fusion-safety-review.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](stream-fusion-safety-review.zh-CN.md)

> 固定原文の非規範的な履歴参照資料。以下のテスト・ブランチ状態・測定・修正は日付付き報告で、新規実行した検証や普遍的な正しさの保証ではない。意図的に発散する例を含むコードは原文保持し、実行していない。自動同期せず、原本を正本とする。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stream-fusion-safety-review.md)

PR #2053 は #2054 の map コールバック効果の重複を修復する。当時の develop へ更新する際も StreamFusion と並ぶ DecodeErrFrame パスを保持している。テキスト上の唯一のマージ競合はパイプライン文書だった。

<a id="reference-and-decision"></a>

## 参照と判断

固定した Gleam checkout `../almide-references/gleam`（`19bf207ebb7d953ea1391f041da48c214ee1440a`）の `compiler-core/src/javascript/decision.rs::assign_subject` は、変数でない対象を再利用する前に一度だけ評価して束縛すべき理由を説明する。これは観測可能な評価を保つ根拠であり、任意の式を安全に繰り返し・並べ替え・省略できると仮定する根拠ではない。Almide の既存 `crates/almide-mir/src/purity.rs` はさらに、ホスト効果を持つモジュール、不純な通常関数（アロケータ状態を含む）、高階呼び出しを区別する。この区別は融合にも当てはまる。

レビューでは、元の融合の停止性解析が純粋性と全域性の両方に最大不動点を使っていたことが分かった。再帰サイクルに外部副作用がなくても、停止する証拠にはならない。次の受理されるプログラムは、

```almide
fn diverge(n: Int) -> Int = if n == 0 then 0 else diverge(n) + 1
fn main() -> Unit = {
  let xs = [1] |> list.map((x) => diverge(x)) |> list.take(0)
  println(int.to_string(list.len(xs)))
}
```

以前は単一の `.map(...).take(0).collect()` 連鎖を出力し、再帰呼び出しが消えていた。現在の記録では `list_take(map(...).collect(), 0)` を出力し、元の eager map が take の前に完了しなければならない。発散するプログラム自体を実行するテストはない。出力形を検査し、単体テストは直接再帰・相互再帰・そのサイクルの呼び出し元を全域的と認めず、非循環の全域的呼び出し連鎖は認める。

全域性は証明済みの葉関数から増やす（最小不動点）。純粋性は最大不動点を維持する。ランタイム呼び出しには、効果を持つ集合の外にある既知の標準ライブラリモジュールも必要。関数引数は呼び出し先の証明が必要で、可変借用は拒否する。任意の InlineRust は疑わしい語がないだけでは純粋性を得られない。許可するのはコンパイラの正確な番号付き配列プレースホルダー形式だけ。テストは不透明な Rust、効果あり／未知のランタイムモジュール、コールバック参照、可変借用を対象とする。

式の証明を、互いに重ならないリテラル・演算子・制御・集約・ラッパー・呼び出しの各ケースに分割する。`Some(false)` は dispatch を止め、未知のケースは証明を返さない。Codopsy 2.2.0 の測定では、ルール変更なしで融合パスは **100/A**、codegen crate 全体の既存集計は **89/B** のまま。

<a id="validation-of-the-updated-branch"></a>

## 更新したブランチの検証

- `almide test` と `almide test --target rust` はともに 438/438 ファイル。
- Codegen crate：単体テスト 56 件、統合テスト 10 件に合格。
- Nanopass／codegen snapshot／end-to-end render ゲート：34／13／2 件に合格。
- 全 codegen target に対する Clippy：合格。変更した融合ファイルを指す警告はない。既存の無関係な警告は残る。
- パス一覧、ファイル規律、`git diff --check`：合格。

<a id="module-names-must-not-establish-root-function-purity"></a>

## モジュール名からルート関数の純粋性を認めてはならない

追加の端から端までの監査では、モジュールの裸の関数名が、ルート関数と同じ証明集合へ追加されていた。純粋な `helper.observe` と出力するルート `observe` があると、`[1, 2] |> list.map((n) => observe(n)) |>
list.take(0)` は誤って両方の出力を抑制した。元のバイナリは `0, 0` を出力し、融合を無効にすると `0, 1, 2, 0` を出力した。モジュール関数は修正後、修飾付きの別名だけを登録し、名前空間をまたいだ純粋性・全域性の証明を防ぐ。`tests/stream_fusion_module_purity_test.rs` は実際に import したモジュールを通した観測可能な出力を固定する。
