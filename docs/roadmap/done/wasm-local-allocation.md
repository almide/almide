<!-- description: Redesign WASM function local allocation and scratch layout -->
# WASM Local Allocation Redesign

## Status: 設計段階

## 現状の問題

### アーキテクチャ

```
WASM function local layout:
[params...][bind locals...][i64 scratch × N][i32 scratch × N]
                            ^match_i64_base  ^match_i32_base

N = count_scratch_depth(body) — IRを静的走査して最大scratch使用数を推定
```

stdlib関数の実装は `match_i32_base + match_depth` から始まる連続したscratch localを使う:

```rust
let s = self.match_i32_base + self.match_depth;
// s+0: list_ptr, s+1: idx, s+2: result, ...
wasm!(self.func, { local_get(s); ... local_get(s + 3); ... });
```

ネストした呼び出しは `match_depth += N` でオフセットを進めて衝突を回避する。

### 問題点

1. **count_scratch_depth は IR 段階で関数名を知らない**
   - `IrExprKind::Call` に対して一律4を返す
   - 実際には list.unique_by は s+0〜s+5 (6個)、list.sort_by は s+0〜s+7 (7個) 使う
   - 4では溢れ、8にすると全関数のlocal総数が膨張して他が壊れる

2. **i32/i64の2系統が必要**
   - f64値を一時保存するときi64 localに入れられない（型不一致）
   - 現状float.roundでmem[0..8]にf64を退避している（汚い）

3. **match_depth の管理が散在**
   - stdlib関数内で `match_depth += N` する箇所が30+あり、漏れや二重カウントのバグ源
   - OptionSome/ResultOk等でも `match_depth += 1` が必要で、見落としやすい

4. **mem[] scratch との併用**
   - 一部関数は mem[0], mem[4], mem[8] にも一時値を保存
   - local scratch と mem[] scratch が混在し、どこで何を使うか一貫性がない
   - ネストした呼び出しで mem[] が上書きされるバグの温床

## 理想系: Scratch Allocator

### 設計方針

**scratch local を「名前付きスロット」として管理する allocator を導入。emit 時に必要な分だけ動的に確保し、不要になったら解放。関数コンパイル後に最大同時使用数から local 宣言を生成する。**

### アーキテクチャ

```rust
struct ScratchAllocator {
    // 型ごとの使用中スロット
    i32_slots: Vec<bool>,  // true = in use
    i64_slots: Vec<bool>,
    f64_slots: Vec<bool>,

    // 確保された最大数（local宣言用）
    i32_max: u32,
    i64_max: u32,
    f64_max: u32,

    // base indices（関数コンパイル後に確定）
    i32_base: u32,
    i64_base: u32,
    f64_base: u32,
}

impl ScratchAllocator {
    /// スロットを確保。未使用スロットがあれば再利用、なければ新規追加
    fn alloc_i32(&mut self) -> u32 { ... }
    fn alloc_i64(&mut self) -> u32 { ... }
    fn alloc_f64(&mut self) -> u32 { ... }

    /// スロットを解放（再利用可能にする）
    fn free_i32(&mut self, idx: u32) { ... }
    fn free_i64(&mut self, idx: u32) { ... }
    fn free_f64(&mut self, idx: u32) { ... }

    /// RAII guard: スコープ終了時に自動解放
    fn scoped_i32(&mut self) -> ScratchGuard<'_> { ... }
}
```

### 使用例

```rust
// Before (現状)
let s = self.match_i32_base + self.match_depth;
wasm!(self.func, { local_get(s); ... local_get(s + 3); });

// After (理想系)
let list_ptr = self.scratch.alloc_i32();
let idx = self.scratch.alloc_i32();
let result = self.scratch.alloc_i32();
wasm!(self.func, { local_get(list_ptr); ... local_get(result); });
self.scratch.free_i32(list_ptr);
self.scratch.free_i32(idx);
self.scratch.free_i32(result);
```

### 利点

