<a id="architecture-decision-records"></a>

# アーキテクチャ決定記録

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)

> [日本語・英語混在の正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/README.md)を 35297a612fb29e3f78f6587aba253aa91d51f466 に固定した非規範的な読者向け翻訳です。自動同期されません。正本が引き続き権威を持ち、規範性がある場合も原本に属し、この読者版には属しません。日付・状態・実装や検証に関する主張は原本の記述として保持し、独立には再検証していません。テンプレートのコードブロックは日本語を含め原文のままです。

言語設計・アーキテクチャの**決定**と、その**根拠**と、**却下した代替案**を時系列で残す。

<a id="なぜ-adr-か"></a>

## なぜ ADR か

`docs/roadmap/` は「これから何をするか」、`docs/specs/` は「いま何であるか」を書く。
どちらも**なぜそう決めたか**を保存しない。結果として同じ議論が半年後に再演され、
一度却下した案が根拠を失ったまま復活する。ADR はその 3 つ目の軸を持つ。

[REJECTED_PATTERNS.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/REJECTED_PATTERNS.md) は「採用しない機能」の一覧であり、
ADR はその**決定に至る過程**（調査した証拠、比較した代替案、反証条件）を保存する。
却下が REJECTED_PATTERNS に載るときは、根拠として ADR を指す。

<a id="形式"></a>

## 形式

`NNNN-kebab-case-title.md`。番号は連番、リネームしない（リンクが腐る）。

```markdown
# ADR-NNNN: Title in English

- **Status**: Proposed | Accepted | Superseded by [ADR-MMMM](./MMMM-....md)
- **Date**: YYYY-MM-DD
- **Context**: 何が問題だったか（決定の前に読者が知る必要のある事実）
- **Decision**: 何を決めたか（一文で言い切る）
- **Rationale**: なぜそれか（証拠つき）
- **Alternatives**: 検討して却下した案と、その理由
- **Consequences**: 何が良くなり、何を払うか
- **Falsifier**: 何が起きたらこの決定を撤回するか
- **References**: 一次情報の URL
```

<a id="規則"></a>

## 規則

1. **Status は嘘をつかない。** 実装が決定と食い違ったら、ADR を改訂するか
   Superseded にする。放置された ADR は SPEC.md §13 の再演になる。
2. **却下した案は消さない。** 却下理由こそが再演を防ぐ資産である。
   後の証拠で却下が誤りと分かったら、新しい ADR で supersede し、
   **なぜ前回の判断が誤ったか**を書く。
3. **Falsifier は必須。** 撤回条件を書けない決定は、決定ではなく好みである。
4. **一次情報を引く。** 「〜と言われている」ではなく、仕様書・公式ドキュメント・
   ソースコードの URL と逐語引用で書く。

<a id="読み方と後続決定への案内"></a>

## 読み方と後続決定への案内

ADR 本文は決定時点の証拠・却下理由・改訂履歴です。`Accepted` は設計の採択を表し、全項目の実装完了を表しません。本文に残る「未実装」や古い API は、後日の追記・後続 ADR と合わせて読んでください。現行の言語仕様は [specs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/)、公開関数は [標準ライブラリ](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/README.md) を参照します。

以下は読み違いを防ぐ案内です。ADR 本文を改変せず、未決の設計を新たに採択・撤回するものではありません。

| 記録 | 後続決定・実装状況の読み方 |
|---|---|
| 0001 | `fan.race` の名前・時計引数・計測コストの範囲は [0028](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md) の採択済み決定を参照。時間単位自体の決定は保持します。 |
| 0002・0003・0012 | 明示的な伝搬は [0008](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0008-explicit-propagation-only.md)、型付きエラーと lambda の合流は [0012](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0012-typed-error-refinement-in-the-marker.md)・[0021](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0021-lambda-failure-channel-is-the-join-of-its-bangs.md)。0003 の厳格な `CustomE → String` 拒否と実装の差は 0021 D5・Falsifier 3 に記録されたままで、この索引では解決しません。 |
| 0004 | `result.context` の記述は D2 の却下と Alternatives 7 を併読してください。現行の公開関数は [result](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/result.md) で確認できます。 |
| 0012・0021 | 冒頭の実装前の説明だけで未着手と判断しないでください。型付きマーカーと callback の伝搬は [typed_error_callback_channel_test.almd](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/spec/lang/typed_error_callback_channel_test.almd) などで検証されています。 |
| 0020・0023 | 0020 §5.3 の export 形は [0023](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0023-wasm-http-over-wasi-http.md) が置き換えます。本文の後日の実装注記と [Wasm build 経路](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/src/cli/build_wasm.rs) を併読してください。serve export の実装を、HTTP client の全経路や host-op 廃止の完了と同一視しないでください。 |
| 0025・0026 | 0025 §2.4 の `process` / `Env` 分類は 0026 D2 の後日の実装注記と [effect category 定義](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-ir/src/effect.rs) を参照。0026 の冒頭の「fixpoint 以外未実装」は後日の追記より前の状態です。 |
| 0026 の外部関数境界への参照 | 最終追記の「ADR-0027 §5」は参照先未解決です。[0027](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0027-effect-functions-are-inspected-in-a-fixed-host-world.md) は検査ハーネスの決定であり、外部関数境界の根拠には使えません。代わりの規範をこの索引で推定しません。 |
| 0029 | Status と Open の 2026-10-18 の裁定を参照。冒頭に残る「questions … still need a ruling」は裁定前の説明です。却下された test-level 属性案は、別の現行 ADR ではなく本文の却下案として残っています。 |

