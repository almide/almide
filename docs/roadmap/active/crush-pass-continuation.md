<!-- description: Exact re-entry points for the remaining crush-pass work — gguf ADT walls, Matrix value model, coverage — with per-item diagnosis, probes, and first move -->
# Crush-Pass Continuation — 残件の正確な再開点

> Last updated: 2026-07-04（セッション交代時の引き継ぎ版）

## 現在地（このセッションまでの確定成果）

- **nn walls 22 → 4**。完了: A-1 ggml load / A-3 fft combine / A-4 find_chunk_at /
  A-5 best_pair_index（各実物 byte-verify 済み・pin テスト同梱）
- **B（CG-1 spec-keying）完了**: 全 127 契約を 7章59節の ALS へ keyed
  （T1-16 / S1-6 / C1-10 / R1-6 / D1-7 / M1-13 / I1-3）。check-contracts.sh が
  三層（spec↔contract↔fixture）を全量強制。新契約は spec キー必須
- 全ゲート green: spec 273 / parity 177（3点観測）/ MIR テスト **518/0**
  （strict-value モードで統一実行）/ corpus-wall PCC exit 0
- 今セッションで足した恒久機構: β簡約 desugar、scalar-tuple fold（成分射影・
  preamble stmts・(scalar,Option) tag+payload 展開）、Range call-arg 材料化、
  (String,Int)/scalar-aggregate concat 要素、heap-var list literal（borrow-view）、
  `list.flatten_rc`（rc_inc-on-copy）、tuple unwrap_or→match desugar、
  let-bound tuple-Option match の成分 merge、`list.push`→concat assign rewrite、
  fs.read_bytes_raw self-host + WASI errno→native 文言、(Record,Int) Result ctor、
  **recursive-drop gate の拡張（String / List[scalar] / List[variant] field —
  GGUFValue 級の ADT が rich 扱いになり $__drop_<V> が生成される）**
- 捕獲した実バグ（テスト昇格の配当）: flatten の rc 規約違反（二重解放）、
  print_str の旧 SCRATCH(512) 直書き、unwrap_or tuple の invalid-wasm 型崩れ。
  probe 検証は **3点（stdout+stderr+exit）必須**に是正済み — stdout-only 比較は
  teardown trap を見逃す（実際に見逃した）

## A. gguf（非 Matrix の唯一の残り、2 walls 同根）

`read_array` / `parse_metadata_entries` — GGUFValue ADT の蓄積ループ。
ここまでの開通: list.push rewrite ✓ / 条件付き push（Unit-if 内）✓ /
call の tuple destructure ✓ / ADT rich 化 gate ✓（コミット 9f5eeb62）。

**残る一手（診断確定済み）**: `fn read_one(p) = if … then (IntV(p), p+4) else
(StrV(…), p+8)` 形 — **(ADT, Int) tuple 返し tail の中の ctor call が
`try_lower_variant_ctor` でなく素の CallFn として emit され、`$IntV` が
unlinked になる**。犯人は tuple 材料化経路の Named-call 要素 lower
（`lower_owned_heap_field` の Named-call arm — binds_p4.rs:82 付近 — と、
同種の CallFn emit 箇所）。**修正: CallFn emit の前に
`self.variant_layouts.ctor_to_type.contains_key(name)` なら
`try_lower_variant_ctor(elem)?` へ迂回**（binds.rs の list 要素 arm で既にやった
のと同じ前置判定）。

再現 probe（v0 出力込みで 3点検証すること）:

```almide
type GV =
  | IntV(Int)
  | StrV(String)

fn read_one(p: Int) -> (GV, Int) =
  if p % 2 == 0 then (IntV(p), p + 4)
  else (StrV("s" + int.to_string(p)), p + 8)

fn collect(n: Int) -> (List[GV], Int) = {
  var items: List[GV] = []
  var p = 0
  for _ in 0..n {
    let (val, next) = read_one(p)
    list.push(items, val)
    p = next
  }
  (items, p)
}

effect fn main() -> Unit = {
  let (vs, endp) = collect(3)
  for v in vs {
    match v {
      IntV(i) => println("i:" + int.to_string(i))
      StrV(s) => println("s:" + s)
    }
  }
  println("end:" + int.to_string(endp))
}
```

現在のエラー: `unlinked stdlib/runtime call(s) with no wasm definition: IntV, StrV`。
これが消えたら実物 gguf を再測定（read_array には再帰 + ValArray(List[GGUFValue])
構築もあるので、ctor の List field = ADT brick 5 が次に出る可能性が高い —
その場合 try_lower_variant_ctor の「List field wall」を
`lower_call_args`/リスト builder 経由で開ける）。

## B. Matrix 値モデル（nn 残 3 walls の前提、週級）

per_head_rms_norm / repeat_kv / concat_rows は **Matrix が native 型**
（`@intrinsic("almide_rt_matrix_*")` の AlmideMatrix、v1 に runtime 不在）である
ことが前提条件。map/if の形を直しても callee（matrix.split_cols_even /
rms_norm_rows / concat_cols / from_lists / to_lists）が unlinked。

**方針（(a) を推奨・記録済み）**: v1 では Matrix = `List[List[Float]]` の
self-host として実装（to_lists/from_lists が恒等化。既存の nested-list 機構が
全部効く）。nn が使う fn 一覧:
`grep -oh 'matrix\.[a-z_]*' ~/workspace/github.com/almide/nn/src/*.almd | sort -u`
（~12 fns）。代替 (b) = prim 床に flat buffer + (rows, cols) ヘッダ新設（重い）。

## C. カバレッジ（継続）

65.89%（v0 生産経路込み、per-file 台帳 = proofs/COVERAGE.md）。A/B の各修正を
pin テスト同梱で入れる運びを継続（今セッションで 507→518）。ターゲット:
`lower/control_p5.rs`（defunc エンジン）、`tail.rs`、`binds_p2/p4`。
計測は `bash proofs/coverage.sh`（手動 llvm-cov — cargo-llvm-cov の
オーケストレーションは使わない）。

## D. 運用メモ（再開時に必ず確認）

- **バイナリ置換**: 機構特定済み（同一 working copy の並行セッションの
  make install）。刻印が FATAL 停止するので、ゲート前に `make install` を癖に。
  watcher = `/tmp/almide-binwatch.log`（常駐）
- **並行セッション**: 同リポの frontend/codegen を触る。git status の予期しない
  M はコミットせず放置
- **規律**: v0 ベースライン先行固定 / probe は3点検証（stdout+stderr+exit）/
  計装はコミット前 grep ゼロ（`grep -rn ALMIDE_MIR_DEBUG crates/` = 0）/
  FIXED 宣言は横断バッテリー後 / ratchet は分離コミット（lefthook 強制）
- **横断バッテリー**: `make install && almide test spec/ &&
  bash proofs/output-parity.sh && cargo test -p almide-mir --release &&
  bash proofs/corpus-wall.sh`（PCC 込み全 green が基準線）
- **§4.1**: 手書き WAT runtime の関数数は増やさない（ratchet テスト有り）—
  新ルーチンは self-host（registry）か既存関数へのインライン
- **rc_inc/rc_dec を使う self-host ルーチン**は `coown_names.rs` の許可リストに
  登録が必要（未登録は lowering が壁る — 実際に弾かれて気づく設計）
