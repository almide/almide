<!-- description: v1 KGI/KPI scoreboard — the terminal goal indicators (trust + writability), the guard invariants that must never degrade (checker size, TB purity, axiom cleanliness, zero claim-drift), and the progress KPIs toward each gap. Weekly fill-in. -->
# v1 KGI / KPI スコアボード

> **STALE (last entry 2026-06-21)** — corpus-wall/ownership-coverage figures have moved
> substantially since (143+ commits mention "corpus"/"ownership coverage" after 2026-06-21 per
> `git log --oneline --since=2026-06-21 --grep="corpus\|ownership coverage"`); needs a fresh
> weekly entry before use.

> **これは何か**: v1 の終局指標(KGI)と、それを守る/攻める指標(KPI)を週次で
> 埋めるスコアボード。このプロジェクトは構造が特殊で ―― **KPI が「攻める系
> (伸ばす)」と「守る系(絶対に劣化させない不変条件)」の2種に割れ、守る系を破った
> 瞬間に攻める系の成果が無効化される**。だから守る系を先に見る。
> **関連**: [v1-system-map](v1-system-map.md) / [receipt-logic](receipt-logic.md) /
> [trust-layer](trust-layer.md)(L0-L4)/ [v1-proof-architecture](v1-proof-architecture.md)。

## 守るものの本質(一行)

> **信じる対象を、小さく・純粋に保つこと。** 検査器は数百行のまま、信頼基底は
> 宣言した底のまま。これが崩れたら、どれだけ性質を増やしても flight-grade は
> 届かない。

---

## KGI(終局指標 ―― これが真なら「勝った」)

勝利は **連言**。片方だけは既知の失敗モード(信頼だけ=CompCert ニッチ /
書けるだけ=ただの AI 言語)。

| KGI | 達成状態(これが真であるべき) | 現在 |
|---|---|---|
| **KGI-T(信頼)** | 第三者が **irreducible floor(Coq カーネル / wasm 意味論 / wasmtime 忠実 / HW / ALS 妥当性)だけ**を信じて、**実プロダクションで使う全プログラム**について[安全性束]を、**数百行の資格化済み検査器**で**毎ビルド再導出**できる。外部資格化(隣接市場 → 航空) | ⬜ 切片のみ |
| **KGI-W(書ける)** | 同一条件・対照群つきで、機械が Almide を**修正して他言語より正確に**書ける(統制 MSR で勝つ) | ⬜ 未測定 |
| **連言(真の勝利)** | 上記 2 つが**同時に**真 ―― 機械が最も正確に書け、かつその出力が flight-grade で信頼できる | ⬜ |

**[安全性束]** = メモリ安全 / leak なし / capability 有界 / 型確定 / call 規約適合 /
バイトがソース意味論を refine(C-FAITHFUL)/ byte 再現(C-REPRO)。

---

## 守る系 KPI(不変条件 ―― 1つでも破れたら攻める成果は無効)

**目標は「常に = この値 / ≤ この上限」。伸ばすのではなく、劣化させない。**

