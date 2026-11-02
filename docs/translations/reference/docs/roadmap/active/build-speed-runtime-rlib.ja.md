<a id="build-speed-runtime-rlib--hot-fn-inlining"></a>

# ビルド速度：ランタイム rlib とホット関数のインライン化

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](build-speed-runtime-rlib.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](build-speed-runtime-rlib.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](build-speed-runtime-rlib.zh-CN.md)

> 固定原文の非規範的な読者用スナップショットであり、自動同期しません。日付付きの現実装の節が、下の以前の no-LTO／インライン化計画に優先します。過去の所要時間と性能の主張を、この翻訳のために独立測定していません。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/build-speed-runtime-rlib.md)

<a id="current-implementation-2026-10-31"></a>

## 現在の実装（2026-10-31）

リリースの高速経路は現在、**fat LTO と一つのコード生成ユニット**でリンクする。ランタイム rlib 自体は LTO 無効・四つのコード生成ユニットでビルドする。`src/cli/cargo_build.rs` には 10 月 9 日の実験を記録している。ラッパーへ `#[inline]` を付けるだけでは、観測された非 LTO の性能差は回復しなかった。キャッシュキーはソース、ツールチェーンの識別情報、最適化レベル、CPU、コード生成フラグを含み、固定の公開一時パスではなく `build_dir::cache_base()` の下に置く。

クロスビルド、matrix の使用、runtime-crate 依存、native 依存、source root がある場合は run/build の高速経路を通らない。実際の判定条件は [cargo_build.rs](../../../../../../src/cli/cargo_build.rs) を参照。以下の時間と no-LTO/inline 計画は以前の実装を記述するもので、測定・設計の履歴として残しており、現在の処方ではない。rlib 高速経路での native 依存対応とビルド済み配布は、別々の提案のままである。

<a id="earlier-implementation-and-inlining-proposal"></a>

## 以前の実装とインライン化の提案

> 目標：Go のようなビルド速度。開発・テスト・実行のループも出荷用ビルドも、毎回ランタイムを再コンパイルするコストを払うべきではない。

<a id="status"></a>

## 状態

**出荷済み（ブランチ `build-speed-parallel-rustc`）**：`almide_rt` ランタイムを一度だけ `.rlib` にコンパイルし（`codegen::emit_runtime_crate()`）、runtime-source + rustc-version + opt-level をキーとして `$TMPDIR/almide-rtlib-<hash>` にキャッシュする。ファイル・ビルドごとの rustc は小さな main（`#[macro_use] extern crate almide_rt;
use almide_rt::*;` とユーザーコード、`RT_BOUNDARY_MARKER` で分割）を出力し、`--extern almide_rt=<rlib>` で rlib をリンクする。何か失敗すれば inline/cargo 経路に戻る。`ALMIDE_NO_RTLIB=1` で無効にする。

接続した経路：
- **テスト経路**（`cargo_build_test_with_native`）：rlib は opt1、小さな main は **opt0**（テストではユーザーコードの最適化が不要）→ **ファイルごと 2.45 倍**、spec/lang 116 ファイルで **壁時計 2.26 倍／CPU 3.1 倍**。
- **run/build 経路**（`cargo_build_generated_with_native`）：rlib と小さな main をバイナリの最適化レベル（debug は 1、release は 3）で処理する。出荷用 `--release` ビルドは **約20倍**（27～33秒→1～2秒）、出力の一致を確認済み。

除外：`http`/`zlib`/`sse`（非 std 依存）は cargo 経路に残る。

<a id="the-remaining-gap-this-roadmap-item-shipping-build-inlining"></a>

## 残る差（このロードマップ項目）：出荷用ビルドのインライン化

ランタイムは **LTO なし**でコンパイルする別 crate なので、非ジェネリックかつ非 `#[inline]` のランタイム関数は**crate 境界を越えてインライン化されない**。文字列・リスト中心のホットループ（測定対象：`csv-to-json` による20万行 CSV）では、出荷バイナリは単一構成のビルドより**約10%遅い**（一般的なプログラムでは2～5%）。ジェネリックなランタイム関数（`list`/`map`/`option`/`result` 操作の大半）と既存の `#[inline(always)]` ヘルパーは、既に crate 間でインライン化されるので影響しない。

却下した代案：**ThinLTO**。インライン化は回復するが、リンク時にランタイムのビットコードを再最適化し、ビルド速度の利点を消してしまう（ほぼ単一構成ビルドと同じ時間）。実測でコンパイルの利点がなかったため、採用に値しないとした。

<a id="plan-inline-the-hot-non-generic-runtime-fns"></a>

### 計画：ホットな非ジェネリックのランタイム関数に `#[inline]` を付ける

少数のホットな非ジェネリック関数、主に `string.*`（`trim`、`split`、`lines`、`concat`、`len`、スライス）や頻繁に呼ぶスカラーヘルパーへ、`runtime/rs/src/*.rs` で `#[inline]` を付ける。rlib 内で MIR が公開されると rustc は LTO なしでも crate 境界を越えてインライン化し、約20倍のビルド速度と「別ランタイム crate・LTO なし」モデルを維持しながら性能差を回復する。Go の「小さな関数は自動インライン化する」という方針に似ている。

手順：
1. ランタイム中心の負荷（例：20万行 CSV の `csv-to-json` ベンチマーク）をプロファイルし、rlib と単一構成の差を支配する関数を特定する。
2. それらに `#[inline]` を付ける（呼び出し側のコンパイル時間肥大化を避けるため、一律の `#[inline(always)]` より対象を絞った `#[inline]` を優先）。
3. 再測定し、出荷時の実行性能低下が約0%になり、ビルドが約20倍を保つことを確認する。
4. 過剰インライン化を防ぐため、小さな main のコンパイル時間が再び増えないか監視する（候補が増えるほど、最後の rustc の仕事も増える）。

<a id="open-follow-ups"></a>

### 未解決の後続事項
- **native 依存**を持つプロジェクトの `almide build` に rlib を接続する（現在 cargo 専用）。cargo へパス依存または `RUSTFLAGS --extern` で rlib を渡す必要がある。
- 新環境の初回ビルドで一度きりの rlib コンパイルを省けるよう、コンパイラのリリースにビルド済み rlib を同梱することを検討する。
