<!-- description: The normative Almide Flight Profile — the SPARK/Ravenscar-class language subset for flight-grade code, machine-enforced by the per-build certificate (by proof, not review). Feature IN/OUT/RESTRICTED classification, the resolved keystone open questions (Dup-in-loop IN, break/continue OUT, recursion=acyclicity-reject, nested loops IN), the @flight enforcement architecture, the MISRA/Ravenscar mapping, and honest language residuals. -->
# Almide Flight Profile — 規範仕様(Normative Subset)

> **Goal**: flight-grade Almide コードの**言語サブセット**を規範化する ── SPARK/Ravenscar
> や MISRA C のアナログ。狙いは、ビルド毎の証明書がサブセット所属を**証明で機械強制**
> すること(レビューでなく)。これは [flight-profile](flight-profile.md) ラダー **G-F1** の
> 中核。2キーストーン([flight-wcet-loops](flight-wcet-loops.md) / [flight-rust-ferrocene](flight-rust-ferrocene.md))
> が残した open question をここで決着させる。
> **関連**: [flight-profile](flight-profile.md) §3.5 / [flight-wcet-loops](flight-wcet-loops.md)。

---

## 1. 設計核(一行)

> **飛行サブセットは「既に lower できるもの」の strict subset を追加ゲートで選ぶ(別コンパイラ
> ではない)。非飛行コードは sound な model-one-iteration / `Opaque` 形に下り、ただ
> flight-admissible でないだけ。所属は `@flight` 関数の closed call graph 上で、二層
> (lowering 壁 + 証明済み checker 性質)で強制 ── 各 `@flight` 関数が in-profile である
> 証明を証明書が運ぶ。それが差別化(レビューでなく毎ビルド証明)。**

---

## 2. 特徴分類表(IN / OUT / RESTRICTED)

凡例 ── **IN**: 飛行コードで許可。**OUT**: 禁止(拒否)。**RESTRICTED**: 明示形でのみ許可。
Enforcement は今ある gate(lowering 壁 / 証明性質)か **未**(壁を建てる必要)。

### 2.1 制御フロー

| 特徴 | 区分 | 根拠 | Enforcement |
|---|---|---|---|
| counted loop `for i in lit..end` | **IN** | 静的 trip count を持つ唯一のループ → WCET・確保上限が by-construction。`n` が整礎 ranking | `try_lower_scalar_for_range`(`lower/scalar_for.rs:12`)が既に隔離。証明 backstop **未**(`CountedLoopStart{bound}` + `NoAllocInLoop.v`/`AllocBound.v`) |
| 無界 `while` | **OUT** | 静的反復上限なし ⇒ WCET なし。`LoopBreakUnless{cond}` は任意ランタイム Bool(`lib.rs:255`) | `try_lower_scalar_while`(`lower/control_p3.rs:692`)は実行用に下すが counted でない。飛行ゲートが非 counted ループを拒否 **未** |
| `for x in <heap容器>` | **OUT** | trip count = ランタイム長、静的でない | 飛行ゲートが非 `CountedLoop` 反復を拒否 **未** |
| 再帰(自己/相互、`CallFn`) | **OUT** | 無界スタック+無界仕事、trip 測度なし | call-graph **acyclicity** チェック(§3 (c)) **未**(機構は `reachable_caps` の `visited` に在) |
| `if`/`match`(scalar/Unit 結果) | **IN** | 有界:1 アームのみ実行、マーカーは所有権ゼロ | 実行可能 `IfThen/Else/EndIf`(`control.rs:175,267`)。checker は flat fold |
| `if`/`match`(heap 結果) | **RESTRICTED→IN** | 1 アーム alloc・move-out(`"im"`)、有界 | `try_lower_heap_result_if`(`control.rs:671`)、アームは文字列リテラル/入れ子 if/直接呼出のみ |
| payload-bind / guard / arm-reassign match | **OUT** | パス依存 rebind を flat fold が見られない(潜在 UAF) | 既に **WALLED**(`control.rs:319,330`) |
| `match` over materialized `Option[Int]` | **RESTRICTED→IN** | tag=長さ、scalar payload は値コピー、有界 | `try_lower_variant_match`(`control.rs:307`) |
| 早期 return `Try`(`?`)/`Unwrap`(`!`) | **RESTRICTED** | 早期 exit は制御パス。live per-iteration heap drop を飛ばさない時のみ可 | `lower/mod.rs:247`。**counted loop 本体内は OUT** が安全(teardown 早期パス free が未証明) |
| `break`/`continue` | **OUT** | 厳密 trip 上限を壊す + heap frame 上で per-iteration drop を飛ばす(wasm leak) | counted-range が既に拒否(`control.rs:777`)、heap frame は `wall_break_over_heap_frame`(`:868`)。§3 (b) |
| `guard … else`(発散) | **OUT**(暫定) | 条件早期 exit、失敗パス未モデル | effect-capture のみ(`lower/mod.rs:401`)。モデル化まで OUT |
| `fan.*`(並行) | **OUT** | 非決定スケジューリング、無界 | MIR サブセットに無い(lowering で wall) |

