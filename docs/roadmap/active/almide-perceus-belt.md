<!-- description: AlmidePerceusBelt — formal memory safety guarantee for Almide -->
# AlmidePerceusBelt

> Almide's equivalent of RustBelt. Type-guided memory safety, proven by the compiler.

## Architecture

```
Phase B (Rust type-state): 検証を通らないと emit できない    ← 今ここ
Phase A (Lean 4 proof):    検証ロジック自体が数学的に正しい  ← next
A + B = 完全体
```

## Phase B: Rust Type-State Enforcement (current session)

Rust の型システムで「検証されていない IR は emit できない」を強制。

```rust
struct Verified<T>(T);  // 検証済みマーカー

impl PerceusPass {
    fn run(fb: FnBody) -> Verified<FnBody> { ... }
}

impl WasmEmitter {
    fn emit(fb: Verified<FnBody>) -> Vec<u8> { ... }  // Verified のみ受付
}
```

### Status: hard-error 化済み（B.5 完了, 2026-06-07）

PerceusVerifyPass (perceus-belt) は **spec コーパス上で 0 violations**（394 ファイルを wasm
emit して計測）。violation > 0 は **hard error**（`almide build --target wasm` を
`error: [COMPILER BUG] Perceus RC verification failed` で拒否）。
- Global: 全 heap var に Dec あり (leak なし)
- Global: dec_count <= inc_count + 1 (double-free なし)
- Per-branch: if/else/match の全 arm で Dec 一貫性
- 除外（leak 判定のみ、いずれも「所有権がこのブロックを離れる」= Perceus "moved" 関係）:
  - `EnvLoad` バインド（クロージャ環境から借用 = 非所有）
  - **`returned_vars`**（関数 return 末尾 = 所有権が関数を脱出）
  - **`moved_out_vars`**（`Block{expr:Some(Var x)}` の `x` = bare-Var ブロック末尾。所有権は
    ブロックの消費側に移り、そちらが Dec を持つ。ANF が生む `__perceus_ret`/`__anf_*` 末尾一時の
    偽陽性を構造的に排除 — `IrVisitor` の網羅走査で収集、名前プレフィックス band-aid ではない）
  - `__tco_*` / `__br_*` 名前プレフィックス（TCO/branch 一時。Dec は実在するが Bind と別ブロックに
    あり、フラットな per-block `count_decs` がローカルに 0 と読む偽陽性。scope-aware Dec accounting
    への置換は follow-up）

### What Phase B guarantees

「検証を通過しない IR は WASM に変換できない」
= コンパイラ内部のバグが検出されずに出荷されることを防ぐ

### What Phase B does NOT guarantee

「検証ロジック自体が正しい」
= 検証関数にバグがあれば、不正な IR が Verified として通過する可能性

## Phase B.5: warn-mode → hard-error(2026-06-07 完了)

**実証データ**: v0.25.0 リリースが `[Perceus] N RC violation(s)`(HOF-closure `__perceus_ret`。
repro = `research/benchmark/stdlib/precise_all.almd` の `bench` fn、最小化 = `f()` の後に
`println("[" + name + "]")` を末尾に持つ HOF)を**警告だけで出荷していた**。`Verified::verify`
(almide-codegen/src/lib.rs)は意図的 warn-mode で、violation があっても `Verified` を発行していた
— 当時の `Verified` は「検証を実行した」であり「合格した」ではなかった。型 doc の "unverified
programs cannot reach emission" と実態が矛盾し、契約 C-041 にも反していた。

**根本原因の訂正（重要）**: これは当初想定した「RcDec **漏れ**」では**なかった**。RC は均衡しており
**偽陽性**である。`__perceus_ret` は自分の定義ブロックの bare-Var 末尾(`Block{expr:Some(Var ret)}`)
= **move-out**で、所有権は消費側の `Bind __anf_*` に移り、そちらが `RcDec __anf_*` を持つ
(dump_rc_trace で実証: `__perceus_ret` への `RcInc` は無く move、消費側 `__anf_1` に Dec が実在)。
ここで Dec を**追加すれば二重解放**になる。verify が「関数 return で脱出する var」(`returned_vars`)
と「ブロックから move-out する var」を区別せず、後者を leak と誤判定していたのが真因。フラットな
`collect_all_tail_vars`(tail-context 解析として `Block{expr:None}` の `Expr` 文へ降りない)では
`__perceus_ret` を拾えず `returned_vars` が空になっていた。

**修正**: `collect_moved_out_vars`(`IrVisitor` の網羅走査、total-by-construction)で全 `Block` の
bare-Var 末尾を構造的に収集し、leak 判定のみを免除する独立集合 `moved_out_vars` を導入。
`returned_vars` への単純マージ(=全チェック skip)ではなく leak のみ免除としたのは、move 済み値への
Dec = 二重解放を引き続き検出するため。5 引数を `VerifyCtx` に集約。名前プレフィックス band-aid では
なく**構造的**な move-out 認識。

**判断: 警告は受け手が違う。** violation はユーザーコードのバグではなくコンパイラ挿入 RC のバグで、
ユーザーには直せない。よって診断分類は **ICE**。"leaks, not unsoundness" は深刻度の話であって
ゲート開閉の理由にならない(深刻度が決めるのはメッセージの口調だけ)。

