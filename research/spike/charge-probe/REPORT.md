# Stage 1 charge-trace preservation — first empirical run

Status: **PASSED — 9/9 comparable runs three-point identical (2026-08-02)**
Spec: [docs/roadmap/active/logical-time-implementation.md](../../../docs/roadmap/active/logical-time-implementation.md) §4
再実行: `ALMIDE_FUEL_PROBE=1 almide run fixtures/<f>.almd [--target wasm]`（この branch のビルドで）

## 何を検証したか

logical-time-proofs.md が「証明しないもの」と明示した唯一の前提 —

> 両 renderer は共有 MIR の charge 事象を保存する（同じ事象、同じ順序、各 1 回）

— を、実装して実測した。`Op::Charge` を IR→MIR 境界（関数入口 + 全 LoopStart 直後、
W1 配置）に挿入し、native（生成 Rust + thread-local カウンタ）と wasm（exported
mutable globals + ゲスト内 epilogue 印字）の両レッグで
**(stdout, consumed, trace_hash)** の三点を比較した。trace は順序敏感
（`t*1000003+site` の wrapping、site = 関数名 FNV + 関数内連番 — 関数順序非依存）。

## 結果

| fixture | 対象経路 | 判定 |
|---|---|---|
| loop | while ループ | **MATCH** (1002) |
| recursion | 再帰 fib(20) | **MATCH** (21892) |
| branch | if/else + match | **MATCH** (602) |
| strings | 文字列連結ループ | **MATCH** (102) |
| mutual | 相互末尾再帰 5000 段 | **MATCH** (5002) |
| switch | 12-arm dense match（**br_table 経路**） | **MATCH** (1002) |
| fusion | 算術密集（**fuser 経路**） | **MATCH** (2002) |
| nested | 二重ループ | **MATCH** (2602) |
| recursion + `--wasm-opt` | **Binaryen -Oz 越し** | **MATCH** (21892) |
| list | lambda 入り filter/map | native wall（v0 fallback）— 比較不能 |
| bce | list.get ループ（BCE 経路） | native wall — 比較不能 |

**乖離ゼロ。** 最有力容疑だった renderer-local peephole（fuser・br_table 化・
bounds-check-elision・wasm-opt）はいずれも charge を落とさず、順序も乱さなかった。

## 消費値の意味論的検証（数字が正しいことの独立確認）

- fib(20): 呼び出し木サイズ 21891 + main 入口 1 = **21892** ✓
- loop 1000 回: 入口 1 + ループ頭通過 1001（脱出判定の回も通る）= **1002** ✓
- is_even(5000): 呼び出し 5001 + main 1 = **5002** ✓

## 既知の限界（Stage 1 スコープ外として明示）

1. **native wall = probe 消失が無音**: v1 native が wall して v0 fallback すると
   probe 行が出ない。fallback 検出を probe 失敗として扱う配線が本実装では必要。
2. **fan{} の並列 native**: thread-local カウンタは arm スレッドの消費を合算しない。
   arm 直列化 or atomic + 順序正規化が Stage 3（race）で必要 — deterministic-bounds
   の本丸がここに現れる。
3. **trap 経路**: proc_exit 直行で epilogue が走らず probe 行が出ない（両レッグ同様）。
4. 計測は entry + loop 頭のみ（粒度 v0）。可変コスト（Dyn charge）は未実装。

## 判定

charge-trace 保存は、この粒度・この経路集合では**成立している**。Stage 1 の
本実装（spec/wasm_cross fixture 化、charge certificate、property test）に進む
根拠が取れた。反証は出なかったが、探索空間は 9 プログラム — 網羅ではない。

## 昇格（2026-08-02、同 branch）

spike の発見をゲートに固定した:

1. **wall honesty**: probe 中の native v0 fallback は hard error（`src/cli/mod.rs`
   `render_v1_native_or_fallback`）。無音の未計測 run は構造的に不可能になった。
2. **静的 certificate**: `charge_probe::{wasm,native}_charge_sites` + 順序敏感な
   first-occurrence 比較。`translation_validation::wasm_pattern` の Charge claim も
   site 特異に強化（drop したその charge を名指しで落とす）。
3. **常駐ゲート**: `tests/charge_probe_test.rs` — 動的三点 × 8 fixture + 静的
   certificate × 8 + wall honesty × 2 を 1 テストで（`cargo test --release
   --test charge_probe_test`、~2.4s、wasmtime 不在時は動的層のみ skip）。

almide-mir 既存 605 lib テストは全緑（Op::Charge の追加は無破壊）。

## Stage 2 垂直スライス — fan.bounded が両ターゲットで着地（2026-08-02、同 branch）

`fan.bounded(compute.ms(100)) { heavy(1000) } ?? fallback` が native v1 / wasm 両レッグで
動き、**決定的境界**が実証された:

- **flip point**: `heavy(1000)` の消費は probe 実測で 1002 charge units（entry 1 +
  ループ頭 1001）。`compute.us(1001)` は EXHAUST、`compute.us(1002)` は OK — **両ターゲット
  で同一の 1µs 刻みの点**で切り替わる（boundary.almd、gate で assert）。理論値と実測の
  厳密一致。
- probe 併用でも三点一致（bounded 込み consumed=503010 / trace 一致）。
- 診断 4 種が ADR-0001 どおり発火: bare Int / Duration 混入 / 非 call body / 未知単位
  （閉集合を列挙）。

### 実装形（logical-time-implementation.md からの差分）

- **アウトライン desugar**: `fan.bounded` は合成 fn `__almd_bounded_N(budget, args…) -> T`
  （enter → body call → exit）に外出しされ、exit が**判定を永続化**（$__b_verdict）、
  呼び出し側がスカラーで読む。`bounded ?? fb` は**融合形**（Result 値が一度も存在しない
  完全スカラー If）— native rung に heap-Result ABI が無いことへの解。裸の bounded は
  ResultOk/ResultErr ノード（wasm で動作、native は既存の rung wall）。
- budget prims は `PrimKind::{BudgetEnter,BudgetExhausted,BudgetExit}`（scalar prim floor）。
  min-cap（EIP-150）は enter/exit の 2 op。fuel は i64::MAX から減算、probe の consumed は
  MAX − fuel。
- `compute.*`/`duration.*` は checker の名義型（防火壁）+ lowering での Int(ns) erasure。

### 仕様からの deviation（本実装 PR までに解消 or 明記維持）

1. metered-clone 特殊化なし — bounded を含むプログラムは**全関数**が計量される
   （bounded を含まないプログラムは 1 バイトも変わらない）。
2. 飽和演算なし（構築子は素の i64 乗算 — S3 と差分）。負値 trap も未実装。
3. body は単一 call・非 Result 戻りに制限（v1 と宣言済み）。
4. callee 内で発散する body は切れない（lazy verdict — モデルが検証済みの overrun 形。
   完走後の判定は厳密）。
5. UFCS 曖昧診断（n.ms()）未実装。matrix gate（S6）未実装。
6. fan{} 並列 native と bounded の相互作用は未定義のまま（Stage 3）。

## Stage 3 垂直スライス — fan.race が両ターゲットで着地（2026-08-02、同 branch）

`fan.race(budget?) { arm; arm }` が native v1 / wasm 両レッグで動き、**証明済みの
lockstep ≡ (spend, index) lex-min 意味論**が実物になった:

- 勝者 = 最小消費の完了（cheap(7)=~2 units が heavy(2000)=~2003 units に勝つ）
- **同着はソース順**（同一 spend の 2 arm → arm 0）
- budget は候補集合だけを変える（全滅 → 台帳定数 Err → `??` fallback、選別 → 残った arm）
- **勝者出現境界**: heavy(500) の spend = 502 units。`compute.us(501)` は全滅、
  `compute.us(502)` で arm 1 が勝者に — 両ターゲット同一の 1µs 点（gate で assert）
- probe 併用でも三点一致（consumed 9018 / trace 同一）
- legacy `fan.race(thunks)` は parser が旧 AST を再構築して **E027 を署名移行ヒント**
  として発火（設計どおりの改訂。fixture 更新済み）

### 実装形

- 各 arm を `outline_metered_arm`（Stage 2 の bounded と同一のアウトライナ）で計量領域化。
  BudgetExit が verdict に加えて **spend を永続化**（$__b_spend / 新 prim BudgetSpend）し、
  呼び出し側が arm ごとに読む。
- 勝者選択は**スカラー if-value の逐次 fold**（candidate = 非枯渇、better = 無勝者 or
  spend 厳密小 — 同着でソース順が自然に出る）。`?? fb` 融合形は Result 値ゼロの完全
  スカラーで native rung を通る。
- 予算なし形は i64::MAX 番兵（発散ガード不在 = fan {} と同じ停止性規約）。

### Stage 3 の deviation（Stage 2 の 6 件に追加）

7. **trap は保守的**: 実行された trap はプログラムを落とす（可視窓による敗者 trap の
   消去は未実装 — strict per-site cap + unwind が必要で、これは metered-ABI 本実装の
   領分）。逐次 + lazy のため両ターゲットで同一に落ちる（決定的だが spec より過剰報告）。
8. arm の Err スキップ（候補から外す）は未実装 — v1 arm は非 Result 単一 call なので
   Err 経路自体が存在しない。

## P0 修正 2 件（2026-08-02、同 branch）

1. **fmt のコード破壊を修正**: formatter の wildcard が `fan.bounded` / `fan.race` を
   `/* unformatted */` に置換してコードを**消していた**（データ損失クラス）。整形
   アームを追加 — bounded はインライン、race は arm 改行の block 形。roundtrip
   （fmt → 再パース緑）と冪等性を確認。