### 2.2 メモリ

| 特徴 | 区分 | 根拠 | Enforcement |
|---|---|---|---|
| ループ外動的確保 | **IN** | アロケータ O(1)(単一プローブ free-list + bump、固定 64KiB、`memory.grow` なし)。有界 count の O(1) alloc は静的 high-water-mark | `Op::Alloc`(`lib.rs:162`)。`AllocBound` で有界化 **未** |
| ループ内確保 | **OUT** | trip count を掛けないと high-water-mark が壊れる | `is_allocating` を `CountedLoopStart/End` 間で拒否 **未** |
| `Init::DynStr`/`DynList`(ランタイム長) | **OUT**(byte 有界飛行) | 長さがランタイム `ValueId`(`lib.rs:137,151`)→ `len` 有界化なしに byte 無界 | `len` 有界証明まで wall **未**(count 基準は許容、byte 基準は不可) |
| static-`Init` heap(`IntList`/`Str` リテラル、`OptSome`) | **IN** | サイズが `Init` から静的既知 → byte 上限可算 | `Op::Alloc` with static `Init` |
| in-place 変異 `xs[i]=v` 等(COW) | **IN** | 有界、`MakeUnique` は共有時のみ clone | `lower_place_mutation`(`lower/mod.rs:419`)。`ListPush`(成長)は**ループ外のみ** RESTRICTED |
| 無界データ構造(Map/Set 成長、ループ内 push) | **OUT** | 成長 ⇒ 無界メモリ。`ListPush` は再確保し得る | ループ内 push = alloc-in-loop ⇒ OUT **未** |

### 2.3 関数

| 特徴 | 区分 | 根拠 | Enforcement |
|---|---|---|---|
| top-level `fn` | **IN** | call graph の単位、有界本体 | `MirFunction`(`lib.rs:393`) |
| `effect fn` | **RESTRICTED→IN** | capability(`Stdout`)を宣言し `reachable ⊆ declared` 検証。I/O は宣言した有界 capability 経由のみ | `lower/mod.rs:109` + `CapabilityBound.v`(`certificate.rs:127`) |
| closure / lambda | **OUT** | capture が未追跡所有権、間接 callee が静的 call graph を壊す | `is_higher_order` 壁(`lower/mod.rs:621`) |
| 高階関数(`list.map(f)` 等) | **OUT** | closure 引数が未モデル capability + 間接 dispatch | `is_higher_order` 壁(`lower/mod.rs:621`) |
| 関数ポインタ / `FnRef` | **OUT** | 間接呼出 ⇒ acyclicity/caps/WCET が静的 callee を失う | `is_higher_order`(`lower/mod.rs:629`) |
| 直接ユーザ呼出 `f(x)`(`CallFn`) | **IN** | 静的 callee 名 ⇒ call graph 明示 ⇒ acyclicity + transitive caps 決定可 | `Op::CallFn`(`lib.rs:214`)。再帰ゲート対象 |
| pure stdlib 呼出 | **RESTRICTED→IN** | `PURE_MODULES` かつ first-order の時のみ | `purity.rs::PURE_MODULES` + `is_higher_order` |

