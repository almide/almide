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

### Status: implementation ready

PerceusVerifyPass (perceus-belt) は 0 violations:
- Global: 全 heap var に Dec あり (leak なし)
- Global: dec_count <= inc_count + 1 (double-free なし)
- Per-branch: if/else/match の全 arm で Dec 一貫性
- 除外: EnvLoad のみ (borrowed = 正当)

### What Phase B guarantees

「検証を通過しない IR は WASM に変換できない」
= コンパイラ内部のバグが検出されずに出荷されることを防ぐ

### What Phase B does NOT guarantee

「検証ロジック自体が正しい」
= 検証関数にバグがあれば、不正な IR が Verified として通過する可能性

## Phase B.5: warn-mode → hard-error(2026-06-07 の実証で前倒し)

**実証データ**: v0.25.0 リリースが `[Perceus] 5 RC violation(s)`(HOF-closure `__perceus_ret` の
RcDec 漏れ。repro = `research/benchmark/stdlib/precise_all.almd` の `bench` fn)を**警告だけで
出荷していた**。`Verified::verify`(almide-codegen/src/lib.rs)は意図的 warn-mode で、violation が
あっても `Verified` を発行する — 現状の `Verified` は「検証を実行した」であり「合格した」ではない。
型 doc の "unverified programs cannot reach emission" と実態が矛盾し、契約 C-041 にも反する。

**判断: 警告は受け手が違う。** violation はユーザーコードのバグではなくコンパイラ挿入 RC のバグで、
ユーザーには直せない。よって診断分類は **ICE**。"leaks, not unsoundness" は深刻度の話であって
ゲート開閉の理由にならない(深刻度が決めるのはメッセージの口調だけ)。

- [ ] `Verified::verify`: violation > 0 で **hard error(ICE 扱い)** — 「コンパイラのバグです。
      報告してください」+ エラーコード付き
- [ ] 脱出ハッチ `--emit-unverified`: 明示フラグでのみ emit 続行し、**manifest に
      `perceus_verified: false` を記録**(認証の waiver モデル: 逸脱は存在してよいが必ず成果物に
      記録される。`make verify` では赤)
- [ ] 今回の 5 violation を修正 + **HOF-closure bench パターンの fixture を spec/ に追加**。
      真の穴はコーパス — このパターンが spec に無いため CI green のまま退行がリリースに乗った
      (C-041 evidence 拡充)
- [ ] 上の Status「0 violations」を「0 violations **on the spec corpus**」に訂正
      (コーパス外で 5 件発生した事実の反映)

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
