<!-- description: CI check that stdlib/defs/*.toml declared types match runtime/rs/src/*.rs signatures -->
<!-- done: 2026-04-20 -->
# Stdlib Defs / Runtime Consistency Check

## Completion status (2026-04-20) — obsoleted by Stdlib Declarative Unification

本 arc を生んだ原因 (`stdlib/defs/*.toml` と `runtime/rs/src/*.rs` の二重宣言) は Stdlib Declarative Unification arc の完遂 (2026-04-19, 956dc96c / 49d341f6) により消失した:

- `stdlib/defs/` ディレクトリ自体が削除済み (`ls stdlib/defs/ → No such file or directory`)
- 旧 TOML の型宣言層は全て `stdlib/*.almd` 上の `@intrinsic("symbol")` に集約
- `pass_borrow_inference::intrinsic_borrow_mode` が Almide 型から runtime ABI decoration を一意に導出するため、宣言側と実装側の型乖離が発生するパスが構造的に存在しない
- runtime fn signature が `@intrinsic` 宣言と不一致なら `cargo build` が即落ちる (latent bug 化しない)

元の #148 (`process.exec_status` / `fs.stat` の匿名 record ↔ tuple 不整合) は pragmatic fix で既解決、かつ同種 bug の再発経路が閉じた。

**Residual concerns (別 arc で扱う):**

- **Test coverage gate** (`stdlib coverage: 76.8%` 時点 audit): 全 runtime fn に spec test を必須化する CI rule は別 arc (「stdlib coverage audit」) で扱う。本 arc は解消理由 (TOML/runtime 二重宣言) が違うので分離する。
- **Named stdlib types** (`ProcessStatus` 等の named record 返し): API ergonomics の別 arc。TOML 時代の制約ではないので本 arc の目標とは独立。

## Motivation

`process.exec_status` と `fs.stat` で **declared return type ≠ runtime signature** の不整合が発見された（[#148](https://github.com/almide/almide/issues/148)）。

- `stdlib/defs/*.toml` 側：`Result[{code: Int, stdout: String, stderr: String}, String]`（匿名 record）
- `runtime/rs/src/*.rs` 側：`Result<(i64, String, String), String>`（tuple）

Almide の型検査器は toml 宣言だけを信じるので、ユーザコードは **record として通る**。コード生成もそのまま emit される。ところが Rust コンパイラがようやく runtime 関数のシグネチャを見た瞬間に落ちる。**テスト 0 件だったため、このバグは latent のまま merge 済みだった**。

**本項目は、この種の「宣言 vs 実装」乖離を build 時に自動検出するリントを整備する。**

## Current status (2026-04-14)

**#148 自体は pragmatic fix で既にクローズ済み**：両関数の declared return を tuple 形式に揃えた。ただし「**named record を返したい**」という本来の API design 要求は未解決で、これは systemic な TOML 型システム拡張 (named stdlib types) が必要になる。

**stdlib coverage audit (same date)**:

- 441/574 = **76.8%** of stdlib functions have >=1 spec test
- ホールサイズ:
  - `io`: **0/7 (0%)** — 全滅
  - `fs`: **1/26 (4%)** — `read_text` のみ
  - `matrix`: **14/46 (30%)** — v0.14.0 で追加した f32 / `mul_scaled` 系が未テスト
  - `http`: 8/20 (40%) — server 側が未テスト
  - `process`: 5/12 (42%) — `exec_status` 再発防止テスト無し
  - `env`: 5/9 (56%)
  - `bytes`: 87/126 (69%)

これらの低カバレッジ領域こそ次の #148 候補。Phase 5 の enforcement でここを強制する。

## The General Problem

`stdlib/defs/*.toml` と `runtime/rs/src/*.rs` は人間が手で揃える前提になっている。整合性の検査は：

- **型検査器**は toml 側だけを見る
- **runtime crate**は独立してコンパイルされる
- **codegen**は toml の `rust = "..."` テンプレートを展開するが、expand 後の Rust が実際に compile 通るかは、最終 user program が生成されるまで分からない

この分離のおかげで：

- runtime のシグネチャ変更 → toml の更新忘れ → latent bug
- toml の型記述変更 → runtime の更新忘れ → latent bug
- どちらも **誰かが実際にその関数を呼ぶまで表面化しない**

## Design

### Consistency check: build-time script

`almide-codegen/buildscript/consistency_check.rs` を新設し、`build.rs` から呼ぶ。以下を検査する：

1. **Function existence**：toml の `rust = "..."` テンプレートが参照する `almide_rt_*` 関数が、runtime crate に実際に存在するか
2. **Parameter arity**：toml の `params` 数が runtime 関数の引数数と一致するか
3. **Parameter type compatibility**：各 param の Almide 型が、対応する Rust 型と互換か
   - `String` ↔ `&str` / `String`
   - `Int` ↔ `i64`
   - `Float` ↔ `f64`
   - `Bool` ↔ `bool`
   - `List[T]` ↔ `Vec<T>`
   - `Option[T]` ↔ `Option<T>`
   - `Result[T, E]` ↔ `Result<T, E>`
4. **Return type compatibility**：toml の return が runtime 関数の戻り型と互換か
   - **匿名 record の禁止**：runtime 関数が匿名 record 型を返すことはできない（AlmdRecN は program 依存のため）。declare されていたら hard error
   - named struct なら名前が一致しているか
   - tuple なら arity と各要素の型が一致しているか

### Failure mode

検査失敗は **`cargo build` を止める**。エラーメッセージは：

```
stdlib defs/runtime mismatch:
  stdlib/defs/process.toml[exec_status]:
    declared: Result[{code: Int, stdout: String, stderr: String}, String]
  runtime/rs/src/process.rs:almide_rt_process_exec_status:
    actual:   Result<(i64, String, String), String>

  Anonymous records cannot be returned from runtime functions.
  Either:
    (a) change the toml return to a tuple: Result[(Int, String, String), String]
    (b) change the toml return to a named type and add a wrapper in stdlib/*.almd
```

### Test coverage gate

加えて、**全 runtime 関数に最低 1 つの spec test が必須**という rule を追加：

- `stdlib/defs/*.toml` の各 `[func]` エントリに対し、`spec/stdlib/<module>_*test.almd` 内にその関数を呼ぶ test block が最低 1 つ存在すること
- CI で grep ベースで検査。存在しなければ build 落ち
- `ignore_coverage = true` オプションで明示的に免除可能（低レベル primitive など）

これにより、**次に同じ latent bug は merge 前に必ず落ちる**。

## Implementation Phases

1. **Phase 1**: `buildscript/consistency_check.rs` を書き、existing mismatches を全て検出（`process.exec_status`, `fs.stat` 以外にもあるか確認）
2. **Phase 2**: 検出された全 mismatch を修正（toml 側を tuple に揃えるか、runtime を変えるか、case-by-case 判断）
3. **Phase 3**: `cargo build` で check を enforce
4. **Phase 4**: test coverage gate を追加
5. **Phase 5**: coverage の空白を埋める（全 runtime 関数に spec test を用意）

## Acceptance Criteria

- `cargo build` 時に toml ↔ runtime の型不整合を検出する
- 既知の 2 件（`process.exec_status`, `fs.stat`）が解消されている
- 新しい runtime 関数を追加する PR が、toml 宣言を忘れると落ちる
- 新しい runtime 関数を追加する PR が、spec test を忘れると落ちる
- Phase 1 で発見された追加の mismatch（存在すれば）がすべて修正されている

## Non-goals

- Almide-level stdlib モジュール（`stdlib/*.almd`）と runtime の整合性 —— これは Almide 型検査器が既にカバー
- WASM runtime との整合性 —— Rust runtime が主戦場なので、Phase 1 は Rust のみ

## Ideal form: named stdlib types

現 TOML schema は関数の型宣言しか書けない。#148 の**本質的な解決** (ergonomic named record API、`r.code`/`r.stdout` アクセス) を可能にするには TOML schema を拡張して stdlib モジュールが名前付き型を宣言できるようにする。

```toml
# stdlib/defs/process.toml
[[types]]
name = "ProcessStatus"
kind = "record"
fields = [
  { name = "code",   type = "Int" },
  { name = "stdout", type = "String" },
  { name = "stderr", type = "String" },
]

[exec_status]
return = "Result[process.ProcessStatus, String]"
rust = "almide_rt_process_exec_status(...)"
```

対応する Rust runtime は stable struct:

```rust
// runtime/rs/src/process.rs
#[derive(Clone, Debug)]
pub struct ProcessStatus {
    pub code: i64,
    pub stdout: String,
    pub stderr: String,
}
pub fn almide_rt_process_exec_status(cmd: String, args: Vec<String>)
    -> Result<ProcessStatus, String> { ... }
```

フロントエンドは import 時にこの named type を型システムに登録し、Almide 側で `r.code` と書いてそのまま通る。codegen は named type を知っているので Rust struct とそのフィールド名で emit する。

**工事規模** (累積):

| # | 作業 | 工数 |
|---|---|---|
| 1 | TOML schema 拡張 (`[[types]]`) | 0.5d |
| 2 | 型システム登録 (import 時の named type 登録) | 1d |
| 3 | Rust codegen (struct 定義 emit + field access) | 1d |
| 4 | Rust runtime (struct 定義 + #148 2関数を named 化) | 0.5d |
| 5 | WASM codegen (struct を tagged record / tuple に lowering) | 1d |
| 6 | Build-time consistency check (Phase 1-3) | 1d |
| 7 | Test coverage gate (Phase 4-5) | 0.5d |
| **合計** | | **~5.5d** |

推奨分割:

- **PR 1**: 工事 1+2 (TOML schema + 型登録、既存への影響なし)
- **PR 2**: 工事 3+4 (codegen + runtime + #148 再度 named 化)
- **PR 3**: 工事 5 (WASM 対応)
- **PR 4**: 工事 6+7 (CI 検証)

## Related

- [#148](https://github.com/almide/almide/issues/148) —— 本項目を生んだ最初のバグ
- [Stdlib Symmetry Audit](./stdlib-symmetry-audit.md) —— Option/Result 間の非対称は「declared API 内部の一貫性」、本項目は「declared API と実装の一貫性」。補完関係
- [Almide Dojo](./almide-dojo.md) —— Dojo の初回 build がこのバグを掘り当てた。**Dojo 由来の最初のフィードバック事例**