### 2.4 型

| 特徴 | 区分 | 根拠 | Enforcement |
|---|---|---|---|
| スカラ Int/UInt/Float/Bool | **IN** | Copy、refcount なし、固定幅、WCET-flat | `Repr::Scalar{width}`(`lib.rs:75`) |
| 整数算術 Add/Sub/Mul/Div/Mod/cmp | **IN** | 有界定数時間。Div/Mod は 0 で trap(全域・定義済み) | `IntOp`(`lib.rs:283`) |
| **浮動小数算術** | **OUT**(暫定) | **MIR に `FloatOp` が存在しない**(`IntOp` のみ)。Float は `Repr` だが演算が lower 後に無い。flight IEEE-754 は決定性規則も要 | `lib.rs` に不在。fixed-point か決定性 pin float が要 **未**(§5) |
| record | **RESTRICTED→IN** | 有界レイアウト、field access は容器粒度 borrow | `Repr::Ptr{layout}`。layout レジストリは今 placeholder **未** |
| enum/variant | **RESTRICTED** | `Option`(len-as-tag)は可、一般 payload-bind は layout brick 要 | `Init::OptSome`、一般 variant match は WALLED(`control.rs:343`) |
| `Option`/`Result` | **RESTRICTED→IN** | `Option[Int]` は証明済み materialized 形、`Result` 伝播=`Try` | `Init::OptSome` + `try_lower_variant_match` |
| `Repr::Boxed`(再帰型) | **RESTRICTED** | box read は Borrow。再帰**型**は可だが再帰**走査**は再帰(OUT) | `Repr::Boxed`(`lib.rs:81`) |
| generics | **RESTRICTED→IN(mono 後)** | monomorphization が MIR 前に消去、各実体化は具体 `MirFunction` | `mono/` + `repr_of` が `Ty::Unknown` 拒否(`lower/mod.rs:67`) |
| `where` 制約 / protocol(`any P`) | **OUT / RESTRICTED** | `any P` = 存在型 = 動的 dispatch(間接 callee)⇒ OUT。`where` は compile-time、mono で消去 ⇒ IN | `any P` OUT、`where` IN |

### 2.5 stdlib(flight-safe 面)

| グループ | 区分 | Enforcement |
|---|---|---|
| pure モジュール(`int`/`float`/`string`/`list`/`map`/`set`/`math`/`option`/`result`/`value`/`json`/`bytes`/`regex`/`matrix`…)・first-order | **IN** | `purity.rs::PURE_MODULES` + first-order |
| pure モジュールの高階メンバ(`list.map`/`filter`…) | **OUT** | `is_higher_order` 壁(purity と別) |
| effectful モジュール(`env`/`fs`/`http`/`io`/`net`/`process`/`random`/`zlib`) | **OUT**(一般)。`io` print のみ RESTRICTED | `purity.rs`。`Stdout`(`PrintInt/List/Str`)のみ modeled capability |
| impure-plain(`datetime`/`args`/`mem`/`testing`) | **OUT** | `effect` キーワード無しで host 到達(時計/非決定/abort)── 飛行で禁止必須 |
| 無界確保 pure メンバ(`string.repeat`/ループ内 concat/runtime-len `with_capacity`) | **RESTRICTED** | no-alloc-in-loop + DynStr 壁の対象 **未** |

---

## 3. キーストーン open question の決着

- **(a) Dup-in-loop = IN**(alloc-count プロファイル)。ループ本体の `Dup`(`'a'`)は既存
  オブジェクトの refcount を inc = 新規ヒープ無し・O(1) 定数時間 → WCET も high-water-mark も
  壊さない。禁止すると値意味論言語で「ループ内ヒープ読み取り」が全滅。最小証明ブリック
  `NoAllocInLoop.v` は `is_allocating`(`'i'` のみ)で定義済み → 新アルファベット不要。
  zero-heap-traffic プロファイルは決定性 RC タイミングが要る場合の**より厳格な任意 sub-level**
  として予約(既定でない)。