番号 0013 は異なる 2 記録に使われています。既存リンクを保つためリネームせず、下の索引ではタイトルで区別します。

<a id="index"></a>

## 索引

| # | タイトル | 状態 | 日付 |
|---|---|---|---|
| [0001](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0001-deterministic-time-units.md) | 決定的な予算は時間単位で記述する | 採択済み | 2026-08-01 |
| [0002](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0002-fallibility-effect-orthogonal.md) | 失敗可能性と effect は直交する軸であり、`-> T!` は pure-fallible を示す | 採択済み (2026-09-24 改訂: effect は fallible を含意する、§D6) | 2026-08-05 |
| [0003](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0003-error-type-conversion-at-propagation.md) | 伝搬地点でのエラー型変換 — 情報を失う変換は明記しなければならない | 採択済み | 2026-08-05 |
| [0004](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0004-error-branchability-doctrine.md) | エラーの分岐可能性 — String は終端のままとし、タグではなく原則・lint・context を用いる | 採択済み | 2026-08-05 |
| [0005](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0005-operators-desugar-to-stdlib.md) | 値レベルの演算子は名前付き stdlib 関数への脱糖である | 採択済み | 2026-08-05 |
| [0006](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0006-fallibility-polymorphic-hofs.md) | HOF の 1 ビット失敗可能性多相性 — try_* 系列を解消する | 採択済み (2026-09-27 改訂: E は callback から流れ、D4 は撤回 — ADR-0021) | 2026-08-05 |
| [0007](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0007-partition-replaces-collect.md) | result.collect を削除し、全エラー収集の実体は partition とする | 採択済み | 2026-08-05 |
| [0008](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0008-explicit-propagation-only.md) | 伝搬は常に明示的 — auto-? を廃止し、`!` のみとし、Result は must-use とする | 採択済み | 2026-08-05 |
| [0009](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0009-fn-type-quadrant-transparency.md) | 関数型で能力の 4 象限を完成させ、fn パラメータは既定で透過的にする | 採択済み | 2026-08-06 |
| [0010](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0010-option-type-shorthand.md) | T? は汎用の Option 省略記法 — すべての型位置で使え、原子的に強く結合し、fmt で正規化する | 採択済み | 2026-08-06 |
| [0011](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0011-execution-substrate-is-a-free-variable.md) | 実行基盤は自由変数であり、1 つの arm の出力が決定性の主張に残る最後の穴である | 提案中 (2026-10-04 改訂: wasm 計算の並列性はインスタンス単位で、メモリは決して共有しない、§D2a) | 2026-08-07 |
| [0012](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0012-typed-error-refinement-in-the-marker.md) | エラー表面の最終形 — 詳細化はマーカー（`T!E`）に残し、消去を既定のままとする | 採択済み (D4-4 は ADR-0021 で置換) | 2026-08-11 |
| [0013](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0013-corpus-stays-id-keyed.md) | 適合性コーパスは契約 ID をキーとしたままにする — 意味別の再編を却下し、命名規律をゲートで検査する | 採択済み | 2026-08-20 |
| [0013 (String)](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0013-string-shared-immutable-buffer.md) | String を共有不変バッファにし、変更操作には copy-on-write を用いる | 採択済み | 2026-09-04 |
| [0014](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0014-expectations-are-derived.md) | 固定された期待値は機械導出するか、置かない — 手作業では決して維持しない | 採択済み | 2026-08-20 |
| [0015](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0015-reference-evaluator-is-fresh-source-level-python.md) | 参照評価器は、ブラックボックスプロトコルの背後にある新規のソースレベル・判定者所有の評価器 — λ_almd を出発点とし、abstain 台帳で後退を防ぐ（改訂: Rust） | 採択済み | 2026-08-21 |
| [0016](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0016-numeric-determinism-is-normative.md) | 数値的決定性は一度だけ記述する 1 つの規範的系列であり、ターゲットがたまたま共有する性質ではない | 採択済み | 2026-08-21 |
| [0017](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0017-flight-profile-is-a-normative-subset.md) | bounded profile（旧称 flight profile）は言語の規範的部分集合であり、checker が強制する前にここで規定する（裁定時の改訂: `@bounded`、章 `bounded.md`、接頭辞 `B`） | 採択済み | 2026-08-21 |
| [0019](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0019-suppression-is-announced-dated-reasoned-acknowledged.md) | 抑制は告知・日付・理由・了承を伴う — 構文が存在する前にその形を固定する | 採択済み | 2026-09-22 |
| [0020](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0020-http-serving-and-handler-state.md) | serve される handler は `var` に到達できない — 状態はホスト非依存の KV store に置き、app は値、各ホストは薄いシェルとする | 採択済み (§5.3 の export 形は ADR-0023 が置き換える) | 2026-09-27 |
| [0021](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0021-lambda-failure-channel-is-the-join-of-its-bangs.md) | lambda の失敗チャネルは、その `!` が合意するエラー型を運ぶ — 文脈型を優先し、次に合流、String を頂点とする | 採択済み | 2026-09-27 |
| [0022](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.md) | 出力と abort の組み込みは pure fn 内で許可する — 書き込みと abort のみで読み取りはなく、呼び出し元を effectful にしない | 採択済み | 2026-09-27 |
| [0023](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0023-wasm-http-over-wasi-http.md) | Wasm HTTP は `wasi:http@0.3` — HTTP を使うプログラムは p3 component としてビルドし、組み込みホストは同じ import を提供し、独自 HTTP op を提供するホストは存在しない | 採択済み | 2026-09-28 |
| [0024](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0024-fan-effect-callbacks-run-concurrently-as-if-sequential.md) | `fan` はすべてのターゲットで effect callback を、逐次実行と同じように並行実行する — `fan.map` は全要素を実行し、コンパイラが基盤を選び、書き手に並行性の概念を持ち込まない | 採択済み | 2026-09-30 |
| [0025](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0025-wasm-process-is-a-private-host-capability.md) | Wasm の `process` は非公開のホスト能力 — `almide:process/spawn` は op 集合が名指す場合のみ含まれ、標準ホストはロード時に拒否し、`[permissions] proc` が制限する | 採択済み | 2026-10-03 |
| [0026](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0026-effect-categories-ride-the-transparent-fn-type.md) | effect category は透過的 fn 型に乗る — 集合は注釈なしで callback を流れ、2 階層を持ち、推論 ⊆ manifest ⊆ host が 1 本の連鎖を成す | 採択済み (D1–D4) | 2026-10-03 |
| [0027](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0027-effect-functions-are-inspected-in-a-fixed-host-world.md) | effect 関数は固定されたホスト世界で検査する — seed 付き sandbox、ホストが決める事柄への性質判定、ネットワーク用 loopback fixture | 採択済み (D1–D8, 2026-10-17) | 2026-10-17 |
| [0028](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md) | `fan.any` を `??` / `list.find_map` で置き換え、`fan.race` を時計必須の `fan.min` に改名し、コストの範囲を meter のバージョンに限定する | 採択済み | 2026-10-18 |
| [0029](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0029-assert-aborts-runs-its-closure-in-a-child.md) | `testing.assert_aborts(expected, closure)` は子プロセスまたは新しいインスタンスでクロージャだけを実行する — クロージャは何もキャプチャせず、すべての abort が数えられ、`testing.assert_throws` は dialect epoch 14 で退出する | 採択済み (2026-10-18) | 2026-10-17 |
| [0030](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0030-compile-cost-is-a-contract.md) | コンパイルコストは契約 — 各 phase に計数可能な予算があり、ソースは引き上げられず、超過は定義された診断となる | 採択済み | 2026-10-24 |
| [0031](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0031-raw-pointers-carry-provenance.md) | プログラムが読み取りに使えるポインタは由来を保持する — `RawPtr` を不透明な `Handle` と境界検査付き `Region` に分け、外部ポインタは `@extern` が長さを示す場合に限り `Region` となる | 採択済み | 2026-10-27 |
| [0032](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0032-a-response-content-type-is-its-body-type.md) | レスポンスの Content-Type は本文の型 — `Body` が各本文型に media type を与え、ヘッダー書き込みで Content-Type を指定できず、スクリプトを扱える型はその安全なモジュールだけが構築する | 採択済み | 2026-10-29 |

## 原本に関する注意

原本自身が採択と実装を区別し、未解決の差異や参照を記録し、0013 の 2 記録を保持しています。この読者版もそれらの区別と 0018 の欠番をそのまま保持します。解決を加えたり、外部関数の規範を推定したり、歴史的な実装注記を現在の保証に変えたりしません。
