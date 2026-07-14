# ALS — 実行時規範（Runtime）

プログラム実行の観測規範（エラー終了・文字列補間の表示形・並行コンビネータ）。
参照方法は [strings.md](strings.md) 冒頭と同じ。

## ALS-R1 effect-main のエラー終了形

`effect fn main` が Err で終わるとき、stderr に `Error: <メッセージ>` を1行
出力し **exit code 1** で終了する。Ok 終了は exit 0。パニック級の異常
（ALS-T6 の abort を含む）も同じ `Error:` 接頭辞と exit 1 に統一される。
Contracts: C-035。

## ALS-R2 補間の表示形

文字列補間 `"${v}"` の表示形は型ごとに規範化される:

- **コンテナ**（List/Map/Set/タプル/Option/Result）: Almide リテラル形
  （`[1, 2]`、`("a", 1)`、`some(3)` 等）。ネストも再帰的に同形。
- **レコード/変種**: `TypeName { field: v, … }` — フィールドは**宣言順**。
  anonymous record はフィールド名の辞書順。再帰・ジェネリック ADT は
  インスタンス化ごとに同じ規則。
- **裸の Float**: Display は整数値の `.0` を落とす（`3`）。`float.to_string`
  は保持する（`3.0`）。この2形の区別は規範である。
Contracts: C-008, C-009, C-010, C-011。

## ALS-R3 fan 並行コンビネータの決定性

`fan.race`・`fan.any`・`fan.map`・`fan.settle` の結果は**リスト順で決定的**
（最初に完了したものではなく、引数リストの先頭から評価した最初の該当）。
エラーは ALS-R1 の統一 abort 形で表面化する。`fan.timeout` は言語に**存在
しない**（0.29.0 で削除）: 壁時計デッドラインは可搬なクロスターゲット意味を
持たず、参照は両ターゲット共通の check 時 tombstone エラー（E027）になる。
デッドラインはプログラムを起動するホスト境界で課す。
Contracts: C-004, C-005, C-006。

## ALS-R4 非有限浮動小数の定数表示

const 畳み込みで生じた非有限値（inf / -inf / NaN）は名前付き定数として
表示される（`inf`・`-inf`・`NaN`）。ビットパターンや `1e999` 形は不適合。
Contracts: C-012。

## ALS-R5 プロセス環境

`process.args` / `env.args` は argv[0]（プログラム名）を除いた引数列を
返し、両ターゲットで一致する。`random.int(a, b)` は WASI entropy 下でも
常に [a, b] 範囲内。
Contracts: C-096, C-112, C-118。

## ALS-R6 ファイルシステムのパス解決

wasm の fs ランタイムは起動時に WASI preopen ディレクトリ表を構築し、
絶対パスを最長一致 preopen + 相対残りに解決する（`./` 正規化込み）。
同一パスへの書き込み→読み戻しは native std::fs と同じホストファイルに
到達する（CWD 非依存）。open エラー文言は ALS-T6 系の native 文言規範
に従う。
Contracts: C-042。