- **(b) break/continue = OUT**(厳密上限)。早期 exit を許すと `AllocBound` が `≤ B` の上界に
  弱化。厳密の方が強く証明も書けてる(`n` が反復毎ちょうど1消費)。lowering が既に拒否
  (`control.rs:777,868`)、許すと新 teardown が要り「最安の壁」に反する。MISRA single-exit
  とも整合。`≤ B` 変種は将来緩和(Brick 2)で規範外。
- **(c) 再帰 = call-graph acyclicity 拒否**。飛行モジュールの call graph(ノード=`MirFunction`、
  辺=各 `Op::CallFn`)は **DAG** でなければならない。検出器は既存:`reachable_caps`
  (`certificate.rs:335`)の `visited` の early-return が cycle hit。飛行ゲートは同じ DFS で
  verdict 反転 ── **active-stack 上で `visited.insert` が失敗したら REJECT**(back-edge=再帰)。
  diamond-dedup でなく真の back-edge の意味で。unknown/cross-file callee は保守的拒否
  (隠れ再帰辺を通さない)。bounded recursion は残務(§5)。
- **(d) 入れ子 counted loop = IN、上限 = trip count の積**。Coq の `lrun` は `LLoop n body` を
  既に再帰処理(モデルは nesting-ready)。総和 = `Σ_loops bound_L × 本体 alloc`、入れ子は内
  bound が外反復数を掛けて**積**上限。残務は emission 側(`scalar_loop_depth` が flat、
  `try_lower_scalar_for_range` が一度一範囲)── Coq 側は準備済み。

---

## 4. 強制アーキテクチャ

**宣言 = 関数毎 `@flight` 属性(モジュールにも sugar)。** compile flag(`--flight`)は不可
(import した非飛行 stdlib まで巻き込む。飛行コードは大プログラム内の**カーネル**)。
whole-module は監査単位として正しいが、強制の**原子は関数**(acyclicity/caps fold が関数毎)。
**決定**: `fn`/`effect fn` 上の `@flight` 属性。`@flight` 関数は `@flight` 関数(と pure stdlib)
のみ呼べる ── acyclicity + caps fold が reachable 集合全体を in-profile に要するから
(`reachable_caps` が既に要する条件と同じ)。飛行境界 = **closed call graph**、これが WCET/
capability 証明の要求そのもの。

**二層強制**(§2.3 / [flight-profile](flight-profile.md) §7.2 と整合):
1. **lowering 壁(未信頼・最安)**:`@flight` 関数の lower 時、既存の値意味論壁に加えて拒否 ──
   非 counted ループ / `CountedLoopStart/End` 間の `is_allocating` / `Init::DynStr|DynList`
   (byte 飛行) / closure・HOF・`FnRef`(既存)/ cycle を閉じる `CallFn` 辺 / 非 `PURE`・
   非 `@flight` callee。拒否時は flight-certified にしないだけ(他は通常通りコンパイル)。
2. **証明済み checker 性質(束縛的・毎ビルド再検証)**:壁は助言、束縛は kernel-proven checker が
   witness を再検証。既存3 witness(ownership/name/caps)に2つ追加(`NoAllocInLoop`/`AllocBound`)+
   call-graph acyclicity witness。checker は flat fold・有界サイズのまま(compiler が `B`/trip を
   計算し witness に載せ、checker は fold + 比較のみ・CFG walk せず)。**証明書が `@flight` 関数毎に
   "in-profile である証明" を運ぶ ── これが差別化。**

---

## 5. MISRA / Ravenscar / DO-178C マッピング(サブセット = 強み)

OUT リストの各項は、DO-178C C-level コーディング標準(MISRA C / Ada-SPARK の Ravenscar)が
**レビューで義務化**している規律。飛行プロファイルは**同じ規律を証明で機械強制**:

