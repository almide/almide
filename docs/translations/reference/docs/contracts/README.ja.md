<a id="almide-behavior-contracts"></a>

# Almide の動作契約

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)

> 非規範的な固定履歴の読者版。正本は不変更で権威を維持。以下の規範的/証明済み/安全/リークなし/正確/target間一致の記述は原契約の主張であり、新規検証や外部認証ではありません。本翻訳ではコマンド・テスト・証明を実行せず、自動同期もしません。388契約の刻印は2026-10-21時点ですが原文にはC-448まで後の行があります。fixture数0や証拠クラスは推測せず保持。C-006対C-208のfan.timeout、C-020対C-412のUnicode大小文字、旧境界挙動対後の規則といった歴史的緊張を保持し、普遍的現行保証へ勝手に統合しません。生成マーカーは不活性で、本版は正本生成表や規範的契約出典ではありません。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/README.md)

> 原本は [contracts.toml](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/contracts.toml) から自動生成。
> 更新には `bash docs/contracts/generate-readme.sh > docs/contracts/README.md` を実行。
>
> 各契約は、コンパイラが両 target（native Rust + wasm32：stdout、stderr、終了コード）で守る、規範的かつ観測可能な約束です。
> native が oracle であり、native == wasm は厳格な不変条件です。すべての契約は実行可能な証拠
> （`spec/wasm_cross/*.almd` fixture、差分 fuzz、出力時 Σ-probe、または Lean 定理）へ追跡できます。
> 主張される挙動で、説明文だけに依存するものはありません。

<a id="change-discipline"></a>

## 変更規律

- **観測可能な挙動を変えるには、同じ PR で契約文と証拠の両方を更新する必要があります。**
- **新しい**挙動 = 新しい `C-NNN` + ≥1 fixture。
- **乖離の除去** = 同じ PR で `status` を `active` にし、flag を外す。`flagged-for-revision` 件数はラチェットで、**減らす方向のみ**です。
- ゲート（`scripts/check-contracts.sh`、CI + lefthook）は、すべての契約に実証があり、各 fixture が契約を名指しし、リンクが双方向であることを強制します。
- **集計値は刻印され、PR ごとには再生成しません。** 下記の「N contracts」合計（および生成文書に引用する他の全合計）は、
  `proofs/ledger-counts.toml` から表示され、`counts:generated` ブロックに日付があります。fixture/契約の PR は行を再生成してもこのブロックは変えません。
  リリース封印前、または夜間の `scripts/check-ledger-counts.sh` がずれを報告したときに `bash scripts/gen-ledger-counts.sh` で更新します。

証拠クラス（弱い → 強い）：`doc-only` < `by-construction` < `fixture` < `fuzz` < `exhaustive` < `lean`。
**active** 契約は ≥1 個の、クラス ≥ `fixture` の証拠が必要です。

> 原本の生成集計ブロック（不活性な履歴表示）：2026-10-21 時点。proofs/ledger-counts.toml の刻印合計。scripts/gen-ledger-counts.sh のみが更新し、fixture/contract PR では更新しません。元のマーカー間は編集禁止です。本参照版は自動更新されません。
388 契約
> 原本の生成集計ブロック終端（不活性）。

