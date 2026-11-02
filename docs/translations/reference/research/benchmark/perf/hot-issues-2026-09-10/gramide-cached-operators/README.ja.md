<a id="gramide-allocation-experiment-for-almide-2078"></a>

# Almide #2078 に向けた gramide のメモリ確保実験

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)

> 固定原文の非規範的な履歴参照資料。以下のテスト・ブランチ状態・測定・修正は日付付き報告で、新規実行した検証や普遍的な正しさの保証ではない。コードと履歴測定は原文保持し、コマンドやベンチマークは実行していない。自動同期せず、原本を正本とする。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/perf/hot-issues-2026-09-10/gramide-cached-operators/README.md)

2026-09-10、Apple M3/macOS。gramide `c6933d2ef93b9c5890540b1b57910da9d90d8b73` に、`git apply --unidiff-zero` で `gramide-cached-operators.patch` を適用する。クライアント側パッチは [gramide PR #1](https://github.com/O6lvl4/gramide/pull/1) として公開されている。
Almide の String や Token の表現は再設計しない。既存のコンパクトな Bytes API を使い、トークン化1回につき演算子のバイトパターンを一度だけ事前計算する。演算子に一致した場合だけテキストを複製する。ソース用ヘルパーの名前は `source_byte_at` とし、balance.almd の List[Int] 用ヘルパー `at` とのテストコンパイル時の衝突を避ける。

字句解析器のフォーマッタは、安全検査でコメント6個の消失を報告するため、このファイルを拒否する。強制整形したり安全検査を削除したりしないこと。

<a id="measurements"></a>

## 測定

フィクスチャは Go 1.26.1 の `src/cmd/compile/internal/ssa/opGen.go`、2,973,586バイト。SHA256 は両 JSON ファイルに記録されている。`check-results.json` はウォームアップ後、順序をシャッフルして交互に行った `/usr/bin/time -l` による7回の実行を記録する（seed 2078）。ベースライン・コンパクトな span・演算子キャッシュの全トークン出力ハッシュは一致する。メモリ確保プローブに表示されるチェックサムは、全方式で1142484379396。

| 方式 | check 実時間の中央値 | 最大 RSS の中央値 |
|---|---:|---:|
| 元の実装 | 0.51 s | 123.34 MiB |
| ソースと span のバッファをコンパクト化 | 0.51 s | 103.25 MiB |
| さらに演算子パターンをキャッシュ | 0.25 s | 103.16 MiB |

時間測定用バイナリには計装していない。測定はヘルパー名だけの衝突修正より前で、最終トークン出力の一致とプロジェクトテストは別に確認した。ローカルの予備測定であり、言語全体の高速化を主張するものではない。

<a id="allocation-attribution"></a>

## メモリ確保の原因分析

`bench_lex.almd` を gramide/src にコピーする。同じファイルを読み、Go を字句解析し、全トークンの位置と text/kind の長さを消費する。文法のコンパイルや構文解析は行わない。
Almide で出力し、`fn main()` を `fn probe_main()` に改名して `counting-allocator.rs` を末尾に追加し、rustc の `--edition 2021 -C opt-level=3
-C lto=yes -C codegen-units=1 -C overflow-checks=no` でコンパイルする。アロケータは alloc/realloc の要求数と要求サイズを数える。サイズは累計であり、生存メモリのピークではない。アトミックな計装は時間を変えるため、このバイナリで時間を測らない。カウンタにはプロセス初期化・ソース読み込み・チェックサム出力も含む。

元の実装：22,961,527要求 / 307,280,840要求バイト。
コンパクトな span：22,743,986 / 268,361,639。
演算子キャッシュ：2,612,801 / 117,801,931（要求数が約88.6%減）。

大幅な減少から、この入力では演算子候補の反復構築が主な費用だと分かる。所有されるトークンの kind/text 文字列は引き続きメモリ確保する。本実験は #2078 の表現上の問題すべてを解決したとは主張しない。

最終のヘルパー改名済みパッチ：プロジェクトの全9テストファイルが合格（WASM 2、ネイティブへのフォールバック7）。全トークン出力の SHA256 も引き続き一致。`final-validation.json` を参照。

<a id="reference-checkpoint"></a>

## 参照実装の確認地点

次の表現変更を選ぶ前にローカルの参照コンパイラを調べた。Rust `0d31508599a7814a7044e9a7a871e3dc5f037753` の `compiler/rustc_lexer/src/lib.rs`（`Token`、`TokenKind`、`advance_token`）、および Zig 0.15.2 `e4cbd752c8c05f131051f8c873cff7823177d7d3` の `lib/std/zig/tokenizer.zig`（`Token`、`Tag`、`Tokenizer.next`）。どちらも走査ループで候補文字列を再構築せず、トークンタグと文字・バイトの直接分岐で固定の記号を識別する。Gramide は言語仕様を値として受け取るため、バイトパターンの事前計算なら、固定言語用の走査器を導入せずに設定可能な順序を維持できる。tag/span によるトークン表現は別の、より広い API 変更であり、今回測定した削減には不要。

Hew の確認地点：hew-lang/hew を ../almide-references/hew に、`a78df3b9e3b9e0558e63caa76e14d2aba4f39e4f` で clone した。`hew-lexer/src/lib.rs` は Logos のコンパイル時 DFA によるトークン化と、文字列リテラルのペイロードに借用された `&'src str` を持つ `Token<'src>` を使う。トークンごとの所有テキストを除くための3番目の具体的参照例になるが、Hew 開発で gramide を使っていることを示すものではない。

Hew コーパスの互換性：そのリビジョンの全1,282 Rust ソースファイルで、元と最終パッチ適用後の gramide の `tokens` 出力・stderr・終了状態を比較した。1,282件すべて一致し、1ファイルは両版で拒否される。Rust ソースの互換性検査であり、Hew 言語の文法対応ではない。`hew-token-equivalence.json` を参照。
