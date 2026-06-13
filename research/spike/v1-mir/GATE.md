# Phase 0 決定ゲート — 結果記録

Status: **PASSED — 5/5 shapes, 0 conditional, 0 fail, 0 escape hatch (2026-06-13)**
Spec: [docs/roadmap/active/v1-mir-architecture.md](../../../docs/roadmap/active/v1-mir-architecture.md) §8
再実行: `research/spike/v1-mir/run-gate.sh`（rustc 1.95.0 で検証）

## ゲートが問うたこと

§2.2 の中核テーゼ —

> **Perceus の RC は一般モデルであり、affine/move は「カウントが静的に 1 と分かる RC」の特殊形にすぎない。**

— を、所有権が最も厄介な 5 形で殺すか証明する。各形は **1 つの Perceus 所有権決定**を最小 MIR に持ち、それを **(A) 慣用的 Rust（move/borrow/clone）** と **(B) 参照カウント（wasm 意味論を手書き RC で模した）** の両方に描く。合格条件:

1. A と B が**構築で一致**する（出力が同一）。
2. 忠実な RC は leak / double-free が無い。
3. 所有権を**再決定する**レンダラ（buggy variant）が**捕捉される**。

各形は自己完結メタハーネス: MIR 決定を組み、2 通りに描き、両方を `rustc --edition 2021 -O` でコンパイル・実行・比較し、PASS のときだけ exit 0。

## 結果

| # | shape | 1 つの決定 | A=B | RC clean | buggy 捕捉 | escape hatch | verdict |
|---|---|---|---|---|---|---|---|
| 1 | alias_return | 最後の consume = payload ptr を転送し**シェルのみ**解放 | ✓ | ✓ | double-free | none | **PASS** |
| 2 | list_get_643 | alias-inc + scope-dec、反復ヒープ temp を per-iter drop | ✓ | ✓ | leak/divergence | none | **PASS** |
| 3 | boxed_pattern_610 | boxed field を**box 越し borrow**、Leaf payload は Scalar/Copy | ✓ | ✓ | double-free | none | **PASS** |
| 4 | closure_capture | capture = env へ Dup + closure-drop で Drop、各 call は borrow | ✓ | ✓ | rustc reject / double-free | none | **PASS** |
| 5 | alias_cow | 共有され得る ref への変更は **MakeUnique 先行**（clone \| cow_check） | ✓ | ✓ | 値破壊（両イディオム同一） | none | **PASS** |

shape ファイル: `src/main.rs`（#2, cargo entry）+ `shapes/{alias_return,boxed_pattern_610,closure_capture,alias_cow}.rs`。

## 正準形を精緻化する 4 つの正直な発見

ゲートは「PASS」だけでなく、§2.2 の正準形の**境界**を 4 点明らかにした。これは escape hatch（正準形の破れ）ではなく、正準形が**何を 1 回決めるか**の明確化である（design doc §2.2/§3 に反映済み）。

1. **所有権の極性は per-binding/per-parameter の MIR fact**（alias_return）。同じソースでも consume なら move、borrow なら clone/dup になる。これはレンダラの再決定ではなく、MIR が**束縛/引数ごとに 1 回**決める事実。正準形 §2.2 の「最後の consume → move」はこの極性が consume のときの姿。

2. **SYNTACTIC な差は吸収してよい、SEMANTIC な決定は不可**（boxed_pattern_610）。Rust に box-pattern が無い（`box Leaf(a)` は unstable）ため、boxed field の nested ctor は tag-guard + `&**deref` nested-match に描く。これは**構文の差**であって所有権/レイアウト決定ではない。レンダラはターゲット構文差を解決してよいが、所有権/レイアウトは**絶対に再決定しない**。

3. **buggy の観測シグネチャは形ごとに異なる**（alias_cow vs 643）。「レンダラが所有権を再決定した」クラスは同一でも、#643 は **RC double-free**、AliasCow は **wrong-output**（value-semantics 違反で RC は**バランスしたまま**両イディオムが同一に `a` を破壊する）として現れる。→ Phase 1 の検証は leak/double-free 検出だけでは不足で、**value 等価**も要る。ゲートはこのシグネチャに合わせて正直に組んだ（false double-free を rig しない）。

4. **shared + mutated-across-calls + returned な capture が Rc<RefCell> 領域の境界**（closure_capture の scope note）。本ゲートの read-only capture-called-twice は plain な owned-String move-closure（Fn）に Rc/RefCell ゼロで描けた。しかし「共有されつつ複数 call 跨ぎで変更され返される」capture は alias_cow 形に属し、そこが明示 Rc の始まり。Phase 1 で MIR の MakeUnique/共有可変として扱う（正準形の既知の縁）。

## 結論

5 形すべてで、**1 つの Perceus 所有権決定が両イディオムに忠実に描かれ一致**し、所有権を再決定する buggy だけが捕捉された。RC と Rust move/borrow は **1 つの正準形を共有する**。§8 の合格分岐に従い **Phase 1（MIR コア + 二レンダラ）へ進む**。上記 4 点は Phase 1 の MIR 設計（per-binding 極性、構文 vs 意味の層分け、value-等価ゲート、共有可変の縁）への入力。