| ID | 契約 | 導入版 | 状態 | 最強の証拠 | Fixture 数 |
|----|----------|-------|--------|--------------------|-----------:|
| C-001 | 整数のゼロ除算/剰余は全域的 — abort し、trap しない | 0.24.0 | active | fixture | 5 |
| C-002 | 符号付き MIN / -1 の溢れは、各幅の真の MIN で abort | 0.24.0 | active | fixture | 3 |
| C-003 | abort しない整数除算/剰余はバイト一致を保つ | 0.24.0 | active | fixture | 1 |
| C-004 | fan.map / fan.settle はリスト順で決定的 | 0.24.0 | active | fixture | 11 |
| C-005 | fan のエラー伝搬は統一された main-error abort として現れる | 0.24.0 | active | fixture | 5 |
| C-006 | [fan.timeout は存在しない — 実時間の期限は host 境界に置く](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/C-006-fan-timeout-removed.md) | 0.29.0 | active | fixture | 0 |
| C-007 | abort しうるトップレベル let は起動時に即時評価する | 0.24.0 | active | fixture | 4 |
| C-008 | [複合値の文字列埋め込みは Almide リテラルの repr を表示（コンテナ）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/C-008-009-010-repr.md) | 0.24.0 | active | fixture | 3 |
| C-009 | [record / variant / 無名 record の文字列埋め込み repr（フィールド整列）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/C-008-009-010-repr.md) | 0.24.0 | active | fixture | 4 |
| C-010 | [再帰的/総称 ADT の文字列埋め込み repr はインスタンス化をキーとする](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/C-008-009-010-repr.md) | 0.24.0 | active | fixture | 2 |
| C-011 | 裸の float 埋め込みの Display は .0 を落とし、float.to_string は残す | 0.24.0 | active | fuzz(100000) | 5 |
| C-012 | 定数畳込みされた非有限 float は名前付き定数を出力 | 0.24.0 | active | fixture | 1 |
| C-013 | Map は compact-ordered-dict：走査は挿入順 | 0.24.0 | active | fixture | 11 |
| C-014 | Set は挿入順で決定的 | 0.24.0 | active | fixture | 3 |
| C-015 | 複合要素とヒープ値の構造的な深い等価性 | 0.24.0 | active | fixture | 10 |
| C-016 | UTF-8 コードポイント対応の文字列演算はバイト一致 | 0.24.0 | active | fixture | 3 |
| C-017 | 空パターンの count / last_index_of は native のコードポイント/バイト意味論に従う | 0.24.0 | active | fixture | 2 |
| C-018 | Unicode 文字列述語は全定義域で Rust char メソッドと一致 | 0.24.0 | active | fixture | 1 |
| C-019 | rt_string_extra 演算（replace_first、strip_*、述語、cmp）は native と一致 | 0.24.0 | active | fixture | 2 |
| C-020 | Unicode 大小文字変換（to_upper/to_lower/capitalize）は Unicode 全体に対応 | 0.24.0 | active | fixture | 1 |
| C-021 | 空白 trim / is_whitespace は完全な Unicode White_Space プロパティを使う | 0.24.0 | active | fixture | 1 |
| C-022 | bytes.to_string_lossy は UTF-8 非可逆復号（bytes.from_string の逆） | 0.24.0 | active | fixture | 1 |
| C-023 | float.to_string は往復可能な最短十進表記で Rust Display とバイト一致 | 0.24.0 | active | fuzz(100000) | 2 |
| C-024 | float.parse は正しく最近接偶数丸めを行う（Clinger AlgorithmM） | 0.24.0 | active | fixture | 1 |
| C-025 | float.to_fixed は正確な二進値に対する中間値偶数丸め | 0.24.0 | active | fuzz(1000) | 1 |
| C-026 | 同梱 libm の trig / exp / log / pow は target 間でバイト一致 | 0.24.0 | active | fuzz(4000) | 3 |
| C-027 | base64 符号化/復号（標準＋URL-safe）はエラーを含めバイト一致 | 0.24.0 | active | fixture | 1 |
| C-028 | int.parse_radix(s, 16)：trim、任意の符号1個、任意の 0x / 0X 1個、続いて十六進数字 | 0.24.0 | active | fixture | 1 |
| C-029 | int.parse のエラー形態は native ParseIntError とバイト一致 | 0.24.0 | active | fixture | 1 |
| C-030 | hex.encode / hex.decode は位置を含むエラー詳細までバイト一致 | 0.24.0 | active | fixture | 2 |
| C-031 | value get/set/remove_path の境界例は、パス境界例で失敗しない native oracle と一致 | 0.24.0 | active | fixture | 3 |
| C-032 | Regex エンジンは fuzz した文法範囲で native エンジンとバイト一致 | 0.24.0 | active | fuzz(220) | 6 |
| C-033 | [別名を持つ可変値の値意味論（copy-on-write）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/C-033-cow-truth-table.md) | 0.24.0 | active | fixture | 22 |
| C-034 | 範囲外の list 操作は安全に丸め/無操作となる（範囲外ヒープアクセスなし） | 0.24.0 | active | fixture | 8 |
| C-035 | Effect-main エラーは統一終了：Error: <msg> ＋ exit 1 | 0.24.0 | active | fixture | 6 |
| C-036 | record、variant、パターン照合はバイト一致 | 0.24.0 | active | fixture | 10 |
| C-037 | bytes.read_f16_le は IEEE-754 半精度浮動小数点を同一に復号 | 0.24.0 | active | fixture | 1 |
| C-038 | 固定幅整数リテラルは宣言されたフィールド幅へ狭める | 0.24.0 | active | fixture | 4 |
| C-039 | 型を変える map.map / set.map は新型のコレクションを返す | 0.24.0 | active | fixture | 2 |
| C-040 | codegen の出力は host アーキテクチャについて決定的 | 0.24.0 | active | fixture | 3 |
| C-041 | ヒープ/RC プリミティブは Lean で証明された Perceus 規律を守る | 0.24.0 | active | lean | 5 |
| C-042 | fs の preopen-dir 走査＋パス解決は観測上等価 | 0.24.0 | active | fixture | 1 |
| C-043 | Box という利用者型は再帰 enum のヒープ間接参照と共存する | 0.24.0 | active | fixture | 1 |
| C-044 | Result/Option の構築と照合はバイト一致 | 0.24.0 | active | fixture | 6 |
| C-045 | List[String] パラメータは join / len / index / 走査を通して機能する | 0.24.0 | active | fixture | 3 |
| C-046 | record スプレッド更新とモジュール横断の単相化はバイト一致 | 0.24.0 | active | fixture | 1 |
| C-047 | 負の指数での int.pow は全域的 — abort し、trap も回り込みもしない | 0.24.0 | active | fixture | 2 |
| C-048 | int.wrap_* / int.rotate_* は64ビットレジスタ上で動き、int.wrap は bits >= 64 でマスクを飽和させる | 0.24.0 | active | fixture | 1 |
| C-049 | float.sign は f64::signum；float/math の min/max は NaN を無視 | 0.24.0 | active | fixture | 1 |
| C-050 | string.split(\"\") はコードポイント単位 | 0.24.0 | active | fixture | 1 |
| C-051 | math.log_gamma はビット一致（両 target が同梱 musl-libm log を使う） | 0.24.0 | active | fixture | 2 |
| C-052 | 空コレクションの fold にはコレクションの要素型が必要（codegen で既定化しない） | 0.24.0 | active | fixture | 1 |
| C-053 | list.min/max/sort/sort_by/unique_by は型指向で全域的、native == wasm | 0.24.0 | active | fixture | 5 |
| C-054 | List/string の Int 個数と添字は狭める前に i64 で範囲内に収める — 切詰めや範囲外アクセスなし | 0.24.0 | active | fixture | 11 |
| C-055 | Float 上の list.min/max/sort/sort_by は IEEE-754 totalOrder を使い、両 target で妥当かつ同一 | 0.24.0 | active | fixture | 3 |
| C-056 | list.product は i64 溢れで回り込み、list.sum と無印 `*` に一致 | 0.24.0 | active | fixture | 1 |
| C-057 | Unit を返すその場更新関数の結果の代入は両 target で checker エラー | 0.24.0 | active | fixture | 1 |
| C-058 | 要素型を推論できない空コレクションは両 target でコンパイルエラー、黙って既定化しない | 0.24.0 | active | fixture | 2 |
| C-059 | 幅広/深い入力でもコンパイルは native スタックを溢れさせず、全 host と build profile で同一 | 0.25.0 | active | fixture | 1 |
| C-060 | Value は単独でも Repr-record フィールドでも、native と WASM でバイト一致の JSON テキストとして repr 表示 | 0.26.7 | active | fixture | 2 |
| C-061 | その場変更する mut Map パラメータは両 target でビルドでき変更が持続、バイト一致 | 0.26.9 | active | fixture | 1 |
| C-062 | Region のバイト移動は両 target でバイト一致し、その外へのアクセスは err | 0.26.15 | active | fixture | 1 |
| C-063 | 異種要素を入れ子にした glTF/JSON 文書の解析と配列の要素ごとの走査は両 target でバイト一致 | 0.26.19 | active | fixture | 1 |
| C-064 | effect-fn の Result 自動 unwrap 規則は束縛位置を通じて同一かつ型指向、両 target でバイト一致 | 0.26.20 | active | fixture | 3 |
| C-065 | 文字列の位置 API は両 target で始めから終わりまでコードポイント添字 | 0.26.20 | active | fixture | 3 |
| C-066 | WASM ヒープは既定で回収される（真の Perceus） | 0.27.0 | active | fixture | 23 |
| C-067 | xs[i] 添字構文は範囲外で abort（読み書き両方） | 0.27.4 | active | fixture | 9 |
| C-068 | 構築位置の Auto-? は対象型指向 | 0.27.4 | active | fixture | 3 |
| C-069 | effect-fn の末尾自己再帰は両 target で O(1) スタックのループに変換 | 0.27.4 | active | fixture | 3 |
| C-070 | 入れ子コンストラクタパターンは両 target で同じ照合/束縛 | 0.27.6 | active | fixture | 15 |
| C-071 | 単一部分の文字列埋め込みの RC 釣合い | 0.27.6 | active | fixture | 1 |
| C-072 | 推論された名前付き record の repr 一致 | 0.27.6 | active | fixture | 2 |
| C-073 | variant コンストラクタを検査する tuple パターン | 0.27.6 | active | fixture | 1 |
| C-074 | 大きな入力に対する反復的 split/replace | 0.27.6 | active | fixture | 1 |
| C-075 | lowmisc 第5群：借用パラメータの所有束縛、effect-Option auto-try 除去、一致するエラーの ! 通過 | 0.27.6 | active | fixture | 3 |
| C-076 | 生成側の同一モジュール内 variant 構築は target 間で安定 | 0.27.6 | active | fixture | 3 |
| C-077 | モジュール横断のヒープグローバル初期化順は依存関係を守る | 0.27.6 | active | fixture | 1 |
| C-078 | phantom record の総称パラメータは Rust target で取り除く | 0.27.6 | active | fixture | 1 |
| C-079 | 異なる無名 record ペイロードを持つ variant case は target 間で安定 | 0.27.6 | active | fixture | 1 |
| C-080 | 空の map.from_list / set.from_list は結果型から要素型を解決 | 0.27.6 | active | fixture | 1 |
| C-081 | パラメータ型推論 lambda 内の総称 fn は型パラメータを解決 | 0.27.6 | active | fixture | 1 |
| C-082 | クロージャ型 lambda パラメータの呼出しはクロージャ自体でなく呼出し結果を返す | 0.27.6 | active | fixture | 1 |
| C-083 | 符号反転した i64::MIN リテラルは表現可能であり、ゼロに畳み込まれない | 0.27.6 | active | fixture | 1 |
| C-084 | Codec/value 復号エラーメッセージは target 間でバイト一致 | 0.27.6 | active | fixture | 3 |
| C-085 | Float 復号は整数の JSON 数値を f64 へ拡張 | 0.27.6 | active | fixture | 1 |
| C-086 | 通過型 stdlib コンビネータは結果に独自の参照を与える | 0.27.6 | active | fixture | 1 |
| C-087 | JSON 数値と \\u 文字列の復号は target 間でバイト一致 | 0.27.6 | active | fixture | 4 |
| C-088 | Rust キーワードの関数名は両 target でコンパイル可能 | 0.27.6 | active | fixture | 2 |
| C-089 | 先行パラメータを参照する既定パラメータにはその引数が補われる | 0.27.6 | active | fixture | 1 |
| C-090 | List[Int] パラメータの bytes.from_list は両 target でコンパイル可能 | 0.27.6 | active | fixture | 1 |
| C-091 | let 分解束縛内の入れ子サブパターンは全ての葉を束縛 | 0.27.6 | active | fixture | 1 |
| C-092 | 総称 record フィールドは構築時のインスタンス化型に従ってサイズを決める | 0.27.6 | active | fixture | 3 |
| C-093 | 相互再帰 variant 型は両 target でコンパイル可能 | 0.27.6 | active | fixture | 1 |
| C-094 | 型推論された lambda パラメータの protocol-method UFCS 呼出しは要素型を解決 | 0.27.6 | active | fixture | 1 |
| C-095 | json.stringify_pretty は target 間でバイト一致のインデント出力 | 0.27.6 | active | fixture | 2 |
| C-096 | env.args は WASM で動き native と一致 | 0.27.6 | active | fixture | 1 |
| C-097 | 型パラメータ上の総称 + は target 間で同じ文字列/リスト連結 | 0.27.6 | active | fixture | 1 |
| C-098 | モジュール横断の導出 Codec メソッドは WASM で dispatch され native と一致 | 0.27.6 | active | fixture | 0 |
| C-099 | 比較/等価演算子は v1 wasm 経路の全オペランド型で native とバイト一致 | 0.27.6 | active | fixture | 10 |
| C-100 | self-host String 分類/変換演算は wasm で native とバイト一致 | 0.27.6 | active | fixture | 4 |
| C-101 | ヒープ要素（String/Value）の List 演算は native とバイト一致し、リーク/二重解放なし | 0.27.6 | active | fixture | 18 |
| C-102 | List 走査、呼出し結果の要素具体化、末尾再帰リスト走査は native とバイト一致 | 0.27.6 | active | fixture | 5 |
| C-103 | self-host 動的 Value モデル（merge、array/as_array 往復、tuple TCO）は native とバイト一致しループ内でリークなし | 0.27.6 | active | fixture | 8 |
| C-104 | 末尾再帰 accumulator の形は native とバイト一致の有界スタックループへ lowering | 0.27.6 | active | fixture | 6 |
| C-105 | var/append accumulator ループ（scalar、所有ハンドル、相互依存、相互再帰）は wasm で native とバイト一致 | 0.27.6 | active | fixture | 7 |
| C-106 | if/match 分岐から束縛したヒープ値は v1 wasm 経路で native とバイト一致 | 0.27.6 | active | fixture | 17 |
| C-107 | ヒープ上の Result-of-tuple / Result-of-list の Ok ペイロードは往復し native とバイト一致 | 0.27.6 | active | fixture | 4 |
| C-108 | unwrap `!` と let-unwrap の脱糖は全位置で native とバイト一致 | 0.27.6 | active | fixture | 10 |
| C-109 | self-host base64 符号化は v1 wasm 経路で正準形/native とバイト一致 | 0.27.6 | active | fixture | 1 |
| C-110 | その場 bytes.push 更新 accumulator は v1 wasm で native とバイト一致 | 0.27.6 | active | fixture | 1 |
| C-111 | モジュールレベル const ヒープグローバルは v1 wasm と native で同じ初期化/読み取り | 0.27.6 | active | fixture | 2 |
| C-112 | random.int の抽出は v1 wasm の WASI エントロピー下限の下で同じく範囲内に保たれる | 0.27.6 | active | fixture | 1 |
| C-113 | let 束縛された ADT/Result variant の tag 照合は v1 wasm で native とバイト一致 | 0.27.6 | active | fixture | 2 |
| C-114 | ヒープペイロードを持つ Option の照合は v1 wasm で native とバイト一致 | 0.27.6 | active | fixture | 2 |
| C-115 | 値を返すブロック本体 lambda へのパイプは v1 wasm で native とバイト一致 | 0.27.6 | active | fixture | 2 |
| C-116 | v1 scalar-value lowering の境界例は native とバイト一致（末尾 Bool リテラル、float.parse inf/nan） | 0.27.7 | active | fixture | 2 |
| C-117 | ループ内 let 束縛のヒープ if/match は末尾 helper に持ち上げ、v1 で表示 | 0.27.7 | active | fixture | 3 |
| C-118 | env.args は WASM で動き native と一致（argv[0] を省略） | 0.27.8 | active | fixture | 1 |
| C-119 | `for` ループ本体内の effect-`!` は Err を伝搬し native とバイト一致 | 0.27.6 | active | fixture | 1 |
| C-120 | 条件で保持/省略する分岐本体を持つ捕捉付き filter_map は native とバイト一致 | 0.27.6 | active | fixture | 1 |
| C-121 | String 通過型高速経路は所有参照（+1）を返す | 0.27.6 | active | fixture | 2 |
| C-122 | Value object 演算は完全な list layout を確保し +1 でペアを共有 | 0.27.6 | active | fixture | 2 |
| C-123 | Record spread はコピーしたヒープフィールドと別名上書きを +1 で共有 | 0.27.6 | active | fixture | 1 |
| C-124 | Value の等価性は深い構造的等価性で native PartialEq を反映 | 0.27.6 | active | fixture | 1 |
| C-125 | bytes.set は値意味論 — 入力を通じて変更が観測されない | 0.27.6 | active | fixture | 1 |
| C-126 | 入れ子 lambda の HOF パラメータは推論リンクを保つ（literal sig-generic 固定なし） | 0.27.6 | active | fixture | 2 |
| C-127 | unwrap_or は連鎖の型が未解決なら既定値からペイロードサイズを決める | 0.27.6 | active | fixture | 2 |
| C-128 | datetime.format は全 backend で strftime 指定子を同じく置換 | 0.28.1 | active | fixture | 1 |
| C-129 | list.chunk / list.window の非正サイズ：負は昇格済み規範を保ち、ゼロは T6 形式で abort | 0.28.4 | active | fixture | 5 |
| C-130 | option/map コンビネータは所有ヒープ結果を返す（裸の通過ハンドルなし） | 0.28.5 | active | fixture | 2 |
| C-131 | ループ再構築バッファは O(n)：COW が守るのは生存する別名のみ、LICM はヒープ割当てを巻き上げない | 0.28.6 | active | fixture | 3 |
| C-132 | 再割当てするコンテナの mut パラメータは全呼出し位置で呼出し元へ持続 | 0.28.6 | active | fixture | 31 |
| C-133 | env.get は native と wasm で host 環境を同じく観測 | 0.29.0 | active | fixture | 1 |
| C-134 | 同梱 libm の atan / tanh は target 間でバイト一致 | 0.30.0 | active | fuzz(3000) | 1 |
| C-135 | Unit 宣言の effect fn ABI は定義と全呼出し箇所で一致 | 0.30.0 | active | fixture | 2 |
| C-136 | place のその場変更は両 target で次の読み取りまで持続 | 0.30.0 | active | fixture | 1 |
| C-137 | wasm の相対 fs パスは host CWD 基準で解決 | 0.31.0 | active | fixture | 1 |
| C-138 | stdlib 呼出しペイロードの ok/err ctor は実値を具体化 | 0.31.0 | active | fixture | 1 |
| C-139 | ヒープ Ok の Result 値コンビネータは tag とペイロードを保つ | 0.31.0 | active | fixture | 2 |
| C-140 | float.round はゼロ結果の符号を保つ | 0.31.0 | active | fixture | 2 |
| C-141 | list.zip_with は要素 repr で振分け — String の zip が動き、誤型リンクなし | 0.31.0 | active | fixture | 1 |
| C-142 | result.unwrap_or_else は Float インスタンス化の両経路で妥当な wasm | 0.31.0 | active | fixture | 1 |
| C-143 | ctor の if ペイロードは選ばれた分岐を具体化 | 0.31.0 | active | fixture | 1 |
| C-144 | scalar-list リテラルは黙って空リストとして観測されない | 0.31.0 | active | fixture | 1 |
| C-145 | 単相化接尾辞付き stdlib コンビネータ名は基底名で振分け | 0.31.0 | active | fixture | 1 |
| C-146 | 捕捉した別名を返す持ち上げ済みクロージャは共同所有参照を渡す | 0.31.0 | active | fixture | 1 |
| C-147 | list.unique_by は key repr で振分け — String キーは内容で重複除去 | 0.31.0 | active | fixture | 1 |
| C-148 | list.scan は各中間値を accumulator 自身の幅で保存 | 0.31.0 | active | fixture | 1 |
| C-149 | unwrap_or_else は共同所有のヒープ Ok ペイロードを返す | 0.31.0 | active | fixture | 6 |
| C-150 | ヒープ var を包む ctor は値コピー — var は生存し続ける | 0.31.0 | active | fixture | 1 |
| C-151 | ヒープ Ok を結果とする Result コンビネータは scalar 実装へリンクしない | 0.31.0 | active | fixture | 1 |
| C-152 | ctor 内の未受理ヒープ呼出しペイロードは wall で拒否し、ゼロ化しない | 0.31.0 | active | fixture | 1 |
| C-153 | テスト外 assert 失敗は両 target で T6 形式の abort | 0.31.0 | active | fixture | 6 |
| C-154 | 無効範囲の clamp は T6 形式で abort | 0.31.0 | active | fixture | 2 |
| C-155 | 定義域外の小数桁数での to_fixed は T6 形式で abort | 0.31.0 | active | fixture | 3 |
| C-156 | if で併合した some((String, String)) ctor は実際に追跡される Option | 0.32.0 | active | fixture | 1 |
| C-157 | 型注記なしの総称 ctor の top-let は解決済みペイロード型を全参照先へ運ぶ | 0.32.0 | active | fixture | 1 |
| C-158 | scalar 呼出しまたは tuple ペイロードの some/ok ctor は実値を具体化し、ゼロ ctor にしない | 0.32.0 | active | fixture | 5 |
| C-159 | list.binary_search は重複キーについて両 target で同じ添字を返す | 0.32.0 | active | fixture | 1 |
| C-160 | 純 Almide の同梱 stdlib モジュールは wasm でバイト一致でリンク/実行 | 0.34.4 | active | fixture | 1 |
| C-161 | Matrix コンストラクタの次元は負を丸め、共通上限を超えると abort | 0.35.0 | active | fixture | 14 |
| C-162 | io.write / io.write_bytes は println と交互でもプログラム順に出力 | 0.35.0 | active | fixture | 1 |
| C-163 | let/var に束縛したヒープ結果 if/match は両 target で選択分岐を実行 | 0.35.0 | active | fixture | 1 |
| C-164 | List 更新操作と接尾部分コピーは tuple / record / 入れ子 list 要素を共同所有 | 0.35.0 | active | fixture | 1 |
| C-165 | String キー map、String list、String set の fold は両 target でヒープ accumulator を受け渡す | 0.36.0 | active | fixture | 1 |
| C-166 | Map の文字列埋め込みは全 self-host key/value 組合せを両 target で表示 | 0.36.0 | active | fixture | 1 |
| C-167 | float.clamp は範囲内の入力を符号ビットも含めそのまま返す | 0.36.0 | active | fixture | 1 |
| C-168 | list.flatten は引数を借用し、同じ束縛の別の借用と組み合わせられる | 0.36.0 | active | fixture | 2 |
| C-169 | サイズ上限を超える list.repeat は両 target で T6 形式の abort | 0.36.0 | active | fixture | 1 |
| C-170 | 整数演算はモジュールレベル let を含む全位置で回り込む | 0.36.0 | active | fixture | 3 |
| C-171 | バイトオフセット境界検査は i64 境界で溢れない | 0.36.0 | active | fixture | 1 |
| C-172 | 任意のヒープペイロード上の unwrap_or は両 target で同じ値を返す | 0.36.0 | active | fixture | 2 |
| C-173 | 文脈で表現できる範囲外の整数リテラルは両 target で checker エラー、黙って値にしない | 0.36.0 | active | fixture | 2 |
| C-174 | 末尾再帰 Map/Set accumulator は空でない seed も含め両 target で seed を保つ | 0.36.0 | active | fixture | 1 |
| C-175 | variant 型の List リテラルは束縛位置でもヒープ結果 if 分岐位置でも両 target で同じ構築/破棄 | 0.37.0 | active | fixture | 1 |
| C-176 | インライン tuple 戻り呼出しを包む some/ok は両 target で実ペイロードを具体化 | 0.37.0 | active | fixture | 1 |
| C-177 | ループ本体の呼出し引数にある可変グローバル射影の読み取りは毎回現在のスロットを読む | 0.37.0 | active | fixture | 1 |
| C-178 | 相互末尾再帰の連鎖は wasm target で深さの上限なく動く | 0.37.0 | active | fixture | 2 |
| C-179 | UInt64 は宣言された全定義域に到達し、すべての観測側はスロットを符号なしで読む | 0.37.0 | active | fixture | 5 |
| C-180 | 固定幅整数の +、-、*、^ は両 target で宣言幅の回り込み | 0.37.0 | active | fixture | 1 |
| C-181 | args.positional は全非 flag 引数を返し、args API は target 間で一致 | 0.37.0 | active | fixture | 1 |
| C-182 | 符号反転した float リテラルは両 target で文脈の float 型を取る | 0.37.0 | active | fixture | 2 |
| C-183 | scalar record の list に対するループ持越し append は両 target で実行 | 0.37.0 | active | fixture | 1 |
| C-184 | 整数 `^` 演算子は両 target で int.pow と同じ全域的べき乗 | 0.37.0 | active | fixture | 1 |
| C-185 | `??` の連鎖は位置によらず両 target で原文順の最初の Ok を返す | 0.37.0 | active | fixture | 1 |
| C-186 | インライン添字読み取りを list accumulator に append すると両 target で lowering/実行 | 0.37.0 | active | fixture | 2 |
| C-187 | モジュールレベル `var` バッファを通じて書くその場更新関数は両 target で同一動作 | 0.37.0 | active | fixture | 6 |
| C-188 | scalar `if` 分岐は文の副作用を実行 — グローバル書込みが反映され外側 var 再代入は安定 local に届く | 0.37.0 | active | fixture | 1 |
| C-189 | env.os と temp-dir API は HOST を報告 — 等価則の唯一の例外で、閉じた集合 | 0.37.0 | active | fixture | 1 |
| C-190 | クロージャは Int/Bool だけでなく Float と全固定幅整数を含む任意の scalar を捕捉 | 0.37.0 | active | fixture | 1 |
| C-191 | モジュールレベル global は全分岐経路で実値を読む — 具体化メモは直線文脈に限定 | 0.37.1 | active | fixture | 2 |
| C-192 | 文字列埋め込み内の自動 unwrap される effect 呼出しは呼出し引数と同じく持ち上げ/伝搬 | 0.37.1 | active | fixture | 3 |
| C-193 | 可変 global 経由の反復その場書込みはその場に留まる — receiver は COW 後のハンドルを借用 | 0.37.1 | active | fixture | 1 |
| C-194 | 可変 global を宛先とする bytes.copy_from は記憶スロットを通じて書く | 0.38.1 | active | fixture | 1 |
| C-195 | checked-conversion 族に対する値位置の variant match は実行される | 0.38.1 | active | fixture | 1 |
| C-196 | 呼出しスタック枯渇は target ごとの深さにおける定義済み abort | 0.41.0 | active | fixture | 1 |
| C-197 | linear memory 枯渇は定義済み abort を伴う資源制限 | 0.41.0 | active | fixture | 9 |
| C-198 | head-count を受ける matrix 入口は dialect epoch 14 で stdlib から除去 | 0.42.0 | active | fixture | 0 |
| C-199 | fan ブロックは全 sibling を join し、リスト順で最初の Err を報告 | 0.42.0 | active | fixture | 2 |
| C-200 | fan sibling の trap は統一 main-error abort を通じて収束的に終了 | 0.42.0 | active | fixture | 3 |
| C-201 | Option コンビネータの tuple 結果は全要素型組合せで所有要素として具体化可能 | 0.44.0 | active | fixture | 1 |
| C-202 | 時間コンストラクタは定義域を守る：負は abort、溢れは飽和 | 0.47.0 | active | fixture | 3 |
| C-203 | 時間型の演算子代数は両 target で単位が正確かつ飽和 | 0.47.0 | active | fixture | 1 |
| C-204 | fan.bounded の判定はプログラムと入力のみの関数 | 0.47.0 | active | fixture | 9 |
| C-205 | fan.min の勝者は受理された分岐中の (spend, index) 辞書順最小 | 0.47.0 | active | fixture | 8 |
| C-206 | settle ブロックは全分岐を分岐順でそれぞれの tuple slot に settle | 0.47.0 | active | fixture | 1 |
| C-207 | CM-1 は実時間について比率のみを主張する版付き定数 | 0.47.0 | active | fixture | 1 |
| C-208 | fan.timeout は omega 相対：fixed は全環境で終了し、固定 omega はバイト一致で再現 | 0.47.0 | active | fixture | 1 |
| C-209 | Codec encode は none を省略；decode は欠落/null を none にまとめ、Value はそのまま通す | 0.52.0 | active | fixture | 2 |
| C-210 | NaN 観測は正準 — deterministic-profile の適合則 | 0.53.0 | active | fixture | 3 |
| C-211 | ! は純粋な Result/Option 戻り fn で伝搬し target 間でバイト一致 | 0.53.5 | active | fixture | 1 |
| C-212 | 同名 private helper を持つ2つの self-host モジュールは正常リンクまたは wall 拒否 — 無効モジュールにはしない | 0.52.0 | active | fixture | 1 |
| C-213 | bytes のバイト単位書込みは範囲外書込みも含め両 target で同じバッファを符号化 | 0.53.5 | active | fixture | 6 |
| C-214 | process.exec_status_timeout：発火経路のエラーは固定、発火するかは host 次第 | 0.53.6 | active | fixture | 0 |
| C-215 | fs 内容 reader：不在は runtime が分類する _if_exists 族の ok(none) | 0.53.7 | active | fixture | 4 |
| C-216 | Option 宣言 effect 呼出しの明示 ! は暗黙 strip と同一 | 0.54.1 | active | fixture | 1 |
| C-217 | let _ = f() は Result を捨てる — err は伝搬しない | 0.55.0 | active | fixture | 1 |
| C-218 | fn 末尾として返すヒープペイロード ?? は両 target で同値 | 0.56.0 | active | fixture | 3 |
| C-219 | 分岐内の Never 型呼出しは両 target で実行し終了コードを設定 | 0.56.0 | active | fixture | 5 |
| C-220 | fs の逐行 stream walker は read_lines の行意味論で fold/each | 0.56.1 | active | fixture | 2 |
| C-221 | effect fn 型スロットは純粋/失敗可能 lambda を一つの carrier 意味論で受理 | 0.56.1 | active | fixture | 4 |
| C-222 | 式内に入れ子の scalar unwrap は両経路で同じく err を伝搬 | 0.56.1 | active | fixture | 1 |
| C-223 | Matrix 演算は platform ではなく同梱 musl-libm と native の積順序で計算 | 0.56.1 | active | fixture | 2 |
| C-224 | if let / guard let は両 target で同じくヒープペイロードを束縛/解放 | 0.56.1 | active | fixture | 1 |
| C-225 | fs.read_lines は両 target でファイルの各行を同じく具体化 | 0.56.2 | active | fixture | 2 |
| C-226 | 呼出し境界を越える mut パラメータは両 target で呼出し元データを変更 | 0.56.2 | active | fixture | 14 |
| C-227 | fs メタデータ/合成の関数族は両 target で同じ応答 | 0.56.2 | active | fixture | 1 |
| C-228 | fs 合成関数族と matrix 行選択は両 target で同じ応答 | 0.56.2 | active | fixture | 4 |
| C-229 | バイトバッファを超えた選択行は両 target で全ゼロ行 | 0.57.1 | active | fixture | 5 |
| C-230 | flight 参照 PID kernel は両 target で同じく実行 | 0.57.1 | active | fixture | 1 |
| C-231 | 整数/boolean リテラル形式は両 target で同じく評価 | 0.57.1 | active | fixture | 1 |
| C-232 | boolean リテラルと表示形式は両 target で同一 | 0.57.1 | active | fixture | 1 |
| C-233 | unit リテラル、その位置、反射的等価性は両 target で同一 | 0.57.1 | active | fixture | 1 |
| C-234 | 括弧グループ化と1-tuple 曖昧性解消は両 target で同じ挙動 | 0.57.1 | active | fixture | 2 |
| C-235 | 単項演算子：`not` 否定と接頭マイナスは両 target で同じく評価 | 0.57.1 | active | fixture | 1 |
| C-236 | tuple の構築/分解/添字/構造的等価性は両 target で同一 | 0.57.1 | active | fixture | 1 |
| C-237 | Option/Result コンストラクタリテラルは両 target で同じく構築/除去 | 0.57.1 | active | fixture | 1 |
| C-238 | 第一級 range 値は両 target で実際の境界を走査 | 0.57.1 | active | fixture | 10 |
| C-239 | コレクションリテラル/添字/空の綴りは両 target で同じ挙動 | 0.57.1 | active | fixture | 1 |
| C-240 | Map リテラルと空 map の読み取りは両 target で同じ挙動 | 0.57.1 | active | fixture | 2 |
| C-241 | 束縛文：let shadowing、var 再代入、検査時ガードは両 target で同一 | 0.57.1 | active | fixture | 1 |
| C-242 | 条件式は選択分岐のみ評価し、両 target で同一 | 0.57.1 | active | fixture | 1 |
| C-243 | ブロック式は両 target で同じ末尾値を返す | 0.57.1 | active | fixture | 1 |
| C-244 | while ループは条件検査付き本体を両 target で同じく反復 | 0.57.1 | active | fixture | 1 |
| C-245 | 文字列埋め込みは各部分の正準表示を両 target で同じく埋め込む | 0.57.1 | active | fixture | 2 |
| C-246 | 識別子は最寄りの束縛へ解決し、未解決名はその識別子を示す検査時 E003 | 0.57.1 | active | fixture | 2 |
| C-247 | match 式は両 target で最初に一致する分岐を同じく選び、非網羅は検査時 E010 | 0.57.1 | active | fixture | 1 |
| C-248 | for-in は list 族の headを任意の tuple 分解付きで両 target 同一に走査 | 0.57.1 | active | fixture | 1 |
| C-249 | tuple let 分解は両 target で各成分を位置順に同じく束縛 | 0.57.1 | active | fixture | 2 |
| C-250 | パイプと合成は順番通り通常の適用として評価され、両 target で同一 | 0.57.1 | active | fixture | 2 |
| C-251 | if let は裸の binder で Option 照合対象を暗黙 unwrap し、両 target で同一 | 0.57.1 | active | fixture | 1 |
| C-252 | 式文とコメント：Unit 呼出しは実行、破棄は明示、コメントは不可視 | 0.57.1 | active | fixture | 1 |
| C-253 | place 代入と型注記は両 target で同じ挙動 | 0.57.1 | active | fixture | 1 |
| C-254 | 型注記はどちらの target でも値を変えず期待型を与える | 0.57.1 | active | fixture | 1 |
| C-255 | record リテラル/メンバー読み取り/spread 更新は両 target で同じ値意味論を保つ | 0.57.1 | active | fixture | 2 |
| C-256 | break と continue は両 target と interp で同じくその文位置で効く | 0.57.1 | active | fixture | 2 |
| C-257 | scalar エラー演算子は両 target で同じく評価 | 0.57.1 | active | fixture | 1 |
| C-258 | 名前付き呼出しと lambda は両 target で同じく評価 | 0.57.1 | active | fixture | 6 |
| C-259 | variant コンストラクタ参照は両 target で match により同じく消去される値を構築 | 0.57.1 | active | fixture | 1 |
| C-260 | 宣言族は両 target で同じくコンパイル/実行 | 0.57.1 | active | fixture | 5 |
| C-261 | 負のゼロ符号を含む正準 float 値は両 target で同じ表示 | 0.57.1 | active | fixture | 1 |
| C-262 | fmt 安定な文字列エスケープと空文字列は両 target で同じく評価 | 0.57.1 | active | fixture | 1 |
| C-263 | 回復ノードは受理済みプログラムに現れず、壊れたファイルも最初のエラー以降を報告 | 0.57.1 | active | fixture | 1 |
| C-264 | scalar フィールドの optional chain は両 target で同じ some/none を返す | 0.57.1 | active | fixture | 1 |
| C-265 | guard 文の通過/raise 経路は両 target で同じ挙動 | 0.57.1 | active | fixture | 3 |
| C-266 | guard let は成功ペイロードを束縛し失敗側で raise、両 target で同一 | 0.57.1 | active | fixture | 1 |
| C-267 | 二項演算子 API は符号付き除算の裁定を含め両 target で同じく評価 | 0.57.1 | active | fixture | 1 |
| C-268 | 呼出し引数位置の `_` は両 target で同じ検査時 E046 | 0.57.1 | active | fixture | 0 |
| C-269 | let に束縛した入れ子 variant match は両 target で同じく実行；対象にないコンストラクタパターンは双方で検査時 E048 | 0.57.1 | active | fixture | 1 |
| C-270 | dialect epoch 14 で除去した stdlib の綴りは移動先を示す検査時エラー | 0.57.1 | active | fixture | 0 |
| C-271 | ハンドルを持つペイロードで `carrier ?? call!` は両 target で fallback の Result ブロックでなくペイロードを返す | 0.57.1 | active | fixture | 1 |
| C-272 | `fs.list_dir` はディレクトリの大きさによらず両 target で全項目をバイト順で返す | 0.57.1 | active | fixture | 1 |
| C-273 | fs 書込み側の基盤は native の std::io Display テキストで応答し fd_write の errno を落とさない | 0.57.1 | active | fixture | 1 |
| C-274 | 失敗可能 callback により fs streaming walk も失敗可能となり最初の err で止まる | 0.57.1 | active | fixture | 1 |
| C-275 | http 応答ヘッダ API は大文字小文字を区別しないフィールド名で両 target 同一に応答 | 0.57.1 | active | fixture | 1 |
| C-276 | ヒープ要素の String キー `list.sort_by` は両 target で同じ整列/共同所有 | 0.57.1 | active | fixture | 1 |
| C-277 | 単一化で固定した戻り型のみ総称のコンストラクタは、定数畳込みが固定文脈を消しても両 target で同じ構築/実行 | 0.57.1 | active | fixture | 1 |
| C-278 | rope 族は dialect epoch 14 で stdlib から除去 | 0.57.1 | active | fixture | 0 |
| C-279 | 反復0回のループはどちらの target でも本体の副作用/trap を行わない | 0.57.1 | active | fixture | 1 |
| C-280 | kernel-conformance 族は両 target で機械検証済み λ_almd 意味論に従う | 0.57.1 | active | fixture | 1 |
| C-281 | 裸の Result コンストラクタとパラメータ通過 effect 末尾は kernel 意味論に従い同じく実行 | 0.57.1 | active | fixture | 3 |
| C-282 | matrix.get は両 target で範囲外なら none；行 reduction は単位元を返す | 0.57.1 | active | fixture | 1 |
| C-283 | Fn 型 local 経由の呼出しはその local に解決し、同名トップレベル fn にはしない | 0.57.1 | active | fixture | 1 |
| C-284 | 利用者 effect fn callback で駆動する失敗可能 list HOF は両 target で同じく実行 | 0.57.1 | active | fixture | 8 |
| C-285 | regex.captures は添字0に全一致を返し、`none` は何も一致しない場合のみ | 0.57.1 | active | fixture | 2 |
| C-286 | `??` は元を読む；Option は消去後も生存し読み取り可能 | 0.57.1 | active | fixture | 1 |
| C-287 | none 分岐を持つ末尾位置ヒープ結果 if/match は record/list ペイロードについて選択分岐を実行 | 0.57.1 | active | fixture | 1 |
| C-288 | rich-variant list の list.enumerate は両 target で実行 | 0.57.1 | active | fixture | 1 |
| C-289 | Float 等価性族は両 target で f64 の等価性で比較 | 0.57.1 | active | fixture | 1 |
| C-290 | fs text reader は両 target で同じく不正 UTF-8 を拒否；bytes reader は生バイトを保つ | 0.57.2 | active | fixture | 1 |
| C-291 | 各エラーチャネル演算子は両 target でその定義関数と一致 | 0.57.1 | active | fixture | 2 |
| C-292 | reference-suite 適合性コーパスは target 間でバイト一致で実行 | 0.57.2 | active | fixture | 48 |
| C-293 | 2層コンテナ入れ子はバイト一致で実行；第3層と map 値形状は固定 wall 拒否 | 0.57.2 | active | fixture | 6 |
| C-294 | compute budget 0 は全経路でインライン化したループなし被呼出し関数を受理 | 0.57.2 | active | fixture | 2 |
| C-295 | guard-lowering された box pattern の checker 網羅 match は native でもビルドできる | 0.57.2 | active | fixture | 1 |
| C-296 | 生成した型×演算行列は target 間でバイト一致で実行 | 0.57.2 | active | fixture | 70 |
| C-297 | クロージャが別名付き list フィールドを返す flat_map は両 target で drop が釣り合う | 0.57.2 | active | fixture | 1 |
| C-298 | Codec 適合コーパスは target 間でバイト一致で往復 | 0.57.2 | active | fixture | 17 |
| C-299 | hash digest は target 間でバイト一致 | 0.58.0 | active | fixture | 1 |
| C-300 | Option-scalar 値の map リテラルは target 間で動作しリークなし | 0.59.0 | active | fixture | 2 |
| C-301 | 全 String map の map.upsert は target 間で動作 | 0.59.0 | active | fixture | 1 |
| C-302 | 数値決定性族の先頭：Float 結果はプログラムだけの関数 | 0.58.0 | active | fixture | 6 |
| C-303 | Float 演算は各回1度だけ最近接偶数丸め — 演算縮約なし、FMA なし | 0.58.0 | active | fixture | 2 |
| C-304 | 非正規化 float を保つ — どの target でもゼロへのフラッシュなし | 0.58.0 | active | fixture | 1 |
| C-305 | 超越関数の精度境界：sqrt は正しい丸め、exp/log/log2/log10/sin/cos/tan/pow は1 ulp以内 | 0.58.0 | active | fixture | 1 |
| C-306 | 符号付きゼロ：IEEE 伝搬、等価性は符号を無視、min/max は -0 を +0 より下に置く（IEEE 754-2019） | 0.60.0 | active | fixture | 2 |
| C-307 | Float の int.from_saturating は0方向切捨てと飽和、NaN は0；from_checked 族は正確か none | 0.58.0 | active | fixture | 2 |
| C-308 | @bounded は型/値を何も変えない関数属性 — bounded 関数は通常の関数 | 0.60.0 | active | fixture | 1 |
| C-309 | 方言でなく部分集合：プログラムの観測可能挙動は @bounded の有無で同一 | 0.60.0 | active | fixture | 2 |
| C-310 | @bounded 関数内ではコンパイル時定数の range 境界を持つ計数ループのみ受理（E070） | 0.60.0 | active | fixture | 0 |
| C-311 | @bounded 関数の計数ループ内は割当て禁止（E071） | 0.60.0 | active | fixture | 0 |
| C-312 | @bounded 関数のループ内では break/continue を拒否（E072） | 0.60.0 | active | fixture | 0 |
| C-313 | bounded profile では再帰を拒否 — 到達可能呼出しグラフは DAG（E073） | 0.60.0 | active | fixture | 0 |
| C-314 | @bounded 関数は @bounded 関数と一階の純粋 stdlib メンバーのみ呼べる（E074） | 0.60.0 | active | fixture | 0 |
| C-315 | @bounded 関数内で実行時長さのヒープ構築を拒否（E075） | 0.60.0 | active | fixture | 0 |
| C-316 | @bounded effect fn は標準出力のみ使用可；他 effect モジュール、host 到達モジュール、fan は拒否（E076） | 0.60.0 | active | fixture | 0 |
| C-317 | @bounded 関数では Float 演算を暫定拒否（E077） | 0.60.0 | active | fixture | 0 |
| C-318 | @bounded 関数の計数ループ本体内では早期終了を拒否（E078） | 0.60.0 | active | fixture | 0 |
| C-319 | 捕捉され変更される var List はクロージャを通じた共有記憶 | 0.60.0 | active | fixture | 12 |
| C-320 | budget cut は終了処理を行う：枯渇は必ず Err、region は独立、cut 位置は不可観測 | 0.60.0 | active | fixture | 2 |
| C-321 | fan prefetch の分割（全開始、分岐順 await、敗者放棄）は不可観測 | 0.60.0 | active | fixture | 1 |
| C-322 | コンストラクタは関数値であり組込み3種も含む | 0.60.0 | active | fixture | 1 |
| C-323 | or pattern：分岐の選択肢は binder なしの和集合として照合 | 0.60.0 | active | fixture | 7 |
| C-324 | 分岐からの mut-param 転送：値フローに畳込みまたは拒否、黙って誤らない | 0.60.0 | active | fixture | 2 |
| C-325 | bytes.push の増加は償却：保持量は線形、大規模でもバイト一致 | 0.60.0 | active | fixture | 1 |
| C-326 | 分岐選択の mut-fn 呼出しは書戻しを分岐外へ持ち上げ：gzip-inflate 形状は構造的に lowering | 0.60.0 | active | fixture | 1 |
| C-327 | env.sleep_ms は全経路で停止待ちし戻る | 0.60.1 | active | fixture | 3 |
| C-328 | http string 族は native/embedded-wasm 経路で同じ応答 | 0.61.1 | active | fixture | 0 |
| C-329 | env.set は全経路で env.get から同じく観測され、p1 shim は environ/args 操作を提供 | 0.62.0 | active | fixture | 1 |
| C-330 | http string 族は p3 component の async-lowered 交換を通じ stock runtime で応答 | 0.62.0 | active | fixture | 0 |
| C-331 | zlib は wasm 上で self-host：復号はバイト一致、符号化は往復が正確でバイト列は経路依存 | 0.62.0 | active | fixture | 1 |
| C-332 | list-rest pattern：prefix 後の tail を全経路で同じく list として束縛 | 0.62.0 | active | fixture | 2 |
| C-333 | as pattern：値全体をその分解と並べて全経路で同じく束縛 | 0.62.0 | active | fixture | 1 |
| C-334 | クロージャ呼出しは借用ヒープ引数に +1：RC-3 の被呼出し側所有規約は call_indirect でも保つ | 0.62.0 | active | fixture | 4 |
| C-335 | path.extension は両 target で先頭の dot を拡張子ではなく隠しファイル印とする | 0.62.0 | active | fixture | 2 |
| C-336 | 各テストは両 target で再初期化した可変モジュール global から始まる | 0.57.1 | active | fixture | 0 |
| C-337 | bytes の検索/編集/述語族は全経路で同じ応答 | 0.62.0 | active | fixture | 2 |
| C-338 | int bit 族、float32.from_bits、Int -> Float32 変換は全経路で一致 | 0.62.0 | active | fixture | 2 |
| C-339 | testing.assert_snapshot 不一致はテストブロック内も含め両 target で T18 形式 abort | 0.62.0 | active | fixture | 1 |
| C-340 | 即時評価 *_or 族 — option/result.unwrap_or — は両 target で選択前に既定値を評価 | 0.62.0 | active | fixture | 2 |
| C-341 | matrix.from_bytes_f16_le / _f32_le / _f64_le はデータが収まらなければ none；buffer 側で判定するため i64::MAX 近くの offset も none | 0.62.0 | active | fixture | 2 |
| C-342 | bytes.copy_from は負の長さ/offset では両 target とも何もコピーしない | 0.62.0 | active | fixture | 1 |
| C-343 | Result 宣言の通常被呼出し関数に対する末尾 `!` は持ち上げた effect fn でペイロードを返す | 0.63.0 | active | fixture | 1 |
| C-344 | Result 束縛後の呼出し引数ブロック内 guard-else-err は全経路で実行 | 0.63.0 | active | fixture | 1 |
| C-345 | 呼出し引数として渡す Result は総称パラメータを含む全引数位置で Result を保つ | 0.63.0 | active | fixture | 3 |
| C-346 | ヒープペイロード ?? は既存経路で呼出し引数および match 対象として lowering | 0.63.0 | active | fixture | 1 |
| C-347 | 既知リテラル上の演算子往復は両 target でそのリテラルを表示 | 0.63.0 | active | fixture | 1 |
| C-348 | バイト添字の文字列 slice は UTF-8 境界を検査 | 0.63.0 | active | fixture | 1 |
| C-349 | fan 分岐内の `??` は両 target で既定値を保つ | 0.63.0 | active | fixture | 1 |
| C-350 | process.exit は終了 status 0..=255 を通し他コードを拒否、WASI preview-1 ビルドは 126..=255 を wall 拒否 | 0.63.0 | active | fixture | 3 |
| C-351 | リテラル process 終了コードは終了 status 定義域に対して検査 | 0.63.0 | active | fixture | 0 |
| C-352 | or pattern の guard は原文順に各一致候補につき1回評価 | 0.63.0 | active | fixture | 3 |
| C-353 | matrix kernel の形状前提は両 target で定義済み abort | 0.63.0 | active | fixture | 1 |
| C-354 | matrix 行範囲は list.slice と同様に範囲内に収め、part count が0以下なら空結果 | 0.63.0 | active | fixture | 1 |
| C-355 | matrix 符号反転 `-m` は IEEE 符号反転であり、両 target でゼロの符号を反転 | 0.63.0 | active | fixture | 1 |
| C-356 | Value 内の非有限 Float は全経路で JSON null として文字列化 | 0.63.0 | active | fixture | 1 |
| C-357 | value.get_int は Int 専用：Float フィールドは none、切捨てない | 0.63.0 | active | fixture | 1 |
| C-358 | float.round、math.choose、int.rotate_*、math.log_gamma は全経路で全定義域にわたり正確 | 0.63.0 | active | fixture | 4 |
| C-359 | epoch 前の timestamp は所属日へ床方向に丸め、%Y は任意の年を表示 | 0.63.0 | active | fixture | 1 |
| C-360 | datetime.parse_iso は UTC offset を適用し、範囲外 field を名前付きで拒否 | 0.63.0 | active | fixture | 2 |
| C-361 | url.parse は host:port 分割後に host を検査し、対象外 authority を名前付きで拒否 | 0.63.0 | active | fixture | 1 |
| C-362 | `scoped` ブロックの観測対象はその本体のもの — region は不可観測 | 0.64.0 | active | fixture | 1 |
| C-363 | `scoped fn` は region 内外で同じ答えを計算 | 0.64.0 | active | fixture | 1 |
| C-364 | scoped fragment 外の形状は両 target で同じく検査時に拒否 | 0.64.0 | active | fixture | 0 |
| C-365 | 明示 conformance の総称 protocol は一つの宣言実装へ dispatch し、両 target で同一 | 0.64.0 | active | fixture | 1 |
| C-366 | http call handle：呼出しごとの制限は発動時に自身の名前を示し、cancel は接続を閉じ、poll はブロックしない | 0.64.0 | active | fixture | 0 |
| C-367 | http.serve は組み込み wasm 経路で native と同じ単一インスタンス・逐次の意味論で動作し、同じ status、header 集合、body を返す | 0.64.0 | active | fixture | 0 |
| C-368 | http router：handler は関数であり、最も具体的な経路が応答する。不正な表は拒否され、404/405/400 は router が生成し、両 target で同じになる | 0.64.0 | active | fixture | 0 |
| C-369 | lambda の失敗チャネルは、その ! オペランドが一致するエラー型を運ぶ。String チャネルは型付きエラーをその文字列補間テキストとして運び、両 target で同じになる | 0.65.0 | active | fixture | 1 |
| C-370 | 要求を分割することになる http header、または client が管理する header は、すべての経路で同じ err により拒否する | 0.66.0 | active | fixture | 0 |
| C-371 | Float32 の算術は、すべての経路で各結果を binary32 に丸める | 0.66.0 | active | fixture | 1 |
| C-372 | Float32 は、同じ binary32 へ往復できる最短の10進表記で表示される | 0.66.0 | active | fixture | 2 |
| C-373 | var は let と同じパターンで分解し、束縛されたすべての名前が可変となり、両 target で同じになる | 0.67.0 | active | fixture | 1 |
| C-374 | 組み込み wasm host の process は native と同じ答えを返す。子を開始する単体 artifact は非公開の almide:process/spawn import を持ち、その定義がない場所では load 時に拒否される | 0.67.0 | active | fixture | 0 |
| C-375 | stock wasm 向けに構築した serve 形プログラムは wasi:http/handler@0.3.0 export となり、wasmtime serve がフラグなしで実行し、native と同じ答えを返す | 0.67.0 | active | fixture | 0 |
| C-376 | Region は参照する Bytes を保持するため、その view のあらゆる使用よりも block の寿命が長い | 0.67.0 | active | fixture | 1 |
| C-377 | net tcp：loopback、handle の番号付け、EOF、close、is_open、tcp_local_port は、native、組み込み wasm 経路、stock WASI 0.3 component で一致する | 0.68.0 | active | fixture | 0 |
| C-378 | net tcp：すべての失敗は一つの表による errno を含まない分類済みテキストとなり、native、組み込み wasm 経路、stock WASI 0.3 component で同一になる | 0.68.0 | active | fixture | 0 |
| C-379 | net tcp：stock WASI 0.3 component では timeout と tcp_available を guest 側で強制し、native と組み込み wasm 経路と同じ答えを返す | 0.68.0 | active | fixture | 0 |
| C-380 | object の組から作る Map はキーごとに1項目を持つ。重複した object key は最初の位置を保ち最後の値を採り、すべての経路で同じになる | 0.68.0 | active | fixture | 1 |
| C-381 | test block 外で testing の assert が失敗すると、両 target で T18 形式の abort となる | 0.68.0 | active | fixture | 8 |
| C-382 | net unix socket と shm file：bytes、受け取った descriptor、readiness、shm 内容は native と組み込み wasm 経路で一致する。stock artifact はこれらを拒否する | 0.68.0 | active | fixture | 0 |
| C-383 | net unix socket と shm file：すべての失敗は native と組み込み wasm 経路で同じテキストの err となり、プログラムが保持しない descriptor は system call の前に拒否される | 0.68.0 | active | fixture | 0 |
| C-384 | testing.assert_aborts は capture のない closure だけを新しい子プロセスで実行し、その子が定義済み abort で終了してテキストに期待する部分文字列が含まれる場合に限り成功する。native と wasm で同じになる | 0.68.0 | active | fixture | 0 |
| C-385 | http client は要求 body の framing を自ら行う。呼び出し側の Content-Length header はすべての経路で破棄する | 0.68.0 | active | fixture | 0 |
| C-386 | record 形式の fan block は、その field を並列の兄弟として実行する record literal である | 0.68.0 | active | fixture | 2 |
| C-387 | 識別子は NFC で保持する UAX #31 の名前であり、互換文字は拒否する。大小文字区別のない名前は宣言によってのみ型側となり、非 ASCII 名はすべての経路でバイト単位に同じ動作をする | 0.68.0 | active | fixture | 1 |
| C-388 | 裸の呼び出しは自身の scope の定義を指し、entry program や別 module の同じ綴りの fn を指すことはない | 0.68.0 | active | fixture | 2 |
| C-389 | http.serve は最大64接続を同時に読み、10 s 以内に request head が揃わなければ408を返すため、遅い client が保持するのは自身の接続だけになる | 0.69.0 | active | fixture | 0 |
| C-390 | zlib decoder は出力を制限し、その上限を超えるとすべての経路で一つの同じ err テキストを返して停止する | 0.69.0 | active | fixture | 1 |
| C-391 | json.parse はネストを1000階層に制限し、すべての経路で同じ err テキストを返す | 0.69.0 | active | fixture | 1 |
| C-392 | fs.open_root の族はすべての path を root 内に保つ。..、絶対 path、symlink により root 外へ出る path は、すべての target で同じメッセージにより拒否する | 0.69.0 | active | fixture | 1 |
| C-393 | [permissions] fs は、symlink を解決したうえですべての fs 呼び出しを許可された directory に制限し、native と組み込み wasm host で同じ拒否を行う | 0.69.0 | active | fixture | 0 |
| C-394 | random.bytes は platform CSPRNG から n bytes を得る。n = 0 は空、負の n はすべての経路で同じテキストにより abort する | 0.69.0 | active | fixture | 2 |
| C-395 | engine の文法外または20,000 states を超える pattern には、regex.new は abort、regex.compile は err を返し、すべての経路で理由は同じになる | 0.69.0 | active | fuzz(300) | 3 |
| C-396 | fs.walk と fs.glob は symlink を entry として列挙するが決してその内部へ降りず、すべての経路で同じになる | 0.69.0 | active | fixture | 0 |
| C-397 | fs.create_temp_file と fs.create_temp_dir は entropy による名前で新しい entry を排他的に作成し、その owner だけが利用できるようにする | 0.69.0 | active | fixture | 0 |
| C-398 | path.join_within は、結合先 base の内側に留まる child だけを受け入れ、それ以外を拒否する | 0.69.0 | active | fixture | 1 |
| C-399 | io.write と io.write_bytes は、すべての target と host で指定 bytes を正確に出力する | 0.68.0 | active | fixture | 1 |
| C-400 | json.parse は、単独 surrogate、不明または短い escape、生の制御文字、noncharacter、未終端文字列を拒否し、入力を削除しない | 0.69.0 | active | fixture | 2 |
| C-401 | json.parse は RFC 8259 の値をちょうど一つ受け入れる。重複 key、後続 data、引用符のない key、末尾 comma、区切り欠落、JSON 外の空白、不正な数値は err となる | 0.69.0 | active | fixture | 3 |
| C-402 | url.parse は WHATWG と同様に入力を整え、先頭の scheme を検証し、backslash、制御文字、host の %、非正準の数値 host を拒否し、%2e を含む dot segment を除去する | 0.69.0 | active | fixture | 2 |
| C-403 | url.resolve は url.parse の規則で reference を整え、すべての path に %2e を含む一つの dot-segment 規則を適用する | 0.69.0 | active | fixture | 1 |
| C-404 | http.query_params は target の最初の ? より後ろに対する url.parse_query であり、すべての経路で一つの decoder を使い、全 pair の順序を保持し、不正な escape は err となる | 0.69.0 | active | fixture | 1 |
| C-405 | socket host は RFC 9112 が拒否する request head に400を返す。対象は単独 LF/CR、不正 request line、origin 形式以外の target、header colon 前の空白、colon のない行、obs-fold、Host の欠落または重複である | 0.69.0 | active | fixture | 0 |
| C-406 | router とその前の middleware は一つの正準 path を読み、非正準 path と、decode 後に traversal となる parameter は400となる | 0.69.0 | active | fixture | 0 |
| C-407 | http.serve のすべての response は、handler がその field を設定していない限り X-Content-Type-Options: nosniff を含む | 0.69.0 | active | fixture | 0 |
| C-408 | buffered http client は framing が完了する前に終了した response を拒否し、call-handle client と同様に response head の上限を設ける | 0.69.0 | active | fixture | 0 |
| C-409 | handler の err は client に届かない。固定500 body は request の番号を示し、stderr にはその番号の下で err を引用して記録する | 0.69.0 | active | fixture | 0 |
| C-410 | datetime.parse_iso は、0..=9999 外の年を wrap した時刻として返さず、年を明示して拒否する | 0.69.0 | active | fixture | 1 |
| C-411 | base64.decode と decode_url は、それぞれ自身の alphabet による正準の padding 付き形式だけを受け入れる | 0.69.0 | active | fixture | 1 |
| C-412 | 大文字小文字変換は非 ASCII scalar を ASCII に変換しない。string.to_upper、to_lower、capitalize はその scalar を変更しない | 0.69.0 | active | fixture | 2 |
| C-413 | int.parse と float.parse は、前後の ASCII 空白を除去した有限の10進数テキストだけを受け入れる | 0.69.0 | active | fixture | 6 |
| C-414 | int.from_checked と int.from_saturating が Float から Int への変換三つ組を完成させる | 0.69.0 | active | fixture | 1 |
| C-415 | random.int、float、choice、shuffle は native と組み込み wasm 経路で OS entropy を使う。同じ clock tick 内の取得も異なり、同時開始の thread は異なる stream を得る | 0.69.0 | active | fixture | 0 |
| C-416 | net の読み取りと受信 descriptor は有界である。読み取りは要求長を先に確保せず、Unix socket ごとに未取得 descriptor が待つ数は最大28である | 0.69.0 | active | fixture | 0 |
| C-417 | process.kill は kill(2) で kernel に要求し、1..=2147483647 外の pid を拒否し、固定 err テキストを返す。kill command と同様に [permissions] proc の制限を受ける | 0.69.0 | active | fixture | 0 |
| C-418 | runtime error のテキストは埋め込む user data の制御文字と bidi 文字を escape し、すべての経路で一つの規則と同じ bytes を用いる | 0.69.0 | active | fixture | 4 |
| C-419 | 2の累乗でない基数の bigint text は4300桁に制限し、固定テキストを返す。2の累乗の基数は無制限。bigint.pow は結果が確実に 30 * 2^28 bits を超える場合、計算前に拒否する | 0.69.0 | active | fixture | 1 |
| C-420 | list.range_step は end より厳密に手前のすべての Int start + i * step を列挙し、その個数は Int の端で wrap しない | 0.69.0 | active | fixture | 1 |
| C-421 | env.set は空の名前、名前内の `=` または NUL、値内の NUL を、すべての経路で同じ err により拒否し、何も保存しない | 0.69.0 | active | fixture | 1 |
| C-422 | hash.hmac_sha256/512 は最初の引数に Secret[Bytes] key を受け取り、すべての経路で RFC 4231 vector と一致する Secret[Bytes] tag を返す。is_valid_ は正しい tag だけを受け入れる | 0.69.0 | active | fixture | 1 |
| C-423 | bytes/string.is_equal_constant_time は data に依存する分岐なしに等値判定を返す | 0.69.0 | active | fixture | 1 |
| C-424 | md5 と sha1 は stdlib に存在しない。hash.md5、md5_hex、sha1、sha1_hex の呼び出しは検査時エラーとなり、hint は SHA-256 を示す | 0.69.0 | active | fixture | 0 |
| C-425 | 値は表示されるあらゆる場所で型が宣言した repr に従って出力され、Secret はすべての経路で Secret(***) と表示される | 0.69.0 | active | fixture | 1 |
| C-426 | Secret[String] と Secret[Bytes] の == は取り出した値を比較する。それ以外の Secret、およびそれを保持する variant や map は E103 となる | 0.69.0 | active | fixture | 1 |
| C-427 | Codec record は Secret field をその値の wire 形式から decode し、encode は導出しない（E023） | 0.69.0 | active | fixture | 1 |
| C-428 | record の `@skip` field は decode も encode もしない。`@skip_decode` field は decode せず encode は行う。どちらも default が必要である | 0.69.0 | active | fixture | 1 |
| C-429 | 導出された `T.decode_strict` は record の decode 対象 field にない最初の key を `unknown key '<k>'` で拒否する。`T.decode` は寛容なままである | 0.69.0 | active | fixture | 2 |
| C-430 | compiler が読み取らない record field の属性は検査エラー（E053）である。field の属性語彙は @builtin、@location、@skip、@skip_decode である | 0.69.0 | active | fixture | 0 |
| C-431 | fs の全ファイル読み取り関数は最大1 GiB の通常 file を読む。device、FIFO、socket は block せず Not a regular file の err、より大きい file は File too large の err となり、すべての経路で同じになる | 0.69.0 | active | fixture | 1 |
| C-432 | runtime が支える値（HttpRequest、HttpResponse、HttpCall）は、単独でも入れ子でも宣言された repr で補間され、すべての経路で同じになる。http の repr は header 数を表示し、header value は表示しない | 0.69.0 | active | fixture | 2 |
| C-433 | value.set_path は json.parse が許す深さより深い値の構築を拒否し、すべての経路で parse と同じ err テキストを返す | 0.69.0 | active | fixture | 1 |
| C-434 | http.serve handler 内の runtime abort は、その request だけを失敗させる。固定の C-409 の500、stderr 1行を出し、server は提供を続ける | 0.69.0 | active | fixture | 0 |
| C-435 | `Codec` は `Encode + Decode` である。`: Decode` 型は `decode`（record では `decode_strict` も）だけ、`: Encode` 型は `encode` だけを導出し、入力型はいかなる field も skip しない | 0.69.0 | active | fixture | 1 |
| C-436 | `http.decode_json[T](req)` は request body を入力型にだけ decode する。`Codec` の T は E110、T の外の key は `400 Bad Request: unknown key '<k>'` となる | 0.69.0 | active | fixture | 2 |
| C-437 | process.start の子のうち wait も stop もされていないものは、main の return、process.exit、effect error によりプログラムが終了すると stop される | 0.69.0 | active | fixture | 0 |
| C-438 | env.secret(name) は環境変数を Option[Secret[String]] として読む。env.get の値を持ち、Secret(***) と表示され、定数時間で比較される | 0.69.0 | active | fixture | 1 |
| C-439 | env.vars は、プログラム自身の set と remove を反映した環境を名前を key として名前順に列挙する。env.remove は名前を解除するか env.set と同様に拒否し、すべての経路で同じ bytes になる | 0.69.0 | active | fixture | 1 |
| C-440 | 表のすべての compile-cost budget が守られる。ソースから引き上げられない計数上限であり、定義された診断で終了する | 0.69.0 | active | fixture | 0 |
| C-441 | uuid.v7 は version 7 と RFC 9562 variant を持ち、8-4-4-4-12 の小文字16進で表し、同一 millisecond 内でも、すべての経路で各値が直前の値より厳密に大きい | 0.69.0 | active | fixture | 1 |
| C-442 | net udp と lookup_host：binding、datagram 境界、max-bytes 規則、送信者 address、loopback 名前検索は native と組み込み wasm 経路で一致する | 0.69.0 | active | fixture | 0 |
| C-443 | net udp と lookup_host：すべての失敗は net の一つの表による errno を含まない分類済みテキストであり、native と組み込み wasm 経路で同一になる | 0.69.0 | active | fixture | 0 |
| C-444 | 表示される値の中に入れ子になった Bytes 値は、その byte 値の list として出力され、すべての target で同一になる | 0.69.0 | active | fixture | 1 |
| C-445 | 組み込み wasm host は native と同様に、main の return、process.exit、effect error により実行が終了すると、wait も stop もされていない process.start の子を stop する | 0.69.0 | active | fixture | 0 |
| C-446 | Region は memory の address 幅の (base, len) として wasm host 境界を渡る。export された Region は host が address を解放するまで host の書き込み対象として残り、境界越しに貸し出した Region はその場で読み書きする | 0.69.0 | active | fixture | 0 |
| C-447 | fs.append_bytes はすべての経路で真の append を行う。file を読まないため、任意の大きさの file が追加 bytes を受け取り、その長さだけ正確に増える | 0.69.0 | active | fixture | 1 |
| C-448 | WASI 0.3 component の net udp と lookup_host は、wasi:sockets@0.3 上で C-442 と C-443 のプログラムに native とバイト単位に同一の答えを返す | 0.69.0 | active | fixture | 0 |

