# runtime/rs — native ランタイムのソーステンプレート

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)



> 原文: [runtime/rs/README.md @ 35297a](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/runtime/rs/README.md)。非規範的な読者向けスナップショット翻訳です。自動同期されません。計測値・主張の独立した再検証は行っていません。原文を正本とします。
>
> 原文上の注意: エラー数やモジュール数は記載日時の記録です。最後の自己完結性に関する例外説明も原文時点の説明であり、現在の依存関係を独立に再検証したものではありません。

`runtime/rs/src/*.rs` は、`stdlib/*.almd` の `@intrinsic("almide_rt_*")` 宣言を支える native (`almide_rt_*`) 実装を格納します。**ビルドするクレートでも、workspace メンバーでもありません。** 本文書はその理由と、実際に動作を検証する場所を記録します。

## このディレクトリの利用方法

`runtime/rs` を cargo 依存としてリンクするものはありません。`crates/almide-codegen/build.rs` が各ファイルを**ソーステキスト**として読み（`include!` 指令を解決）、`crates/almide-codegen/src/generated/rust_runtime.rs` に `RUST_RUNTIME_MODULES` として埋め込みます。出力時、コンパイラはプログラムに必要なモジュールを**一つのフラットなモジュール**に連結し、合成 prelude を前置して rustc に渡します。

- `emit_runtime_crate()` → 共有 `almide_rt` rlib（`--crate-name almide_rt`、std のみを使う全モジュール。`NON_STD_RUNTIME_MODULES` — `http`、`zlib`、`sse` — は、単体の rustc に rustls/flate2 の `--extern` がないため除外）。
- `emit_source()` → 生成 Rust プログラムのランタイム前置部。

どちらの経路も `strip_test_blocks` を呼び、rustc が見る前に全 `#[cfg(test)]` ブロックを削除します。

以前の `strip_test_blocks` は文字列リテラルを認識せず、行ごとに波括弧を数えていました。そのため、リテラル内の波括弧が不均衡なテスト本体では、処理が行き過ぎてファイル末尾まで削除していました。`regex.rs` には二つ（`r"a\{x"`、`r"a{2"`、深さ +4）があり、削除処理はテストモジュールから最終行まで進み、末尾の `// ---- End Regex Runtime ----` コメントも消していました。無害だったのは、そのモジュールがファイルの最後にあったからにすぎません。同様のブロックの後に実コードがあれば、全出力クレートから黙って削除されたはずです。削除処理にはバイト同一の二つの複製がありました。こちらと、`crates/almide-kernel/src/*.rs` を処理するビルドスクリプト側です。どちらの問題も検査されていませんでした。

両方を修正し、両方を検査しています（#2511）。現在、削除処理は一つだけで、`crates/almide-codegen/src/strip_test_blocks.rs` を `#[path]` でビルドスクリプトと共有します。文字を数える代わりにリテラルの状態を追跡します（通常文字列、任意の hash 深度の raw 文字列、ライフタイムと区別した文字リテラル、行コメントと入れ子のブロックコメント。すべて行をまたいで保持）。ブロック終端を見失えば、EOF まで黙って削除せず、ファイルと開始行を示す ERROR にします。「削除できなかった」は安全な失敗ですが、「その後をすべて削除した」はそうではありません。

七つのテストモジュールの削除（#2507）による出力テキストの変化は、空行かコメントだけのちょうど八行でした。変更前後のソースに `strip_test_blocks` を実行して確認しました。六ファイルで空行が一つ減り、`regex.rs` には過剰削除されていたコメントが戻りました。コード行は動いていません。

`runtime/rs/src` の編集は埋め込まれるテキストを変えるため、CI は `rust_runtime.rs` / `runtime_fn_modes.rs` を再生成し、差分があれば失敗します（ci.yml の "Generated runtime registry matches committed sources"）。`almide-codegen` を再ビルドし、編集と一緒に生成ファイルもコミットしてください。

## workspace メンバーではない理由（#2507、2026-09-22 計測）

ルートの `Cargo.toml` の `members` に `"runtime/rs"` を加え、`cargo check -p almide_rt` を実行すると、`src/lib.rs` が宣言する 25 モジュール中 15 モジュールで **220 エラー**（124 × E0425、83 × E0433、13 × E0405）が出ます。二種類に分かれ、どちらも依存関係やターゲット設定の問題ではありません。rustls と webpki-roots は正常に解決します。