| 守る KPI | 守るべき値 | 現在 |
|---|---|---|
| **検査器規模** | ≤ 数百行・**プログラム / 言語 / コンパイラ複雑度に非依存**(規模 ∝ #イベント文字 + #subset 性質 + #op→パターン表) | ✅ |
| **信頼基底(TB)** | = 宣言した floor。**creep ゼロ**(import が増えても台帳で境界づけ) | ✅ TRUSTED_BASE.md 維持 |
| **公理純度** | 全定理 `Print Assumptions` ⊆ 標準・信頼拡張(native_decide 等)ゼロ | ✅ "Closed under the global context" |
| **証明の独立検査** | kernel-checked + coqchk + クロスバージョン(0 sorry / 0 Axiom) | ✅ 9.1.1 + 9.2 |
| **主張ドリフト** | = 0(公開主張 ⊆ 機械照合) | ⚠️ 機械照合は未(手動で 1 件検出・修正) |
| **silent 通過** | = 0(受理 ⟹ 安全 or `[COMPILER BUG]` 停止。shortcut 全機械ゲート) | 🔄 hole-hunt 未乾 |

**この表の意味**: 攻める成果を「守る KPI を破って」買ったら、それは前進ではなく
**純損失**(検査器が数千行になれば、全言語を覆っても資格化対象が消える)。
これが「整える勝負」の KPI 表現 ―― **不変条件を破る前進は、無い方がマシ**。

---

## 攻める系 KPI(進捗 ―― KGI へ向けて伸ばす)

| 攻める KPI | 0 ──────────→ 完了 | 現在 | gap / C-* |
|---|---|---|---|
| **性質被覆** | 安全性束 の 何 / 8 | **4 / 8**(mem✅ name✅ cap✅ type✅ / leak・reuse・call-mode・byte 残) | C-SAFE / C-PROVEN |
| **証明書形式の表現力** | eager → perceus → full | **eager**(i/a/d/m 合法、r/b 拒否) | 横断(形式 v1) |
| **実プログラム被覆**(③ EXECUTE して v0 byte一致) | 端まで実行して v0 一致する実 repo/関数 | **実 repo 2 full-conquest**(`csv` 4/4 + `svg` records render)+ 多数 fixture ―― いずれも byte一致 + leak-free(10⁴ loop)+ corpus-wall ACCEPT | Gap 3 |
| **言語面被覆**(③ 実行 path) | subset → call → 制御フロー → closure → nested heap → 全言語 | **call / 制御フロー / heap-result if/match / closure(defunc inline) / nested-heap list / records(構築・field・spread・再帰drop・List[Record] literal+concat) / `Map[String,String]` entries** ―― 実 repo が EXECUTE。残: Set / guards / 一般 Map(非String値) / TS-target | Gap 3 |
| **バイト束縛** | §3 契約(信頼) → 証明済み | **信頼のまま** | **Gap 1(本丸)** |
| **frees / leak-freedom** | eager(leak 既知の負) → frees(leak 証明済み) | **eager** | Gap 2 |
| **統制 MSR** | 未測定 → 対照群との差 | **未測定** | 柱 A |
| **外部** | ローカル → CI 強制 → 多様性 2 実装 → CertiCoq 機械語 → 隣接資格化 → 航空 | **CI 強制 ✅** | KGI-T |

### gap ↔ 攻める KPI 対応

- **Gap 1**(最重要・最深): バイト束縛 ―― witness ⟹ wasm バイトを §3 契約から
  証明済みへ。WasmCert-Coq import + ランタイム heap refinement 証明 + per-build V'。
- **Gap 2**: frees / leak-freedom / reuse ―― 精密所有権モデル(Perceus per-edge)を
  MIR に。証明書形式の a/m/r 文字がその ground-fact 化の入口。
- **Gap 3**(= ③ 実行パリティ、次の本命) ―― **2026-06-14 実測でスコープ訂正**:
  検証鎖(frontend → MIR → 証明検査器)は **call も制御フローも既に通る**(実測 ACCEPT:
  `helper()`呼出 `im` / `if-then-else` `im` / `match Option` `ad|im` / `for`累積 `ad`、
  かつ gate.sh に two_functions/transitive_caps が既在)。`render_wasm`/`render_rust`
  も MIR op(CallFn 含む)を扱い、V(translation_validation #570)は **pattern 検査**で
  op→wasm 命令の存在を確認する。**stale だった「subset・call なし」は検証側には当て
  はまらない** ―― 検証側は call/制御フロー programs の **所有権**を(call は cert 上
  deferred/elided のまま)検証できている。
  **したがって ③ の真のフロンティアは「v1 が実プログラムを faithful に EXECUTE して
  v0 と byte 一致」**: 今 V は走らせず pattern 照合のみ、実行は v0(二重オラクルの
  reference、receipt C-REPRO「until v1 parity」)。lower は cert のため call を defer
  するので、**実行には call を un-defer(忠実呼出 + 実結果)する EXECUTION path が要る**。
  ③ = **v1 実行 path を建てて v0 とパリティ**(faithful call → 制御フロー実行 → closure →
  nested heap)、各段を v0 byte 一致で被覆テスト。実プログラム被覆(端まで=実行して v0
  一致)が進捗計。**GTM 解錠条件**。検証側が先行している分、ここは「実行を建てる」新軸。

  **第一スライス実測(2026-06-14): EXECUTION 入口の道具を建てて 1 本走らせた**:
  `crates/almide-mir/examples/render_program.rs`(.almd → 全関数 lower → MirProgram →
  `render_wasm_program` → 完全 wat module、EXECUTION 側の emit_cert_from_source 相当)を
  新設(production/検証コードは無改変=安全)。実測 ―― render は op の構造(user 関数
  call `$double`・算術・制御フロー)は出すが、RUNTIME は仮足場(preamble の手書き WAT:
  `$print_list`/`$print_int`/list_copy/itoa のみ。`$print_str`/`$int.to_string` 等は無し)。

  **第二スライス実測(2026-06-14): HEAP 値の実プログラムが v1 実行 path を通って v0 と
  byte 一致(print 無し・規律遵守)**。3 本 ―― `fn greeting()->String="hi"; main{let _g=
  greeting()}` / List 戻り call / String chained call ―― が **v1(lower→MIR→render_wasm→
  wasm→wasmtime)で exit 0・出力一致**。2 修正(両方 render 内、手書き WAT 不増):
  ① **`Init::Str` un-defer**(lib.rs に `Init::Str(String)` 追加、alloc_init が LitStr に
  実バイトを載せる。所有権 cert は不変 ―― Alloc は内容に依らず `i` 一個)。
  ② **call-result-repr 修正**(`value_reprs_wasm` が CallFn の `result` Repr を読む。
  String/List 戻り call は Ptr=i32 ハンドルで、scalar i64 既定だと `$alloc` の i32 と型不整合)。
  回帰テスト `heap_returning_call_types_result_as_i32_handle`(end-to-end build_and_run)新設。
  corpus-wall(in-profile 4083・3 性質 ACCEPT)・gate・cargo test 93/0 全 green。

  **print = self-host を実証で確認(規律の壁)**: 唯一の実 print 経路は `println→PrintStr`
  (`lower/calls.rs`)で `$print_str` を要求。手で preamble に `$print_str` を足すと
  `println("hello")` は走り v0 と byte 一致した **が** discipline test
  (`handwritten_wasm_runtime_does_not_grow`、baseline 11)が即停止 ―― **まさに v0 の罠**で、
  test message も「self-host the new routine in Almide and call it via CallFn」。なので
  `$print_str` ルーチンは revert、`PrintStr→(call $print_str)` arm は残す(将来の self-hosted
  print への正しい CallFn 配線)。`$print_int`/`$print_list` は実ソースに producer 無し
  (テスト fixture 専用)= 仮足場の print は実プログラムから死んでいる。**print の前進 = Phase 3
  self-host**(低レベル Almide subset: メモリ/host-call プリミティブ + Almide で print_str)。
  **③ の次スライス候補(順): (a) scalar 呼出の実行 → 第三スライスで DONE(下記)。(a') scalar 値計算
  (算術/リテラル)―― `Op::Const` 値運搬 + `IntBinOp`(IntOp は Add/Sub/Mul のみ、Div/Mod/比較は要拡張)。
  print が無い今 end-to-end で観測不能 ⇒ PrintInt unit テストでのみ検証可、Op enum 12箇所変更で invasive。
  (b) print = self-host(骨太、Phase 3)。**

  **第三スライス実測(2026-06-14): SCALAR 呼出が v1 実行 path を通って v0 と一致 + 検証被覆も向上**。
  `try_lower_scalar_call`(lower/calls.rs)新設 ―― scalar-result の Named / pure-Module 呼出を実
  `CallFn{dst:Some, args:lower_call_args, result:Some(scalar)}` に lower(heap 呼出 binds.rs:114 を
  heap-push 抜きでミラー)、bind/tail/失敗時は **ロールバック→defer**(Const + elided marker)で totality
  保持(新規 wall ゼロ、in-profile 4083 不変)。battery 4本(`add(2,3)` literal 引数 / 未使用 scalar 結果 /
  入れ子 `g(f())` / scalar+heap 混在 `mix(5,"hi")`)が **v1 で valid wat → wasmtime → v0 と exit/出力一致**
  (print 無し ⇒ 値は未観測だが構造が走る)。**副次効果: 検証被覆 UP** ―― 延期呼出の実体化で
  caps 3528→**3582**(+54 関数 TAINTED→VERIFIED)、ownership 13007→**13153**(+146、heap 引数の Alloc+Drop バランス)。
  **3エージェント敵対的検証 SOUND**(brick #56 系の健全な caps 回収 ―― 不健全 flip ゼロ、2関数は de-taint で
  真の Stdout 到達を露出し正しく未検証維持、name 1:1 保存で caps 集合不変、double-count gate 0、proven-checker
  backstop)。回帰テスト `scalar_user_call_lowers_to_executable_callfn`(from-source lowering、cargo test 94/0)。
  **残: module scalar 呼出は `$string.len` 未定義で実行ダングリング(= heap module 呼出と同じ self-host ギャップ、
  検証は改善)。corpus-wall.sh caps step に既存ハーネス flake(3582 連続 checker spawn の OS-OOM/shared-scratch、
  決定的再チェックで 0-reject ―― 将来 batch/in-process 化)。**

  **⚠ 設計の核 ―― v0-reuse は v0 の罠(v1-mir-architecture.md §4・⚠注を参照)。正は
  SELF-HOST RUNTIME**: ランタイムを **Almide で書き**、同じ Core→MIR→target を通して
  v1 が自己コンパイルする(dogfooding、rt-oracle/drift クラス消滅)。
  **alloc/RC の最小プリミティブだけ MIR に残し、Push/IndexSet/Print/string/list/json/…
  は self-host runtime fn への Call にする**(op に焼かない)。
  現状 render_wasm.rs の手書き WAT(list_copy/itoa/print を op に焼いた仮足場)は「v0 の罠
  そのもの」=「走る」を示す仮足場であり、**手書き WAT を増やさないことが規律**。収束は
  (a)焼いた op を runtime fn の Call に置換 → (b)MIR op をプリミティブへ縮小 →
  (c)runtime を Almide 化(Phase 3、v1-mir-architecture.md §4 / #575/#576)。

  PUNCH-LIST(順): (1) MIR op 集合をプリミティブ(alloc/RC + 不可分)へ定義、Push/Print 等
  を runtime fn Call に置換、(2) 最小 stdlib を **Almide で書く**(`research/selfhost/` の
  方向)→ render_program で program+runtime を v1 コンパイル → v0 と byte 一致、
  (3) faithful Module/Computed call の実行 → 制御フロー → closure → nested heap。
  ※ self-host 設計は骨太 ―― 疲労下で詰め込まず、fresh session で(検証ワークフローで
  de-risk してから)建てる。「stdlib 全部動く」= self-host runtime を建てること(まだ)。

---

## KGI と KPI の関係(運用ルール = この 1 行)

> **KGI = max(攻める系) s.t. 守る系を全てピン留め。** 守る系を破って買った
> 攻める系の値は、加算ではなく**減算**(信頼の核を侵食するから)。

だから週次の見方は「攻める系がどれだけ伸びたか」**の前に**「守る系が全部 green の
ままか」を見る。守るが 1 つでも落ちたら、その週は前進ではなく**後退**。

---

## 週次記録(最新を上に追記)

### 2026-06-21 — ③実行 path で実 repo 2本 full-conquest(csv + svg)
- **守る系**: 全 green 維持。corpus-wall **毎コミット ACCEPT**(4性質: ownership 16556 /
  names 3850 / caps 3141 / caps-transitive 192、leak/double-free なし)。`cargo test -p almide-mir`
  green(回帰テスト 7 本追加)。検査器規模・TB・公理純度・主張ドリフト不変。silent通過は 🔄 据置。
  **規律の自己捕捉**: svg を一度 byte一致でコミット後、**10⁴ leak loop で OOM を発見**(map.entries
  結果の tuple-list が flat drop)→ 即根治(`DropListStrStr`)し次コミットでゲート通過。leak を残した
  状態は ship していない(②規律)。
- **攻める系(③ 実行 path)**: **実 repo 2 本が EXECUTE して v0 と byte一致** ―― `csv`(4/4 公開関数)
  + `svg`(records ベース renderer: rect/text/group/doc/nested children + `map.entries` 属性)。
  どちらも 10⁴ loop leak-free。**実プログラム被覆 1→実 repo 2 full-conquest + 多数 fixture**。
  **言語面被覆**: subset → **call / 制御フロー / heap-result if·match / closure(defunc inline) /
  nested-heap list / records 全域 / `Map[String,String]` entries**。新機構: 再帰 record drop、
  record 引数 drop、List[Record] literal+concat、`(String,String)` tuple-list(`map_entries_str` /
  `DropListStrStr`)、defunc-map 内 self-recursion 許可(`in_defunc_body`)、lower_bind の UnOp arm
  (`let hc = not <call>` の脱落修正)。詳細 [[v1-records-svg]](STATUS 6)/ [[v1-parser-tco-lever]]。
- **性質被覆は 4/8 据置**: leak-freedom は **経験的に clean(10⁴ loop)だが Perceus 証明は未**
  ―― 証明済み性質は増えていない(これは emit/実行層の前進であって証明ではない)。Gap 1(byte束縛)/
  Gap 2(frees 証明)は不変。
- **読み**: ③ の「実行を建てる新軸」が **実 repo を 2 本通すところまで到達**(検証側先行の差を実行側が
  詰めた)。残る言語面 gap = **Set / guards / 一般 Map(非String値)**。次は ④ MSR(柱 A・未測定)と
  Gap 1/2 の証明軸、または Map/Set 依存の薄い実 repo の追加 conquest。守りは 1 つも落とさず前進。

### 2026-06-14 — lower 被覆ほぼ天井 + 2 つの潜在 accept-but-unsafe を発見・封鎖
- **守る系**: 全 green 維持。さらに **silent 通過(hole-hunt)が前進** ―― 検証ワーク
  フロー(敵対的 refute)で **潜在 accept-but-unsafe を 2 件発見し、どちらも封鎖**:
  (1) **Try/Unwrap/Fan の早期 return wasm リーク** ―― deferred-continue 証明書は均衡
  (no_leak) だが v0 wasm の Err パス `return_` が Perceus 終端 rc_dec を飛び越し生存
  ヒープローカルをリーク。**v0 codegen を本丸修正**(emit_early_return_decs、壁ではなく
  実バグ修正)、経験的にリーク解消(100k×100KB err-loop が OOM せず完走)+ 二重解放
  なし(260-file wasm corpus + wasm_gc/runtime/cross-target 全緑)を実証。
  (2) **caps ゲート `mir<=ir` の相殺** ―― FnRef/ClosureCreate マーカーの過剰計数が
  Computed/Method 省略の過少計数と相殺し得る穴を、count_ir_calls で構造的に封鎖
  (今は到達不能、by-construction 堅牢化)。**健全性は GENUINELY 100% 維持**。
- **攻める系**: **lower+verify 被覆 4022→4083(97.3%)**。(あ)小ギャップ一掃
  (heap-tail Block / Map 挿入 / nested tuple destructure / top-level let global /
  break スカラフレーム / ループ・分岐アキュムレータ / Computed-effect-call)+ 上記
  早期 return リーククラスを根治して壁を撤去・−59 回収。性質被覆は **4/8 据置**
  (リーク修正は emit 層であって証明ではない)。**実プログラム被覆は 1 のまま**
  (render を実プログラムまで前進させる ③ は未着手)。
- **③ スコープ実測訂正(同日)**: 検証鎖は **call も制御フローも既に ACCEPT**
  (`helper()` `im` / `if` `im` / `match` `ad|im` / `for` `ad`、gate に two_functions/
  transitive_caps 既在)。stale だった「render は subset・call なし」は検証側に当て
  はまらない ―― 検証側(検査器 + render pattern + V)は先行している。**③ の真の
  フロンティアは「v1 が実プログラムを faithful に EXECUTE して v0 と byte 一致」**
  (今 V は走らせず pattern 照合のみ、実行は v0 が reference=二重オラクル until parity。
  lower は cert のため call を defer するので、実行には call を un-defer する EXECUTION
  path が新たに要る)。詳細は Gap 3 節を更新済み。
- **読み**: **量の軸(lower 被覆)はほぼ天井**。残りは骨太の新軸 2 つ ―― **③ v1 実行
  path を建てて v0 とパリティ**(検証側は先行、実行を建てる新軸)と **④ MSR**(柱 A・
  未測定)。どちらも fresh session で集中して建てる。守りを 1 つも落とさずこの 2 軸を
  伸ばせるかが次の勝負。

### 2026-06-13 — 基準点
- **守る系**: 全 green(検査器小 / TB 宣言済み / 公理純 / CI green・クロスバージョン
  9.1.1+9.2)。主張ドリフトの機械照合と silent 通過(hole-hunt 乾き)は未完。
  **形は無傷** ―― 「整える」は現サブセットに対して達成。
- **攻める系**: 4/8 性質・eager のみ・実 1 プログラム・subset(call なし)・
  バイト束縛は信頼のまま・frees 未着手・**MSR ゼロ**。
- **読み**: 守るべきものは守れている。満たすべき中身が薄い。KGI の両輪
  (KGI-T 切片のみ / KGI-W 未測定)はどちらもこれから。次の勝負は、守る系を
  1 つも落とさずに攻める系(とりわけ **MSR** と **Gap 1 バイト束縛**)を伸ばせるか。

<!-- 週次テンプレ:
### YYYY-MM-DD
- 守る系: [全green か / 落ちた不変条件があれば即記録 → これは後退]
- 攻める系: [動いた攻めKPIと現在値]
- 読み: [守りを落とさず攻めたか。次の一手]
-->

---

## 一言

守るのは「**信じる対象の小ささと純度**」(= 数百行の検査器 + 底まで潰れた TB)。
KGI は「**実プロダクション全体を、その小さな信頼で毎ビルド証明でき、かつ機械が
最も正確に書ける**」の連言。**今は守りが完璧で攻めが初期** ―― 次の勝負は、守る系を
1 つも落とさずに攻める系(MSR と Gap 1)を伸ばせるか。
