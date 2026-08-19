# ALS — データ形式（Data Formats）

JSON / 正規表現 / バイナリ形式の観測規範。参照方法は [strings.md](strings.md)
冒頭と同じ。

## ALS-D1 JSON パス操作

`json.get_path`・`set_path`・`remove_path` のエッジケース（欠損キー・配列
範囲外・型不一致ノード・空パス）は infallible native oracle（serde_json 上の
参照実装）と観測等価: 欠損は none / no-op に縮退し、trap しない。
Contracts: C-031。

## ALS-D2 Value の JSON テキスト表現

動的 `Value` の文字列化はその **JSON テキスト**と byte 一致する（キー順・
数値表現・エスケープを含む）。裸でも Repr レコードのフィールドとしても同形。
Contracts: C-060。

## ALS-D3 異種ネスト文書の走査

異種ネスト JSON（glTF 級: 配列の配列・混在型・深いネスト）のパースと
要素単位の走査（`as_array` / 添字 / フィールド）は両ターゲットで byte 一致。
Contracts: C-063。

## ALS-D4 正規表現エンジン

正規表現エンジン（match / find / replace / captures）は native エンジンと
**fuzz された文法全域で** byte 一致する。方言差（PCRE vs RE2 等の齟齬）は
不適合 — 対応構文は単一の規範文法。
Contracts: C-032。

## ALS-D5 半精度浮動小数のデコード

`bytes.read_f16_le` は IEEE-754 binary16 を f64 へ正確に拡張する（subnormal・
±inf・NaN・±0 を含む）。
Contracts: C-037。

## ALS-D6 Codec と JSON デコード

JSON 数値・`\u` エスケープのデコード、整数形数値の f64 への拡張、
`json.stringify_pretty` のインデント出力、derive された Codec のクロス
モジュール dispatch、動的 Value モデル（merge / array 往復）、および
decode エラーメッセージの文言はターゲット間で byte 一致する。
Contracts: C-084, C-085, C-087, C-095, C-098, C-103。

## ALS-D7 バイト列ブリッジ

RawPtr / 線形メモリのバイト移動と `bytes.from_list(List[Int])` は値を
そのまま写す（切り詰め・符号化けは不適合）。
Contracts: C-062, C-090。