1. **prelude がソースとして存在しません。** `AlmideRepr`、`AlmideMap`、`AlmideKey`、`AlmideKeyLookup`、`AlmideMapKey`、`ALMIDE_MAP_INDEX_THRESHOLD`、`almide_stdout_flush`、`almide_stdout_write_bytes`、`almide_rt_intern_key`、`almide_http_request_stream_impl` は `rust_runtime_prelude()` が出力します。これは `crates/almide-codegen/src/lib.rs` 内に文字列リテラルとして書かれた Rust で、可視性が異なる二種類があります。`runtime/rs` 配下に定義ファイルはありません。

   ```
   error[E0425]: cannot find function `almide_stdout_flush` in this scope
     --> runtime/rs/src/process.rs:44:5
   error[E0405]: cannot find trait `AlmideRepr` in this scope
     --> runtime/rs/src/value.rs:422:6
   ```

2. **`src/lib.rs` のモジュールツリーは実態と異なります。** ファイルは互いをパスなしで呼び出し、フラットに連結した場合にしか解決しません。`json.rs` → `almide_rt_value_stringify`、`math.rs`/`matrix.rs` → `almide_rt_libm_*`、`fan.rs` → `almide_rt_list_par_*`、`sse.rs` → `almide_rt_json_parse`。`pub mod json; pub mod value; …` の下ではすべて E0425 です。`lib.rs` は `hash.rs`、`net.rs`、`zlib.rs` も完全に省いています。埋め込み側は出荷するのに、モジュールツリーでは一度も言及されていない三ファイルです。

   ```
   error[E0425]: cannot find function `almide_rt_value_stringify` in this scope
     --> runtime/rs/src/json.rs:6:62
   error[E0425]: cannot find type `AlmideValue` in this scope
     --> runtime/rs/src/json.rs:6:37
   ```

メンバー化するには、ランタイムソースを再構成するか（埋め込み側はフラットな連結に依存）、別の場所で prelude を再導出する必要があります。また、何も出荷しないクレートのために、`almide-kernel`、`almide_rt`、二つ目の `webpki-roots` メジャーバージョン（`almide-rt-core` が固定する 1.0.9 に加えて 0.26.11）が workspace の lock に加わります。

`Cargo.toml` はランタイムソースが必要とするものの記録として残しています（crates.io 依存。#4007 で、どのビルドも設定しない `have_blas` BLAS 経路とともに `build.rs` を削除）。ビルド単位ではなく、ここで cargo を呼ぶ CI job もスクリプトもありません。

## 動作の oracle は `spec/`

**`runtime/rs` の動作の oracle は、実行可能な `spec/` スイートです。** 実プログラムと同じく stdlib を通して native と構造的 Wasm の両経路でコードを動かし、CI もこれを実行します。ここに書いた Rust の `#[cfg(test)]` ブロックはどこでも実行されません。cargo ビルドがこのディレクトリをコンパイルせず、`strip_test_blocks` が全出力クレートからブロックを削除するためです。

したがって、**`runtime/rs` 配下に `#[cfg(test)]` を追加しないでください。** テストは `spec/stdlib/`（クロスターゲットの約束なら `spec/wasm_cross/`）に書きます。`scripts/check-runtime-test-placement.sh` が CI で強制します。

### 唯一の限定的な例外: ルートテストが取り込む自己完結モジュール

テスト対象が **private** で、唯一の公開入口が非決定的なら、`spec/` では到達できません。`random.rs` の 53-bit 構築（`almide_float_from_u64`）がその例です。`random.float()` は誰も選べない u64 を抽出するため、`spec/stdlib/random_test.almd` は標本の下限と構築の上限値を確認できても、旧商が 1.0 に丸められた個別の u64 入力は指定できません（#2495）。

そのため、`tests/runtime_random_float_construction_test.rs` が `runtime/rs/src/random.rs` を独自モジュールへ `include!` し、`cargo test --workspace` で**出荷されるソーステキスト**をテストします。private 要素へのアクセサはランタイムソースではなくテストファイルに置くため、出荷物は変わりません。

これが成立するのは、`random.rs` が `std::cell::Cell` 以外に依存しないためだけです。他の各モジュールは、埋め込み側のフラットな連結と合成 prelude の下でしか解決しません（前述の 220 エラー）。同じ include はコンパイルできません。一般的なパターンではなく、一つの計測済みの例外として扱ってください。二つ目のモジュールが条件を満たした場合は、自己完結する理由をテストファイルに記載してください。