| Almide OUT/RESTRICTED | DO-178C C 標準の等価 | 違い |
|---|---|---|
| 再帰禁止 | MISRA C:2012 Rule 17.2 / Ravenscar | 彼ら=レビュー、Almide=**call-graph acyclicity, proof-carried** |
| 無界 while / 非 counted ループ OUT | DO-178C WCET 義務 / MISRA Dir 4.1 / Ravenscar 静的反復 | 彼ら=手動 WCET+レビュー、Almide=**trip count → AllocBound 証明** |
| alloc-in-loop OUT / alloc 有界 | MISRA 21.3(no malloc)/ SPARK no-dynamic-alloc | 彼ら=heap 全禁止か review、Almide=O(1) アロケータ + **静的 high-water-mark 証明**(より寛容かつ証明済み) |
| closure/関数ポインタ/`any P` OUT | MISRA pointer-to-function 制限 / Ravenscar 動的 dispatch 禁止 | `is_higher_order` 壁 + closed call graph |
| break/continue 禁止 | MISRA 15.x(single exit) | exact-bound 要求、lowering 壁 |
| 時計/random/宣言外 I/O OUT | DO-178C resource determinism | **capability-bound 証明**(`used ⊆ declared`) |

飛行エンジニアがこの OUT リストを読むと**馴染みのコーディング標準**に見える。新規性は規則で
なく、**各規則が kernel-proven checker の毎ビルド再実行で discharge される**こと(`make verify`
キラーデモ)。

---

## 6. 正直な残務(言語自体に欠けるもの)

飛行コードが要するが Almide がまだ**表現/証明できない**もの:

1. **bounded recursion** ── プロファイルは再帰を全面禁止(§3 c、`CallFn` cycle に trip 測度なし)。
   実飛行は静的深度の再帰(固定深度木走査)を要すことがある。`@bound(n)` 再帰 + call-graph cycle の
   Coq 測度が欠。
2. **fixed-point / 決定性 float** ── MIR に float 算術 op が皆無(`IntOp` のみ)。DAL-A 制御則は
   fixed-point か決定性 pin IEEE-754(rounding/NaN/Inf/no-FMA)を要す。flight float op set か
   fixed-point 型が欠。
3. **静的サイズ配列 / 有界コレクション** ── 全ヒープ容器が動的サイズ、`DynStr`/`DynList` は
   ランタイム長。`[T; N]`(compile-time N)が欠 ── **byte-level WCET の gating 残務**(これ無しに
   `DynStr`/`DynList` は wall のまま)。layout レジストリは今 `PLACEHOLDER_LAYOUT`。
4. **byte-level メモリ上限**(object-count 上限でなく) ── RC 証明書は byte サイズを捨てる
   (`lib.rs:130`)。今は object-count high-water-mark のみ証明可。size-carrying `LAlloc(size)` 精緻化が
   欠(3 に依存)。
5. **`≤ B` 上界 early-exit** ── exact bound のため禁止(§3 b)。データ依存早期 exit(なお WCET 有界)は
   今 OUT。`AllocBound` の `≤ B` 緩和が欠(証明は対応、プロファイルが許さない)。
6. **実 layout / 可読型** ── record/payload enum/`Vec<i64>` 一律が未 layout(`PLACEHOLDER_LAYOUT`)。
   Rust render は `Vec<i64>` 一律で非 review-grade。DO-178C「ソース可読」が落ちる。layout レジストリ
   brick + `debug_name` side-table が欠。
7. **機能正しさトレーサビリティ**(本プロファイル範囲外だが真の飛行ギャップ) ── プロファイルは
   *安全*(mem/name/caps/bound)を証明、制御則が*正しい*かは証明しない。柱③ = ゼロ。どの
   サブセットも供給できない、アプリの責務 + Almide に無いトレース機構。

**要約**: 検証スパインは gating より先行 ── 証明と検出器は概ね在り、飛行プロファイルは主に
**既存 fold を reject verdict に配線 + Subset 形の Coq 性質を2つ足す**こと(キーストーンの
「作り直しでなく拡張」と整合)。
