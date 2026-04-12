<!-- description: CI check that stdlib/defs/*.toml declared types match runtime/rs/src/*.rs signatures -->
# Stdlib Defs / Runtime Consistency Check

## Motivation

`process.exec_status` と `fs.stat` で **declared return type ≠ runtime signature** の不整合が発見された（[#148](https://github.com/almide/almide/issues/148)）。

- `stdlib/defs/*.toml` 側：`Result[{code: Int, stdout: String, stderr: String}, String]`（匿名 record）
- `runtime/rs/src/*.rs` 側：`Result<(i64, String, String), String>`（tuple）

Almide の型検査器は toml 宣言だけを信じるので、ユーザコードは **record として通る**。コード生成もそのまま emit される。ところが Rust コンパイラがようやく runtime 関数のシグネチャを見た瞬間に落ちる。**テスト 0 件だったため、このバグは latent のまま merge 済みだった**。

**本項目は、この種の「宣言 vs 実装」乖離を build 時に自動検出するリントを整備する。**

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

## Related

- [#148](https://github.com/almide/almide/issues/148) —— 本項目を生んだ最初のバグ
- [Stdlib Symmetry Audit](./stdlib-symmetry-audit.md) —— Option/Result 間の非対称は「declared API 内部の一貫性」、本項目は「declared API と実装の一貫性」。補完関係
- [Almide Dojo](./almide-dojo.md) —— Dojo の初回 build がこのバグを掘り当てた。**Dojo 由来の最初のフィードバック事例**
