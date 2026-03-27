<!-- description: Seven compiler bugs discovered during test coverage expansion -->
# Compiler Bugs Found by Test Expansion [ACTIVE]

テストカバレッジ拡大（806→1501）で発見されたコンパイラバグ7件。テスト側で回避中だが、コンパイラを修正してテストをあるべき姿に戻す。

## Bugs

### 1. ~~`float.abs()` が Rust で free function `abs(x)` を生成~~

- **実際**: ランタイムに `almide_rt_float_abs()` が存在し正常動作。テスト記述ミス。
- **Status**: [x] NOT A BUG

### 2. top-level let + String → `const` 生成で `to_string()` 呼べない

- **期待**: `lazy_static` or `static` or `let` で初期化
- **実際**: `const NAME: String = "hello".to_string()` → `E0015: cannot call non-const method in constants`
- **場所**: `src/emit_rust/program.rs` TopLet codegen
- **検出**: lang/top_let_test.almd
- **修正**: String/非const式 → `static LazyLock<T>` に変更、変数参照時に `(*name).clone()`
- **Status**: [x] DONE

### 3. top-level let + float演算 → 型不一致

- **期待**: `const TRIPLE_PI: f64 = PI * 3.0`
- **実際**: `const TRIPLE_PI: i64 = (PI * 3.0f64)` → `E0308: mismatched types`
- **場所**: `src/emit_rust/program.rs` TopLet codegen + `ir_expr_contains_float` ヘルパー追加
- **検出**: lang/top_let_test.almd
- **修正**: IR式にfloatリテラルが含まれる場合は f64 に推論
- **Status**: [x] DONE

### 4. generic variant の型推論ヒント不足

- **期待**: `let e1: Either<String, i64> = Right(5)` のように型注釈生成
- **実際**: `let e1 = Right(5i64)` → `E0283: type annotations needed for Either<_, i64>`
- **場所**: `src/types.rs` Ty::Named に型引数追加、`src/emit_rust/ir_blocks.rs` ir_ty_annotation で Named の型引数付きを処理
- **修正**: Ty::Named(String) → Ty::Named(String, Vec<Ty>) に拡張。lower.rs/check/ で型引数を保存、codegen で `Either<String, i64>` のように型注釈を生成
- **検出**: lang/type_system_test.almd
- **Status**: [x] DONE

### 5. generic container の borrow 推論不足

- **期待**: `container_add(c, 1)` で `c` が clone されるか borrow される
- **実際**: `c` が move されて後続の `assert_eq!(c.label)` で `E0382: borrow of moved value`
- **場所**: `src/emit_rust/` の borrow analysis（generic record 型パラメータの推論）
- **検出**: lang/type_system_test.almd
- **修正**: borrow analysis が既に自動 clone を挿入するよう改善されていた。Bug #4/7 の修正による型情報改善でも解決
- **Status**: [x] DONE

### 6. `map.from_list` クロージャ内の borrow 推論不足

- **期待**: クロージャ引数 `w` が clone される
- **実際**: `|w| { (w, string.len(&*w)) }` → `w` が move 後に borrow → `E0382`
- **場所**: `src/emit_rust/` の borrow analysis（クロージャキャプチャ）、`stdlib/defs/map.toml`
- **修正**: (1) map.from_list の TOML テンプレートに `{f.clone_bindings}` 追加、(2) list.toml の全クロージャ関数に同様の修正、(3) Tuple 式で同一変数が複数回使われる場合に最初の move 位置で .clone() 生成
- **検出**: lang/edge_cases_test.almd
- **Status**: [x] DONE

### 7. named record 型に構造体リテラルが代入できない

- **期待**: `type Container = { items: List[T], label: String }` で `{ items: [], label: "x" }` が代入可能
- **実際**: `cannot assign { items: List[Int], label: String } to Container`
- **場所**: `src/check/statements.rs` の型チェック、`src/types.rs` の resolve_named
- **修正**: resolve_named を拡張して型引数を持つ Named 型の解決をサポート。let/var の型チェックで Named → 構造体に解決してから互換性チェック
- **検出**: lang/type_system_test.almd
- **Status**: [x] DONE

## Priority

**P0（codegen correctness）**: #1, #2, #3 — 正しい Almide コードがコンパイル通らない
**P1（generics usability）**: #4, #7 — generic 型の基本的な使い方が壊れている
**P2（borrow refinement）**: #5, #6 — borrow 推論の精度向上

## Fix → Test Restore Flow

各バグの修正後:
1. コンパイラを修正
2. テストを「回避版」から「あるべき姿」に戻す
3. `cargo test` + `almide test` で全パス確認
4. このファイルの Status を `[x] DONE` に更新

## 修正ログ

| # | Bug | Fixed | Test Restored | Date |
|---|-----|-------|---------------|------|
| 1 | float.abs codegen | N/A (not a bug) | N/A | 2026-03-12 |
| 2 | top-level let String | done | done | 2026-03-12 |
| 3 | top-level let float型 | done | done | 2026-03-12 |
| 4 | generic variant 型注釈 | done | done | 2026-03-12 |
| 5 | generic container borrow | done | done | 2026-03-12 |
| 6 | map.from_list borrow | done | done | 2026-03-12 |
| 7 | named record 互換性 | done | done | 2026-03-12 |
