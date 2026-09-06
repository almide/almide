# ALS — データ形式（Data Formats）

> Last updated: 2026-08-27

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

`: Codec` を宣言した型 `T` は静的メソッド `T.encode` / `T.decode` を持つ。
レコードの encode は宣言順に走り、`none` のフィールドを省略する（明示
null は出さない。`Value?` の `some(null)` だけが明示 null になる）。
default 付きフィールドは実値をそのまま出し、`Value` フィールドは素通し。
decode は宣言順にフィールドを walk して最初のエラーを返す: object 以外に
`expected Object`、必須キーの欠落に `missing field '<key>'`、型不一致に
Value variant 名を使う `expected Int` / `expected Float` / `expected Str` /
`expected Bool`（C-084 の語彙）。未知キーは無視する。Option フィールドは
欠落も明示 null も `none` に畳むが、`Value?` は三状態（欠落 → `none`、
明示 null → `some(null)`、値 → `some(v)`）。default 付きフィールドは欠落
時に default 値を取る。`Float` フィールドは整数形 JSON 数値を f64 へ拡張
して受け（C-085）、`List[U]` は配列を要素ごとに decode して最初の err を
そのまま表面化する（インデックス修飾は付かない）。ネストした Codec
レコードは再帰し、レコード位置の非 object は `expected Object`。variant
の Codec は externally-tagged（`{"Case": [payload…]}`、unit case は
`{"Case": null}`）で、未知タグは `unknown variant for <T>` を返す。挙動の
行列は C-298 の corpus — `spec/wasm_cross/codec_decode_errors.almd`・
`spec/wasm_cross/codec_none_omission.almd`・
`spec/wasm_cross/codec_variant_roundtrip.almd`・
`spec/wasm_cross/codec_extra_keys.almd`・
`spec/wasm_cross/codec_float_int.almd` — が固定する。
Contracts: C-084, C-085, C-087, C-095, C-098, C-103, C-209。

## ALS-D7 バイト列ブリッジ

RawPtr / 線形メモリのバイト移動はバイト値をそのまま写す。
`bytes.from_list(List[Int])` はバイト域(0..=255)の値をそのまま写し、域外は
**mod-256 でラップ**する(`[65, 300, -1]` → 65, 44, 255 — 0.59.1 両ターゲット
実測。全 Int 域の無切り詰め規範は存在しない)。
Contracts: C-062, C-090。
