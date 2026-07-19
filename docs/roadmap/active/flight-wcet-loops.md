<!-- description: Concrete implementation design for flight keystone (あ) — G-F1/G-F2: counted-loop flight subset + lifting loops into Coq to prove bounded allocation (WCET-by-construction). The counted loop is a SEPARATE structural witness (preserving the flat-fold RC invariant), two Subset.v-shaped properties prove it, and try_lower_scalar_for_range already knows the trip count. -->
# Flight Keystone (あ) — Counted Loops + Bounded Allocation(WCET-by-construction)

> **Goal**: [flight-profile](flight-profile.md) のラダー gate **G-F1**(飛行サブセット
> 定義+強制)と **G-F2**(ループを Coq に持ち上げ・確保上限を証明)の具体実装設計。
> 核心 ── **WCET の真のギャップはアロケータ(既に O(1))ではなくループ**。Coq モデルは
> 今 loop-free 断片(`proofs/Termination.v:16-18`)。これを**別の構造 witness**として
> 持ち上げ(RC の flat-fold 不変条件を壊さない)、`Subset.v` 同型の2性質で「確保 count
> が静的に有界」を証明する。
> **関連**: [flight-profile](flight-profile.md) §3 / [certificate-format-v1](certificate-format-v1.md)。

---

## 1. 設計核(一行)

> **counted loop(反復数が静的定数)を、RC ストリームとは別の構造 witness に持ち上げ、
> `Subset.v` 同型の `NoAllocInLoop` / `AllocBound` 2性質で「ループ本体に alloc 無し ⟹
> 総確保 count はループ非依存の定数」を証明する。`try_lower_scalar_for_range` は反復数を
> 既に知っている(今は捨てている)。最小ブリックは純 Coq で中心主張を証明する。**

なぜ別 witness か ── `OwnershipChecker.Op`(`proofs/OwnershipChecker.v:48-53`)に `Loop`
構築子を足すと、`exec` の flat-fold(`:63`)を消費する全証明(`RuntimeModel.mrun_tracks_exec`、
`Termination.fuel_exec`)が Loop を扱う羽目になり、「v0 i/d は退化ケース・新証明義務ゼロ」
の不変条件(`OwnershipChecker.v:34-36`)が壊れる。`Subset.v` が `check` と独立した別関数で
あるのと同じく、counted loop は**独立 witness + 独立検査関数**にする(既存の3性質3 witness
パターン、`gate.sh`)。

---

## 2. G-F1 — 飛行サブセットの定義と強制

### 2.1 counted-loop の形(Rust 側 `Op` 拡張)

今のループ `LoopStart … LoopBreakUnless { cond: ValueId } … LoopEnd`
(`crates/almide-mir/src/lib.rs:248-252`)は break が**任意ランタイム Bool** ── 反復数が
不可知。counted loop は**反復上限を marker に静的定数として載せる**。既存マーカーは
壊さず(汎用 while を保つ)、並行する制限ファミリを追加:

```rust
/// 飛行サブセットの COUNTED ループ: 静的既知回数 `bound` だけ回る(リテラル境界の
/// `for i in a..b` から lowering が導出)。LoopBreakUnless{cond}(任意ランタイム exit)
/// と違い trip count をここに定数で持つので WCET・総 alloc 上限が導ける。本体は
/// CountedLoopEnd まで = 1 イテレーション、既存マーカーと同じ per-iteration balanced。
CountedLoopStart { bound: u64 },
/// counted ループを閉じ back-edge を張る。LoopBreakUnless は持たない ── exit は
/// カウンタが bound に達することによる(renderer が down-counter br_if を出す。
/// データ依存テストではない)。
CountedLoopEnd,
```

純追加なので `verify_ownership`(`lib.rs:548-551`)・`ownership_certificate`
(`certificate.rs:377`)・`wasm_pattern`(`translation_validation.rs:92-95`)は既存ループ
マーカーの隣に no-op アームを2つ得るだけ ── **既存証明・witness は不変**。