1. **count_scratch_depth が不要になる** — allocatorが最大同時使用数を自動追跡
2. **型ごとに独立** — i32/i64/f64 を混在して安全に使える
3. **ネスト管理が不要** — match_depth の手動管理がなくなる
4. **mem[] scratch を廃止** — 全てlocal scratch で統一
5. **バグ検出** — 解放忘れや二重解放を検出可能

### 移行戦略

完全移行は大きすぎるので段階的に:

#### Phase 0: 2パス方式への移行（前提条件）

現状: count_scratch_depth (IRスキャン) → local宣言 → emit (1パス)

理想: emit (1パス) → allocatorが最大数を記録 → local宣言を後付け

WASM binary formatは関数先頭にlocal宣言が必要。2つのアプローチ:
- **A. Placeholder + Patch**: 仮のlocal宣言で emit → 実際の使用数でバイナリを書き換え
- **B. 2パスコンパイル**: 1パス目で scratch 使用数を収集、2パス目で emit
- **C. wasm-encoder の後付けlocal**: `Function::new()` の後に local を追加できるか調査

実際にはwasm-encoderの`Function::new(locals)`は最初に呼ぶ必要がある。**B案**が現実的: 1パス目はdry-runでscratch allocationだけ記録し、2パス目で実際にemit。ただしコンパイル時間が2倍。

**妥協案**: count_scratch_depthを改良して「関数名ごとの使用数テーブル」を返す。IRのCall nodeから関数名を取得できるので、テーブル引きで正確な数を返せる。

#### Phase 1: count_scratch_depth の関数名ベース化

```rust
fn stdlib_scratch_depth(module: &str, func: &str) -> u32 {
    match (module, func) {
        ("list", "unique_by") => 6,
        ("list", "sort_by") => 7,
        ("list", "sort") => 5,
        ("list", "map") => 5,
        ("list", "fold") => 3,
        ("list", "filter") => 2,
        _ => 4,  // default
    }
}
```

CallTarget::Module と CallTarget::Method から関数名を取得し、テーブル引きで正確な depth を返す。

**工数**: ~50行の変更。count_scratch_depth の Call ケースを修正するだけ。
**効果**: local out of bounds エラーを根絶。

#### Phase 2: mem[] scratch の廃止

mem[0], mem[4], mem[8] への一時値保存を全てlocal scratchに置換。

対象: calls_list_closure.rs, calls_list_closure2.rs, calls_map.rs, calls_option.rs 等で `i32_const(0); ... i32_store(0)` パターンを使っている箇所。

**工数**: 各関数を個別に書き換え。~20関数、各10-30行の変更。
**効果**: ネストした呼び出しでのmem[]上書きバグを根絶。

#### Phase 3: ScratchAllocator 導入

FuncCompilerにScratchAllocatorを追加。既存の `match_i32_base + match_depth` パターンを段階的に置換。

**工数**: allocator本体 ~100行 + 各stdlib関数の書き換え ~500行。
**効果**: match_depth管理の完全廃止。

#### Phase 4: f64 scratch 追加

ScratchAllocatorにf64スロットを追加。float.round等で使っている mem[] f64退避を置換。

**工数**: ~30行。
**効果**: float操作のmem[]依存を解消。

## 推奨: Phase 1 を即実行

Phase 1 (関数名ベース count_scratch_depth) は最小工数で最大効果。残りのlocal out of boundsエラー2件を即座に解消できる。

Phase 2以降は中長期の改善として、必要に応じて着手。

## 関連ファイル

| ファイル | 役割 |
|---|---|
| `src/codegen/emit_wasm/mod.rs:310-320` | FuncCompiler struct (match_i32_base, match_depth) |
| `src/codegen/emit_wasm/functions.rs:42-50` | 関数コンパイル時のlocal allocation |
| `src/codegen/emit_wasm/closures.rs:200-215` | lambda コンパイル時のlocal allocation |
| `src/codegen/emit_wasm/statements.rs:240-280` | count_scratch_depth |
| `src/codegen/emit_wasm/calls_list_closure*.rs` | stdlib関数 (scratch消費元) |
