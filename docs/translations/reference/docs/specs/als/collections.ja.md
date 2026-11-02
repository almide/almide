<a id="als--コレクションcollections"></a>

# ALS — コレクション（Collections）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](collections.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](collections.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](collections.zh-CN.md)

> 正本：[docs/specs/als/collections.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/collections.md)、固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`。非規範的な読者向けスナップショットであり、元の ALS や検査器・台帳・生成器の入力を置き換えず、自動同期もしない。要求・歴史的状態・証拠・安全性の主張は原文の範囲を保つ。この版で独立の実行・計測・認証・証明は行っていない。原文言語版は読者用コピー、他言語版は翻訳である。

> 原文の範囲に関する注記：ALS-C5 はここで `mut` 引数を論じず、関数引数の一般的な値意味論を記す。この記述を保持し、別途の明示的な書き戻し契約を否定する意味へ拡張しない。定数時間の主張は ALS-T31 の範囲を保ち、入力長非依存やプログラム全体の時間情報の保証は追加しない。


> Last updated: 2026-10-27

コレクション型（List / Map / Set / タプル / レコード）の観測規範。参照方法は
[strings.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/strings.md) 冒頭と同じ（契約台帳の `spec` キーから参照される）。

<a id="als-c1-map-の順序規範"></a>

## ALS-C1 Map の順序規範

Map は**挿入順を保存する順序付き辞書**である。`keys`・`values`・`entries`・
イテレーションは常に挿入順。既存キーへの再代入は順序を変えない。削除後の
再挿入は末尾に付く。ハッシュ順・ソート順に依存する実装は不適合。
Contracts: C-013。

<a id="als-c2-set-の順序規範"></a>

## ALS-C2 Set の順序規範

Set は挿入順を保存し、イテレーション・`to_list` は決定的に挿入順を返す。
重複挿入は順序を変えない。
Contracts: C-014。

<a id="als-c3-構造的等価"></a>

## ALS-C3 構造的等価

`==`/`!=` は**構造的深い等価**である: リスト・タプル・レコード・Map・Set・
Option/Result・ネストした複合値は、要素ごとに再帰的に比較される。参照
（ポインタ）比較は不適合。Float 成分は IEEE-754 ビット比較でなく数値比較
（NaN != NaN）。
`Secret[String]` と `Secret[Bytes]` の `==` は中身を定数時間で比べる（ALS-T31）。
それ以外の `Secret[A]`、Secret を含む variant・Map・Set の `==` は検査時のエラー
E103 で拒否する（`spec/wasm_cross/secret_eq_and_decode.almd`）。
Contracts: C-015, C-426。

<a id="als-c4-範囲外アクセスの縮退規則"></a>

## ALS-C4 範囲外アクセスの縮退規則

読み取り系の範囲外は**安全に縮退**する: `list.get` は none、`slice` は
範囲を clamp、`take`/`drop` は空/全体へ縮退。負値は **0 として読む**
（ALS-T16 の負の個数規則、almide/almide#4147）: `slice` の負の start・負の
end はどちらも 0、`take(xs, 負)` は空・`drop(xs, 負)` は全体。
`list_slice_oob.almd` と `negative_count_rule.almd` が固定する（以前の
「負の start → 空・負の end → len・`take(xs, 負)` は全体」は `as usize` の
再解釈から来たもので、`?limit=-1` が件数の上限を外したため撤回した）。
逆転した範囲は空。
いずれもヒープ外アクセスや trap を起こしてはならない
（`xs[i]` 添字**構文**の範囲外 abort は ALS-T6 が別途規定する）。
Contracts: C-034。

<a id="als-c5-値意味論copy-on-write"></a>

## ALS-C5 値意味論（copy-on-write）

`var b = a` の後の `b` への変異は `a` に**観測されない**（値意味論）。実装は
copy-on-write 等で共有してよいが、エイリアス経由の変化が観測されたら不適合。
関数引数も同様（呼び出し先での変異は呼び出し元に見えない）。
Contracts: C-033, C-150。

<a id="als-c6-型変換コンビネータ"></a>

## ALS-C6 型変換コンビネータ

`map.map`・`set.map` は要素型を**変えられる**（`Map[K,A] → Map[K,B]`、
`Set[A] → Set[B]`）。結果のコレクションは変換後の型の等価・順序規範
（ALS-C1/C2/C3）に従う。
Contracts: C-039。

<a id="als-c7-レコード変種パターンマッチ"></a>

## ALS-C7 レコード・変種・パターンマッチ

レコード（named / anonymous）・変種（ADT）の構築・フィールド読み・
パターンマッチ（リテラル / ctor / タプル / ガード / ネスト）の評価結果は
ターゲット間で観測等価。マッチは上から順に最初に一致したアームを実行する。
Contracts: C-036。

<a id="als-c8-幅指定整数フィールド"></a>

## ALS-C8 幅指定整数フィールド

幅指定整数型（Int8/16/32 等）のフィールドに置くリテラルは**宣言幅へ
狭窄**される(C-038 は域内リテラルの表現狭窄を約束する)。宣言域を超える
リテラルは **E024** で拒否される — ラップしない(ALS-M14 の域規律)。
読み出しは宣言幅の値そのもの(`Int8` は `Int8`)で、暗黙の数値拡大は無い —
`Int` が要る位置へは `int.from(x)` で明示変換する(0.59.1 実測)。
Contracts: C-038。

<a id="als-c9-順序系コンビネータの全域性"></a>

## ALS-C9 順序系コンビネータの全域性

`list.min/max/sort/sort_by/unique_by` は**要素型指向**で全域: Int/String は
全順序、**Float は IEEE-754 totalOrder**（NaN は符号・生成元によらず唯一の
+qNaN 0x7FF8000000000000 として +inf の後の最大、-0 < +0）で比較し、
NaN 混入でも決定的に完走する（パニック・未定義順は不適合）。String キーの `sort_by` は要素が heap でも安定順序で
全域（結果は各要素を co-own する）。
Contracts: C-053, C-055, C-276。

<a id="als-c10-空コレクションの型要件静的規範"></a>

## ALS-C10 空コレクションの型要件（静的規範）

要素型が推論できない空コレクション（`[]` 単独の fold 種数不足等）は
**コンパイルエラー**である（実行時の任意挙動ではない）。fold の初期値は
**蓄積子の型**と単一化される(要素型とは独立 — `list.fold([1,2,3], "", …)`
は適合し "123" を返す)。空コレクションは要素型の注釈を要する。
Contracts: C-052, C-058。
