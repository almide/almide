原文の description メタデータ：歴史的 Perceus belt 設計、廃止済み emit 型状態、範囲を限定した証明の根拠
<a id="almideperceusbelt"></a>

# AlmidePerceusBelt

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](almide-perceus-belt.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](almide-perceus-belt.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](almide-perceus-belt.zh-CN.md)

> 固定原文の非規範読者用スナップショット。正本は原文であり、自動同期しない。独立した実装検証や引用指示の実行は行わない。歴史的状態、計画、比較主張は原文の範囲を保持する。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/almide-perceus-belt.md)

> 原文注記：冒頭の廃止訂正により、後続の歴史的な絶対安全の標語と Phase B 保証の範囲は制限される。廃止済み型状態境界や旧脱出ハッチは現行 CLI 指示ではない。違反ゼロは当時の 394 file コーパスだけであり、全プログラムではない。訂正された事件は Dec 不足でなく偽陽性で、追加すれば二重解放となる。旧来の将来 Phase A 計画と後続証明作業が併存する。Lean 所有権証明と Coq runtime model は対象が異なり、renderer store と記録仮定は信頼境界として残る。指示・証明は再実行していない。


<a id="historical-type-state-design-and-present-proof-scope"></a>

## 歴史的な型状態設計と現在の証明範囲

以下の Phase B の `Verified`/`PerceusVerify` emit 境界は、廃止された v0 emitter に属していた（[廃止記録](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-codegen/src/lib.rs)）。現在の構造的 emitter の入口条件ではなく、旧 `--emit-unverified` の指示も現行 CLI の案内ではない。

checker が確立するのは、受理した witness に対してモデル化した性質であり、あらゆるコンパイラバグが出荷不可能だということではない。Lean belt は所有権変換をモデル化する一方、後続の Rocq runtime と証明書の作業には、それぞれの対象範囲と記録上の仮定がある。現在の境界は[メモリ安全性](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/MEMORY-SAFETY.md)を参照する。以下のインシデント訂正と後半の Coq 付録は歴史的な根拠であり、旧 Phase A 計画が今も丸ごと未実装だと主張するものではない。

> Almide における RustBelt 相当。型に導かれるメモリ安全性をコンパイラが証明する。

<a id="architecture"></a>

## アーキテクチャ

```
Phase B (Rust type-state): 検証を通らないと emit できない    ← 今ここ
Phase A (Lean 4 proof):    検証ロジック自体が数学的に正しい  ← next
A + B = 完全体
```

<a id="phase-b-rust-type-state-enforcement-current-session"></a>

## Phase B：Rust の型状態による強制（当時のセッション）

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

<a id="status-hard-error-化済みb5-完了-2026-06-07"></a>

### 状態：hard-error 化済み（B.5 完了, 2026-06-07）

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

<a id="what-phase-b-guarantees"></a>

### Phase B が保証すること

「検証を通過しない IR は WASM に変換できない」
= コンパイラ内部のバグが検出されずに出荷されることを防ぐ

<a id="what-phase-b-does-not-guarantee"></a>

### Phase B が保証しないこと

「検証ロジック自体が正しい」
= 検証関数にバグがあれば、不正な IR が Verified として通過する可能性

<a id="phase-b5-warn-mode--hard-error2026-06-07-完了"></a>

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

<a id="follow-upb5-で構造化しきれなかった分明示的トレードオフ"></a>

### 後続作業（B.5 で構造化しきれなかった分、明示的トレードオフ）

- `__tco_*` / `__br_*` の名前プレフィックス除外を **scope-aware Dec accounting** に置換する。現状は
  偽陽性抑制として正しく機能している(Dec は実在、Bind と別ブロック)が、名前依存は理想形ではない。
  `moved_out_vars` のような構造的規則(move-via-`Assign` フロー)で置換できるか要調査。ただし TCO の
  RC 規約は ANF move-out と異なり(`Assign loopvar = Var tmp` と `RcDec tmp` が両方存在)、安易な
  統合は偽陰性リスクがあるため別タスクとして慎重に扱う。