2. **CM-1 を実測で再校正（v0.1 → v0.2）**: draft の 1000ns/unit は実測に対して
   **21 倍過大**で、ADR-0001 D5 の宣言帯（5 倍）を自ら破っていた。参照測定:
   heavy(1000) = 1002 units が release で 47.0µs → 46.9ns/unit。**50ns/unit** に
   pin（帯比 1.07）。`compute.us(51)` で bounded が通り、`compute.us(26)` で race に
   勝者が出る — 新フリップ点も両ターゲット同一（gate 更新済み）。
   教訓: 「ms を名乗る」の誠実さは定数 1 つに懸かっており、D5 校正ゲートの CI 常設は
   merge 前必須。定数は 3 箇所（charge_probe 定数 / wasm BudgetEnter render / native
   BUDGET_SHIM）に現れる — 単一ソース化は本実装 PR で。
   **後日談（v0.2 → v0.3）**: この 47µs という参照測定自体が混入だった — 1002 units
   の実行は µs 級で、ms 級の process spawn を通しては解像できない。D5 ゲートを常設した
   瞬間（T3-7）にゲートが ratio 0.05（≈18 倍過大）で落とし、heavy(100M) = 1e8 units の
   min-of-3 実測（native 0.25s / wasmtime 0.28s — **両ターゲット ≈2.5ns/unit で一致**）で
   **3ns/unit** に再 pin。境界 fixture は ns 構築子で 1 unit 精度に強化: bounded は
   3006ns（=1002×3）、race は 1506ns（=502×3）ちょうどで両ターゲット同時に反転する。
   宣言時計上の 1ns が判定を変える、が現在の決定性の主張形。二度の誤校正
   （21 倍過大 → 18 倍過大）を人手レビューは素通りし、ゲートだけが両方を捕えた。

## fan{} 並列 × budget の裁定（T3-8、2026-08-02）

「native の fan{} は実スレッド、fuel カウンタは thread-local — 併用したら計数は
どうなるのか」という無定義状態の解消。結論: **併用は型システムが既に排除している**。

- **metered region 内の fan{}**: 不可能。region body は pure 文脈で検査され
  （can_call_effect=false）、`fan {}` は effect 文脈必須（E007）。したがって
  metered region がスレッドを跨ぐことは構文的に起こらず、thread-local カウンタは
  常に安全。checker pin: `t3_8_fan_parallel_inside_metered_region_is_rejected`。
- **fan{} arm 内の metered region**: 定義済みかつ決定的。region の enter/exit は
  自分の実行コンテキスト（native なら自スレッドのカウンタ、wasm なら逐次の
  グローバル）で自己完結し、arm ごとの verdict は跨ぎ観測を持たない。wasm で実証
  （arm1=Ok / arm2=exhausted）。native は fan{}+effect-arm が v1 rung 外で wall し、
  v0 fallback は budget prim 不在の honest 拒否（T3-1 で追加）に落ちる —
  「誤答への経路なし」の状態。native v1 の fan{} rung 開通は branch 外の既存残件。
- 注意書き: probe / --time-report の consumed は main スレッドのカウンタを読むため、
  native 実スレッド fan{} の arm 消費は含まれない（wasm は含む）。semantics
  （verdict）はこの差を観測できない — 観測するには enclosing region が必要で、
  それは上記のとおり不可能。

## Wave 1 — fan v2 表面統一（2026-08-02、同 branch）

race/bounded が block head になったことで残っていた表面の不整合（any/settle だけ
thunk-list）を解消:

- **block 形**: `fan.any { a(); b() }` / `fan.settle { a(); b() }` — parser が
  literal thunk-list AST（内部名 `__any_block`/`__settle_block`）へ脱糖し、frontend
  lowering が名前を正規化。**checker 以深は完全無変更**で両ターゲット即動作。
  fmt は内部名を block 構文へ re-sugar（roundtrip + 冪等確認済み）。
- **thunk-list 綴りの tombstone**（E027 署名移行ヒント）: `fan.any([...])` /
  `fan.settle([...])` は removed。2 引数の mapper 形は「宣言済み・未実装（Wave 2）」
  の専用診断。合成ノードだけが legacy 形の唯一の生産者。
- **移行 10 ファイル**: spec 8（wasm_cross 5 + lang 3）+ diagnostics fixed 2。
  各 pin を読んで移行 — fan_var_thunk_list（#599 の var-bound list pin）は綴りごと
  対象が消えたため「混在 arm 種の list 順決定性」の pin に改記。fan_pure_thunks の
  #514 pin（pure arm の Ok-adapter）は block 形で生存。新 tombstone fixture 2 件追加。
- **async/await の死骸撤去**: `ExprKind::Await` / `IrExprKind::Await` /
  `Decl::Fn.r#async` / `IrFunction.is_async` と全消費アーム（optimize/codegen/
  interp/fmt/mir、約 40 ファイル）を削除。「文法から書けない」が「AST/IR で表現
  できない」へ格上げ。

settle block 形の戻り型は v1 では legacy どおり `List[Result]`（fan-v2.md の
tuple 契約は本実装 PR での deviation 項目に追加）。