### 2.2 3つのルール

1. **no-alloc-in-loop**(op 列上の決定可能述語)。`is_allocating(op)` = 新規所有ヒープを
   生む op:`Op::Alloc` と heap を返す `Call`/`CallFn`。**これは cert が既に `'i'` を出す
   集合そのもの**(`certificate.rs:343-345,368-374`)── だから G-F2 が新抽象なしで証明
   できる。飛行ルール = `CountedLoopStart`〜`CountedLoopEnd` 間に `is_allocating` な op が
   無い。
2. **bounded-static-alloc**(WCET 関連の集計)。`total_allocs ≤ (ループ外 alloc) +
   Σ_loops (bound_L × 本体 alloc_L)`。ルール1で本体 alloc=0 なら `total = ループ外 alloc`
   = ランタイム値に依存しない定数。
3. **allocator 側**。`$alloc` は既に O(1) 単一プローブ + 固定 64KiB ページ(`render_wasm.rs`)
   なので**確保コストは既に有界**。飛行プロファイルは count を有界化(1,2)し、静的
   high-water-mark = `total_allocs × max_block` を得る。フラグメンテーション(exact-fit
   退化)対策にループ毎単一サイズ or arena-reset を pin ── これは allocator policy で
   checker 性質ではない。

### 2.3 強制の二層(ラダー表 [flight-profile](flight-profile.md) §7.2 と整合)

1. **lowering ゲート(コンパイラ側・未証明・最安の壁)** ── `try_lower_scalar_for_range`
   (`crates/almide-mir/src/lower/scalar_for.rs:12`)で `end` もリテラルなら静的 bound を
   計算し `CountedLoopStart { bound }` を出す。本体を `scalar_loop_depth > 0`
   (`lower/scalar_for.rs:64`)下で lower する間、`is_allocating` な op を拒否 ── 既存の heap 再代入
   壁(`lower/mod.rs:278-292`)の兄弟。拒否時は rollback で sound な model-one-iteration 形へ
   (非飛行プログラムもコンパイルは通る、飛行サブセット外なだけ)。
2. **checker 性質(証明・毎ビルド再検証)** ── G-F2 の `NoAllocInLoop`。lowering ゲートは
   助言、**束縛的強制は証明された checker**(ゲートにバグがあっても backstop)。

「サブセット = 弱みでなく強み」([flight-profile](flight-profile.md) §3.5):counted-range
+ no-alloc-in-loop は DO-178C の C ルール(再帰禁止・動的確保禁止・ループ有界)そのもの、
新規性は**レビューでなく証明書で機械強制**すること。

---

## 3. G-F2 — ループを Coq に持ち上げ・上限を証明

### 3.1 新しい構造 inductive(`OwnershipChecker.Op` は触らない)

```coq
(* proofs/CountedLoop.v — 飛行ループ断片の構造モデル *)
Inductive LOp : Type :=
  | LAlloc : LOp                       (* 新規確保(cert 'i') *)
  | LOther : LOp                       (* 非確保 op *)
  | LLoop  : nat -> list LOp -> LOp.   (* counted loop: bound, body *)

(* 総和インタプリタ: 構造再帰。bound が整礎測度なので fuel 不要。*)
Fixpoint lrun (prog : list LOp) (acc : nat) : nat :=
  match prog with
  | [] => acc
  | LAlloc :: rest => lrun rest (S acc)
  | LOther :: rest => lrun rest acc
  | LLoop n body :: rest => lrun rest (acc + n * loop_body_allocs body)
  end.
```

`LLoop n body` = 「body を厳密に n 回」。構造的なので構造再帰でも fuel でも証明できる。

### 3.2 停止性拡張(`Termination.v` のアナログ)

`Termination.v:45-56` は flat 断片に `length ops` fuel が足りると証明。`LOp` では
**`LOp` 木の構造再帰 + per-loop trip-counter**で停止性は Gallina の総和性から即時。本質は
「止まるか」でなく**step 上限が静的 trip count であること**:

```coq
(* step_bound prog = 非ループ op + Σ_loops n * step_bound(body)。
   この step 数で必ず足りる = どの LLoop も発散しない。*)
Theorem counted_loop_steps_bounded :
  forall prog, fuel_lrun (step_bound prog) prog <> None.
```

**証明戦略: `prog` の構造帰納 + `LLoop` アームは trip-count `n` で整礎。** `n` が有限で各
反復が `n` を1つ消費する。**この `n` が ranking 関数であることが、counted 制限こそが停止性
を証明可能にする理由**(任意 `LoopBreakUnless{cond}` には測度が無い)。

### 3.3 2つの新性質ファイル(`Subset.v` 同型、定理文)

```coq
(* proofs/NoAllocInLoop.v — ループ本体に確保 op 無し *)
Definition not_alloc (o : LOp) : bool := match o with LAlloc => false | _ => true end.
Fixpoint no_alloc_in_loops (prog : list LOp) : bool := (* 各 LLoop body に forallb not_alloc + 入れ子再帰 *)
Theorem no_alloc_in_loops_sound :
  forall prog, no_alloc_in_loops prog = true -> AllocFreeLoops prog.
(* 戦略: prog 構造帰納 + forallb_forall(Subset.v:35 の手) ── subset_check_sound と機械的に同型 *)

(* proofs/AllocBound.v — 総確保 count ≤ 静的上限 *)
Definition total_allocs (prog : list LOp) : nat := lrun prog 0.
Definition alloc_bounded (prog : list LOp) (B : nat) : bool := Nat.leb (total_allocs prog) B.
Theorem alloc_bounded_sound :
  forall prog B, alloc_bounded prog B = true -> total_allocs prog <= B.
Proof. intros prog B H. apply Nat.leb_le. exact H. Qed.

(* キーストーン系: NoAllocInLoop を合成 ── ループ alloc-free なら総和はループ外 alloc
   count = trip count 非依存の定数。これが WCET 関連の主張「確保有界 BY CONSTRUCTION」。*)
Corollary alloc_free_loops_bound_is_constant :
  forall prog, no_alloc_in_loops prog = true ->
    total_allocs prog = count_top_level_allocs prog.
(* 戦略: prog 帰納、LLoop n body アームで loop_body_allocs body = 0 を書き換え n*0=0。
   ここで2ファイルが合成する。*)
```

`alloc_bounded` は自明な `leb`、健全性は `Nat.leb_le` 1行(`subset_check_sound` と同型)。
**内容は系**:`NoAllocInLoop` ⟹ 総和はループ非依存 ⟹ 静的上限が厳密。

### 3.4 証明書 witness 拡張(`certificate.rs`)

第3の witness emitter(`ownership_certificate` / `name_witness` / `cap_witness` と並行):

```rust
/// counted-loop / alloc-bound witness: flatten した LOp 木 + 主張する静的 alloc 上限 B。
/// 形式(Subset 風・内部化可能): "<B>|<flattened LOp>"
///   'A'=LAlloc, 'o'=LOther, 'L<n>{ … }'=LLoop(trip n + 括弧本体)
pub fn alloc_bound_witness_string(func: &MirFunction) -> String { ... }
```

`func.ops` を歩き、`CountedLoopStart{bound}` で `L<bound>{` を開き、`CountedLoopEnd` まで
本体(`is_allocating`→`A`、他→`o`)、閉じる。主張 `B` = コンパイラの静的総和。checker は
`lrun` で実総和を再導出し `actual ≤ B` を検証。driver に `noalloc`/`allocbound` モード追加、
`Extract.v` が2つの `check_*_cert` を抽出(caps と同じ要領)。

### 3.5 サイズ不変条件(最重要規律)

規律 = **checker 規模 ∝ #event文字 + #subset性質 + #op→パターン、プログラム複雑度に非依存**:

