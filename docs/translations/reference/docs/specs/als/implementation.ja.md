<a id="als--実装不変量implementation-invariants"></a>

# ALS — 実装不変量（Implementation Invariants）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](implementation.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](implementation.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](implementation.zh-CN.md)

> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/implementation.md)の非規範的な読者用コピー。以下の規範性・要件は原本に属し、このコピーに独立した規範性はありません。自動同期せず、歴史的な実装・適合・安全性の主張をこの翻訳作業で独立に再検証していません。コードと規則識別子は保持しています。

> Last updated: 2026-10-27

観測（stdout/stderr/exit）に直接は現れないが、メモリ安全・資源・決定性を
支える実装規範。契約台帳から `spec` キーで参照される。

<a id="als-i1-参照カウント規律perceus"></a>

## ALS-I1 参照カウント規律（Perceus）

heap 値の RC は Perceus 規律に従う: pass-through コンビネータは自分の
参照（+1）を返し、補間・spread・Value 構築の共有は +1 で釣り合い、
WASM heap は既定で回収される（プログラム終了時のリーク 0 が規範 — 例外は
C-066 自身が刻む TCO 自己呼び出しループの反復あたりリークで、Stage C までの
既知の残債として statement に記録されている）。
検証は Lean 証明・emit-time Σ-probe・RC カウンタ fixture が担う。
Contracts: C-041, C-066, C-071, C-086, C-121, C-122, C-146, C-149。

<a id="als-i2-コンパイラの決定性と資源"></a>

## ALS-I2 コンパイラの決定性と資源

コード生成はホストアーキテクチャに依存せず決定的（同一入力 → byte 同一
出力）。コンパイルは幅広・深いネスト入力で native スタックを溢れさせない
（専用 256 MiB コンパイルスレッド + 文連鎖の反復パス — C-059; 深い式ネストは
パーサの MAX_DEPTH=500 でクリーンに拒否され、そもそもコンパイルに入らない）。
Contracts: C-040, C-059。

<a id="als-i3-v1-lowering-エッジの等価証拠"></a>

## ALS-I3 v1 lowering エッジの等価証拠

v1（MIR）経路特有の lowering エッジ — scalar 値の端形・tuple/list Ok payload
の往復・自作 base64・条件 keep/skip の filter_map・借用パラメータの所有束縛
クラスタ・stdlib 呼び出し payload の ctor 直束縛・heap-Ok Result の値系
コンビネータ・ctor の if-payload・scalar リストリテラルの非 silent-empty 保証
— は v0 と byte 一致する（v1 検証行脚の固定化契約群）。
self-host モジュールを 2 つ以上リンクする際、`__` 始まりのモジュール内ヘルパが
同名衝突した場合は**名前一致でマージしない** — 正直な link エラーで壁にする
（マージは呼び出し側を誤シグネチャに再束縛し、`Ok` 応答後に wasm 検証で落ちる）。
test モードでは、各 `test` ブロックは MUTABLE なモジュール top-let を宣言時の
初期化子から観測する（native はスレッド分離、wasm はテスト毎の再初期化）。
Contracts: C-075, C-107, C-109, C-116, C-120, C-138, C-139, C-143, C-144, C-152, C-212, C-321。

<a id="als-i4-コンパイルコストの予算"></a>

## ALS-I4 コンパイルコストの予算

コンパイラは信頼できない入力（PR の CI、ファイルを開いた瞬間の LSP、採点器、
playground）を処理する。入力に比例して仕事が増えるフェーズは、それぞれ
名前付きの**予算**に縛られる。一覧は `almide budgets` が出す表（`docs/specs/cli.md`
に埋め込まれる）で、行は `nesting`（1 つの式の入れ子の段数）・`type_size`（1 つの
推論型のノード数）・`generic_instances`（有界ジェネリック関数の特殊化インスタンス
数。各インスタンスの型引数も `type_size` に縛られる）・`exhaustiveness_steps`
（1 つの `match` の網羅性検査の手数）・`manifest_bytes`（`almide.toml` /
`almide.lock` 1 つの大きさ）・`dependency_count`・`dependency_depth` である。

- 予算は**数える**もので、時間や空きメモリでは測らない。同じ入力は、どの機械でも
  同じ判定になる。
- 属性・マニフェストのキー・ロックの項目・環境変数のどれも予算を変えない。
  変えられるのはコマンドラインの `--budget NAME=N` だけで、
  `almide check --profile critical` はこのフラグを拒否する。
- 予算を超えた入力は、その予算の診断と非ゼロの終了コードで終わる。クラッシュ・
  メモリ不足による強制終了・ハングにはならない。`type_size` は E111
  `type too large (N+ nodes, max M)`（型が予算を超えた式または制約の位置）、
  `generic_instances` は E112、`exhaustiveness_steps` は E113
  `match too complex to check for exhaustiveness`、`nesting` は構文エラー
  `expression nesting too deep (max N)`、プロジェクト側の 3 つはファイル・数値・
  予算を名指すエラーで終わる。予算を超えた型を推定で通すことはなく、検査を
  終えられなかった `match` を網羅的とみなすこともない。
- 型は構造を共有する。`type_size` を外しても、`let t(i) = (t(i-1), t(i-1))` を
  n 回（直接、または `fn dup[T](x: T) -> (T, T)` を通して）重ねたプログラムは、
  n に線形な時間とメモリで検査を通る。

テスト: `tests/diagnostics/e111-type-doubling-chain/`,
`tests/diagnostics/e111-type-doubling-through-a-generic/`,
`tests/diagnostics/e113-bool-matrix-too-complex/`, `tests/compile_budget_test.rs`。
Contracts: C-440。

> 原文の文脈：日付付きの要件には、Stage C までの C-066 の反復ごとのリーク例外と、強いコンパイル資源の主張が含まれます。原文の要件・歴史的記述として保持しており、あらゆるコンパイラビルドや資源環境に対して独立に確立した保証ではありません。
