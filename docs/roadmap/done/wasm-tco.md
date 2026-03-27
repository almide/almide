<!-- description: Tail call optimization for WASM target to prevent stack overflow -->
# WASM Tail Call Optimization

## Status: 未実装 — deep recursion (100K+) で stack overflow

## 現状

Almide の TCO 戦略はターゲット依存:

| Target | TCO 方式 | 状態 |
|---|---|---|
| Rust | LLVM が自動変換 | 動作する |
| TS/JS | V8/JSC JIT が最適化 | 動作する |
| **WASM** | **なし** | **stack overflow** |

WASM codegen は全ての再帰呼び出しに `call` 命令を使用。コンパイラ側にもランタイム側にも TCO がないため、深い再帰でスタックを使い果たす。

```
// sum_to(100000, 0) → 100,000 フレーム → stack overflow
fn sum_to(n, acc) = if n <= 0 then acc else sum_to(n - 1, acc + n)
```

## 影響するテスト

- `spec/lang/tco_test.almd` — "tco deep recursion" (sum_to 100K)
- 間接的に deep recursion を使うテスト全般

## 選択肢

### A. コンパイラ IR パスで tail call → loop 変換（推奨）

自己再帰の tail call を loop + 引数再代入に変換する IR パス。

```
// Before (IR)
fn sum_to(n, acc) {
  if n <= 0 { return acc }
  return sum_to(n - 1, acc + n)   // tail position
}

// After (IR → loop rewrite)
fn sum_to(n, acc) {
  loop {
    if n <= 0 { return acc }
    let (n', acc') = (n - 1, acc + n)
    n = n'; acc = acc'
    continue
  }
}
```

**利点**:
- 全ターゲット（Rust/TS/JS/WASM）で一貫して動作
- ランタイム依存なし
- WASM proposal の実装状況に左右されない

**検出ルール**: 関数末尾の `Call { target: Named(self_name) }` で、引数が全て self の params と対応

**実装箇所**: `src/codegen/` にナノパスとして追加。mono の後、codegen の前。

**対応パターン**:
1. **直接自己再帰** (Phase 1): `fn f(...) { ... f(...) }` — 最も一般的
2. **if/match 分岐の tail position** (Phase 1): `if cond { base } else { f(...) }`
3. **相互再帰** (Phase 2): `fn f() { g() }; fn g() { f() }` — trampoline が必要
4. **CPS 変換** (Phase 3): 一般的な tail call — 難度高

### B. WASM return_call 命令の使用

WASM Tail Call proposal の `return_call` / `return_call_indirect` 命令を使う。

**利点**: 相互再帰も含めて全てのtail callに対応
**欠点**:
- wasmtime: `--wasm tail-call` フラグが必要（デフォルトOFF）
- ブラウザ: Chrome のみ実験的サポート、Firefox/Safari 未対応
- wasm-encoder クレートの対応確認が必要
- ポータビリティ喪失

### C. Trampoline パターン

再帰呼び出しを「次の呼び出し情報を返す」形に変換し、ドライバーループで回す。

```wasm
;; 各関数は "Continue(args)" か "Done(result)" を返す
;; ドライバーがループで Continue を処理
```

**利点**: 相互再帰にも対応
**欠点**: 全呼び出しにオーバーヘッド（ヒープ割り当て）、複雑

## 推奨実装計画

### Phase 1: 自己再帰 tail call → loop（最優先）

1. **Tail position 検出器**: `is_tail_position(expr, fn_name) -> bool`
   - 関数body末尾の Call
   - if/match の各分岐末尾の Call
   - do ブロック末尾の Call

2. **ループ書き換えパス**: `pass_tco.rs`
   - 対象: 自分自身を tail position で呼ぶ関数
   - 変換: body 全体を `loop { ... }` で包み、tail call を引数更新 + `continue` に置換
   - IR ノード: 既存の `Loop` + `Continue` + `Assign` を活用

3. **テスト**:
   - `tco_test.almd` の全テストが WASM で pass
   - Rust ターゲットの既存テストが regression しない

### Phase 2: return_call 対応（optional）

wasmtime のデフォルトサポート待ち。対応したら wasm-encoder の `return_call` を使って相互再帰もカバー。

## 作業量見積もり

- Phase 1 tail position 検出: IR ウォーク、1ファイル ~150行
- Phase 1 loop 変換: IR 書き換え、1ファイル ~200行
- Phase 1 テスト: 既存テストで検証
- 合計: ~350行の新規コード、1-2セッション

## 関連ファイル

- `src/codegen/target.rs` — codegen パイプライン定義
- `src/codegen/emit_wasm/calls.rs:128-137` — `call` 命令の emit
- `src/ir/mod.rs:218` — `Call` IR ノード
- `spec/lang/tco_test.almd` — TCO テスト