- [x] `Verified::verify`: violation > 0 で **hard error** —
      `error: [COMPILER BUG] Perceus RC verification failed`「コンパイラのバグです。報告して
      ください」+ `--emit-unverified` 案内。controlled `process::exit(1)`
      (`assert_types_concretized` と同方式・同じ `[COMPILER BUG]` 体裁、`panic!` バックトレース
      でも数値 E-コードでもない — 後者は rustc の `E060x` 空間と紛らわしいため避けた)
- [x] 脱出ハッチ `--emit-unverified`(`almide build` のみ): 明示フラグでのみ emit 続行し warning を
      出す。`run`/`test` は waiver 非対応で常に hard-error(漏れる成果物を黙って実行させない)。
      **manifest への `perceus_verified: false` 記録は未実装** — ビルド manifest 機構自体が未整備の
      ため、trust-layer の receipts 作業(trust-layer.md)に follow-up として送る
- [x] `__perceus_ret` 偽陽性を構造的に修正(上記)+ **HOF-closure fixture を spec/ に追加**
      (`spec/wasm_cross/hof_closure_string_tail.almd`, `// @contract: C-041`)。真の穴はコーパス —
      このパターンが spec に無いため CI green のまま退行がリリースに乗った。fixture は move-out を
      HOF/nested/returned-String/map-churn で網羅し native==wasm バイト同一を保証(C-041 evidence 拡充)
- [x] 上の Status「0 violations」を「0 violations **on the spec corpus**」に訂正

### follow-up（B.5 で構造化しきれなかった分、明示的トレードオフ）

- `__tco_*` / `__br_*` の名前プレフィックス除外を **scope-aware Dec accounting** に置換する。現状は
  偽陽性抑制として正しく機能している(Dec は実在、Bind と別ブロック)が、名前依存は理想形ではない。
  `moved_out_vars` のような構造的規則(move-via-`Assign` フロー)で置換できるか要調査。ただし TCO の
  RC 規約は ANF move-out と異なり(`Assign loopvar = Var tmp` と `RcDec tmp` が両方存在)、安易な
  統合は偽陰性リスクがあるため別タスクとして慎重に扱う。
- `--emit-unverified` の manifest 記録(上記）。

## Phase A: Lean 4 Formal Proof (future)

Lean 4 で Perceus ルールの正しさを機械証明し、lean4-rust-backend で Rust に変換。

### Plan

1. Lean 4 プロジェクト作成 (almide-perceus-belt)
2. FnBody, VarId, Ty を Lean 4 で形式化
3. Perceus 6 ルールを Lean 4 で定義
4. 定理証明:
   ```
   theorem perceus_sound :
     ∀ (fb : FnBody) (vt : VarTable),
       well_typed fb vt →
       rc_balanced (perceus fb vt)
   ```
5. lean4-rust-backend で Rust コードに変換
6. 生成された Rust 関数を almide-codegen に組み込み
7. Phase B の Verified<T> と統合

### Dependencies

- lean4-rust-backend: ~/workspace/github.com/O6lvl4/lean4-rust-backend
- Lean 4 toolchain: v4.28.0 (installed via elan)

### References

- Reinking et al., "Perceus: Garbage Free Reference Counting with Reuse" (ICFP 2021)
- Ullrich & de Moura, "Counting Immutable Beans" (IFL 2019)
- Jung et al., "RustBelt: Securing the Foundations of the Rust Programming Language" (POPL 2018)

## Exit criteria

- Phase B: `Verified<FnBody>` 型が WASM emitter の入力に必須
- Phase A: Lean 4 で `perceus_sound` 定理が証明済み
- A + B: 証明済みロジックが型で強制される = AlmidePerceusBelt 完全体


## Post-v0.27.0 proof-coverage gap (2026-06-11) — CLOSED, in a different proof system (2026-07)

The belt's Lean theorems certify the IR-level Inc/Dec balance. v0.27.0 made
the runtime side REAL (free-list push/reuse, the rc==0 double-free sentinel,
the rc_inc resurrection trap, region resets that clear the free list). At
the time this was written, none of that was in any proof surface, and the
candidate next phase proposed modeling the allocator state machine
(alloc/dec/reuse/reset) and proving the sentinel invariants ("a block on the
free list has rc=0", "reuse restores rc=1", "a region reset empties the
list"), mirroring how `ClosureRc.lean` grew out of the closure-env work.

**That candidate phase shipped** — not as an extension of this Lean belt, but
via the separate v1 trust-spine (Rocq/Coq) proof effort: `proofs/RuntimeModel.v`
models the runtime heap as a linear-memory state machine (refcount in a cell
at `base + RC_OFFSET`) and proves it tracks the abstract RC semantics exactly,
faulting on double-free in lockstep (`mrun_tracks_exec`); `proofs/FreeList.v`
proves the free-list allocator is reuse-safe — a valid allocation is never a
currently-live block (`FreeList.alloc_not_live`, no reuse-after-free); and
`proofs/WasmDecode.v` closes the raw-byte → ISA "last mile", proving the
actual rendered `$rc_dec`/`$rc_inc` wasm bytes trap on double-free and
free-list-link on unique release. See
[certificate-format-v1](certificate-format-v1.md) — bricks 3b (`RuntimeModel.v`),
3c (`WasmDecode.v`), and 4a (`FreeList.v`, A1.2) — for the full theorem list
and the residual honest trusted base (the wasm engine's ISA fidelity, the
assembler grounding, the kernel). Emitter-level rc operations (stored-field
dups, in-runtime incs) are still outside *this* IR-level belt by construction,
but are no longer outside *all* proof — the v1 spine now covers the runtime
half this section used to flag as open.