- `NoAllocInLoop`/`AllocBound` は `Subset.v` 形(`forallb` + `leb`、1ステップ健全性)。
  新アルファベット3トークン(`LAlloc`/`LOther`/`LLoop`)+ subset 風2性質、いずれも
  checker 内で**定数サイズ**。
- **コンパイラが仕事し checker は subset するだけ**。静的上限 `B` と trip count `n` は
  **未信頼 lowering が計算**(`try_lower_scalar_for_range` が既に持つ)し witness に載せる。
  checker は `lrun` fold + `leb` のみ ── 上限を導出せず・callee を開かない(transitive-caps
  設計と同じレバー)。
- **CFG walk 無し**。RC `exec` fold は flat のまま不変、ループ構造は別 `LOp` witness に住む。

→ 2性質は `Subset.v` サイズ級の小ファイル2本(各 ~60-90 行、大半が例 + `Print Assumptions`)。
**規律は保たれる。**

### 3.6 正直なサイズギャップ(count 先・byte 後)

RC アルファベットはバイトサイズを捨てる(`lib.rs:130`「Alloc は中身に関係なく 'i' 1つ」):

- **count 基準は今証明可能**(§3.3):`total_allocs ≤ B`(確保**イベント数**)。O(1)/alloc の
  アロケータと併せ count 基準 WCET と worst-case **オブジェクト** high-water-mark を与える。
- **byte 基準は追加で必要**:witness が `Alloc` 毎バイトサイズを持ち(`Repr`/`LayoutId` +
  `Init` から)、`LOp` を `LAlloc (size:nat)` に精緻化し `Σ size` を畳む。難所は
  `Init::DynStr { len: ValueId }`(`lib.rs:137`)= **ランタイム長** ── byte 上限には `len`
  自体の有界化(第2の counted 量)が要る。**count 先・byte は静的 `Init` サイズのみ、
  `DynStr` は `len` が有界証明されるまで wall。**

---

## 4. De-risking 順(最小ブリック先)

**Brick 0(中心主張を証明する最小スライス)** ── *「no-alloc 本体の counted loop は確保
count が有界」*。純 Coq、コンパイラ変更なし:
`proofs/CountedLoop.v`(`LOp` + `lrun` + `count_top_level_allocs`)+ `AllocBound.v`
(系 `alloc_free_loops_bound_is_constant`)+ `NoAllocInLoop.v`(`no_alloc_in_loops_sound`)を
書き、`_CoqProject` に追加(`CowSafety.v` の後)。手書き `LOp` 例(`LLoop 1000 [LOther]` の
`total_allocs` = ループ外 alloc)で性質を証明、`Print Assumptions` closed。
**最大の未知(構造 `LOp` モデルが CFG walk なしで `Subset.v` 形の健全性を許すか)を de-risk**
── 許す(1 `Nat.leb_le` + 1 帰納)なら原理的に証明完了。停止性半分も同時(`counted_loop_steps_bounded`)。

**Brick 1(コンパイラに束縛)** ── `Op::CountedLoopStart/End` 追加 + 4 fold に no-op アーム、
`try_lower_scalar_for_range` から `end` リテラル時に emit + no-alloc 本体壁、
`alloc_bound_witness_string` + driver モード + `Extract.v` + `gate.sh` 行を実 `.almd` fixture で
(`for i in 0..1000 { println(...) }` は accept、本体に `Alloc` は reject)。**端から端の実スライス。**

**Brick 2(一般上限)** ── 本体 alloc を許し `total ≤ Σ bound × per-iter` を証明、`end` を
const-foldable に緩和。

**Brick 3(byte 基準)** ── `LAlloc (size:nat)` 精緻化、静的 `Init` サイズのみ、`DynStr` wall。

---

## 5. 正直な難所 / open question

1. **入れ子ループ**:`lrun` は `LLoop` を既に再帰処理(モデルは入れ子可)。だが lowering の
   `scalar_loop_depth` は flat カウンタ ── 入れ子 `CountedLoopStart` の bound 合成(外 × 内)
   と `try_lower_scalar_for_range` の一度一範囲処理が**emission 側の作業**。Coq 側は準備済み。