- `--emit-unverified` の manifest 記録(上記）。

<a id="phase-a-lean-4-formal-proof-future"></a>

## Phase A：Lean 4 の形式証明（将来）

Lean 4 で Perceus ルールの正しさを機械証明し、lean4-rust-backend で Rust に変換。

<a id="plan"></a>

### 計画

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
7. Phase B の Verified&lt;T&gt; と統合

<a id="dependencies"></a>

### 依存関係

- lean4-rust-backend: ~/workspace/github.com/O6lvl4/lean4-rust-backend
- Lean 4 toolchain: v4.28.0 （elan 経由で導入済み）

<a id="references"></a>

### 参考文献

- Reinking et al., "Perceus: Garbage Free Reference Counting with Reuse" (ICFP 2021)
- Ullrich & de Moura, "Counting Immutable Beans" (IFL 2019)
- Jung et al., "RustBelt: Securing the Foundations of the Rust Programming Language" (POPL 2018)

<a id="exit-criteria"></a>

## 完了基準

- Phase B: `Verified<FnBody>` 型が WASM emitter の入力に必須
- Phase A: Lean 4 で `perceus_sound` 定理が証明済み
- A + B: 証明済みロジックが型で強制される = AlmidePerceusBelt 完全体


<a id="post-v0270-proof-coverage-gap-2026-06-11"></a>

## v0.27.0 後の証明範囲の空白（2026-06-11）

belt の Lean 定理は IR レベルの Inc/Dec の均衡を証明する。v0.27.0 では runtime 側を実体化した（free-list への push/reuse、rc==0 の二重解放 sentinel、rc_inc の復活 trap、free list を消去する region reset）が、当時これらはいずれも証明対象に入っていなかった。

**この半分は Lean belt ではなく Coq の信頼基盤に入った** — `.lean` file ではなく [`proofs/FreeList.v`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/FreeList.v) を読む：

- 再利用安全性を持つ allocator 状態機械（bump frontier、free-list、ghost live set）— `alloc_not_live`：検証された allocation は現在生存中の block には決してならない（#909 / A1.2）；
- **region reset は古い free-list entry を残さない** — `RegionSave`/`RegionRestore` は任意の window 本体を通じ allocator 不変条件を維持するため、free-list entry は回収済み region を指せず、reset 後も再利用安全性が保たれる（`region_window_reuse_safe`、`region_reset_leaves_no_free_into_the_region`）。window の free-list を残す素朴な代案は、具体的な trace 上で不変条件を破り、かつ生存中 block を再び渡すと証明されている；
- **PINNED_RC の不死性** — `$alloc8` / `__alloc_pinned` block（WASI scratch、preopen table）は、任意の実行を通じて解放されず、free-list に載らず、`$alloc` から返されず、region reset を生き残る（`pinned_stays_immortal_forever`、`p_region_reset_preserves_PINV`）。これが C-042 で破られた不変条件である。

- **「再利用は rc=1 を復元する」** — 対となる [`proofs/FreeListRc.v`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/FreeListRc.v) の COUNT 側。参照カウントを `RuntimeModel.read_rc`（`WasmRcDec`/`WasmExec` が出力 byte に結びつけるのと同じ cell）として対象に入れ、両半分を別々に証明する。allocator は古い count を持たない再利用 block を返し、到着時は正確に 0（`reuse_hands_back_a_zero_count_block`）。constructor の store 後は正確に `RC_INITIAL` = 1（`reuse_restores_rc_1`）。「free-list block の読出し値は 0、生存 block は ≥ 1」という不変条件は任意の実行（`r_steps_preserve_RINV`）と region reset（`r_region_reset_preserves_RINV`）で維持され、それゆえ `$rc_dec` sentinel が壁になる：`double_release_traps`。この対は飾りでなく必要だと証明される — `omitting_the_rc_store_traps_on_the_first_release` は renderer の store が欠落し、再利用 block が最初の release で trap する trace を実行する。

なお信頼に置くのは、この点だけ：emitter が RC 管理下の各 `call $alloc` 箇所で `RC_INITIAL` を store すること。これは allocator でなく renderer の性質なので、allocator の証明では届かない。`almide-mir` の `every_rc_managed_alloc_site_initializes_the_rc_cell` が実行可能な gate となり、renderer source 内の全 16 箇所の `$alloc` を列挙し、各箇所が RC cell を初期化するか、名前付き raw-scratch 箇所（モデル化・証明済みの `alloc_raw` category：解放されない argv/envp buffer）であることを要求する。また `.v` file から `RC_INITIAL` を読むため、定数が乖離できない。emitter レベルの RC 操作（保存 field の dup、runtime 内の inc）も、構造上 IR verifier からは見えないままである。