2. **ループ内 `Dup`(`'a'` 問題)**:ルール1は新規確保(`'i'`)が対象。本体の `Dup` は既存
   オブジェクトの refcount を inc ── ヒープ**オブジェクト数**は増やさないが per-iteration RC
   トラフィックはある。厳格な「ループ内ヒープトラフィック無し」なら `is_allocating` に `Dup`
   を含めるべき、「alloc-count 上限」なら含めない。**どちらが飛行ルールか = 意図的に文書化
   すべき選択**(ラダーは "no-alloc-in-loop" = count なので最小ブリックは除外)。
3. **`break`/`continue` vs 厳密上限**:counted サブセットは早期 exit 禁止で `bound` 厳密。
   条件早期 exit を許すと静的上限は**上界のみ**(WCET には十分、`AllocBound` の `≤ B` は保つ)。
   **厳密(break 禁止)か上界(break 許可・`≤` に弱化)か = open**。証明は両対応、最小は厳密。
4. **再帰 vs ループ**:本設計はループを閉じる。**再帰は依然無界・スコープ外** ── 自己ホスト
   runtime fn の再帰(`CallFn` サイクル、`reachable_caps` が `visited` で検出)に trip 上限なし。
   飛行プロファイルは**再帰を全面 wall**(DO-178C C ルール)= 別の単純ゲート(飛行関数の
   `CallFn` back-edge 拒否)、ここでは未解決。
5. **byte サイズギャップ**(§3.6 再掲):count は今正直に証明可能、byte は静的 `Init` サイズ +
   `DynStr` wall。有界ループ内でも動的長確保は `len` 有界化なしには byte footprint 無界 = 真の残務。
6. **サイズ不変条件を壊す tripwire**:将来ブリックが checker に**上限を導出**させる(B 再検査で
   なく)か、flat マーカーから CFG を再構成して `LOp` fold させると規律が壊れる(checker が
   プログラム複雑度で増える)。**ガード = witness が構造(`L<n>{…}`)と主張 `B` 両方を運び、
   checker は fold + 比較のみ**。`bound_in_witness_matches_emitted_loop` テストで「witness の
   `n` = `CountedLoopStart` の `bound`」を pin(B を膨らます compiler を捕捉)。

---

## 6. 接合点まとめ(file:line)

| 関心 | file:line | 変更 |
|---|---|---|
| counted-loop `Op` | `crates/almide-mir/src/lib.rs:248` | `CountedLoopStart{bound:u64}`/`CountedLoopEnd` 追加 |
| no-op fold アーム | `lib.rs:548-551`, `certificate.rs:377`, `translation_validation.rs:92-95` | 各2アーム |
| bound 導出 + no-alloc 壁 | `lower/scalar_for.rs:12,64`, `lower/mod.rs:278` | counted marker emit、本体 `is_allocating` 拒否 |
| 新 witness emitter | `certificate.rs`(`cap_witness_string` の後) | `alloc_bound_witness_string` |
| Coq 停止性 | model on `Termination.v:27-56` | 新 `proofs/CountedLoop.v` `lrun` + `counted_loop_steps_bounded` |
| Coq 性質1 | model on `Subset.v:22-38` | 新 `proofs/NoAllocInLoop.v` |
| Coq 性質2 | `Subset.v` + `Nat.leb_le` | 新 `proofs/AllocBound.v` + 系 |
| build/extract/gate | `_CoqProject:17`, `Extract.v:17`, `driver.ml:20-26`, `gate.sh:78` | 登録・抽出・モード・実 source 行 |

**キーストーンの要約**: *flat RC ストリームと別の構造 counted-loop witness を足し、その上で
`Subset.v` 形の2性質を証明し、trip count を既に知る `try_lower_scalar_for_range` に bound を
運ばせる。最小ブリック(Brick 0)は純 Coq で中心主張をコンパイラ変更前に証明する。*
