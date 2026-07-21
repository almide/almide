<!-- description: Almide v1: MIR as the single source of semantic truth — greenfield + dual-oracle architecture, mission-first surface language -->
# Almide v1: MIR を唯一の真とする単一意味論アーキテクチャ

Status: active — Phase 0 決定ゲート合格(5/5, §8.1)。**完全グリーンフィールド + 二重オラクル**で実装(§6)、言語表層は **mission-first**(§10)で確定(2026-06-13)。手術前の網2枚(意味法則 oracle / compound-drop churn)敷設・緑。
Owner: compiler
Supersedes / subsumes: #529 (WasmIR), #643/#591/#610 (cross-target emit drift の class),
  および wasm-ownership-emit-mechanization.md の提案部
Relates: flight-grade #563–#586, trust-layer

---

## 0. なぜ v1 か — 病巣は「意味論の実装が2つある」こと

今の Almide は高 IR から **独立した lowering が2本** ある:

- `walker/` → 慣用的 Rust(native)。所有権を Rust に委譲。
- `emit_wasm/` → 手書き wasm(~136 ランタイムルーチン + emit 層で inc/dec を手置き)。

native が「オラクル」なのは Rust という成熟・検証済みの所有権/メモリ系に**委譲**しているから。wasm はそれを**手で再実装**している。cross-target バグの ~72% はこの「wasm が native オラクルから drift」で、#643/#591/#610 はその ownership/memory 版。

巨大な検証群(byte gate / rt-oracle registry / churn / interp 3rd judge / Lean)は**すべて「2実装の drift を後から検出する」装置**。これには2つの原理的限界がある:

1. **「構築による正しさ(by construction)」に原理的に届かない。** 2つの手書き実装を永遠に同期する約束だから、上限は「網羅的検出による正しさ」。完全性ロードマップが「ちまちま潰す」になるのはこのため。
2. **検出自体に穴がある。** #643 がその証拠 — 観測(`__println_int`)が `__alloc` を呼んでヒープ再利用を摂動し corruption をマスクする heisenbug で、byte gate も `almide test` も取りこぼした。

→ **v1 の目的は「検出による正しさ」から「構築による正しさ」へ移ること。** そのために意味論の実装を **1つ** にする。

---

## 1. v1 アーキテクチャ

```
.almd
  → Core IR            型付き・脱糖・ターゲット中立(今の高 IR を整理したもの)
  → MIR                所有権(Perceus)と レイアウトを明示。小ステップ意味論を持つ。 ← 唯一の真
      ├ → wasm         主経路・検証済み・主権的(MIR は wasm 整合 → ほぼ自明に下る)
      └ → Rust         perf と Ferrocene 資格化の踏み台(MIR を Rust idiom に描くだけ)
  runtime              Almide 自身で書き、同じ Core→MIR→target を通る(self-host)
```

**不変条件(v1 の憲法):**

> **所有権とレイアウトの決定は MIR で1回だけ行う。レンダラ(Rust/wasm)は決定を再現するだけで、絶対に再決定しない。**

この一文が #643 のクラスを構造的に不可能にする。今の #643 は「emit_wasm が所有権を手置きして MIR(に相当する解析)から drift した」ことが原因 — レンダラが再決定したから起きた。憲法はそれを禁ずる。

---

## 2. MIR の設計(中核)

### 2.1 明示するもの

1. **Repr(値表現)**: 各値が `Scalar{width}` / `Ptr{layout}` / `Boxed{layout}` のどれか。フィールドオフセット、variant のタグ配置、recursive 型の box 化 — 今 layout registry と box_deref が散在して決めているものを、MIR の**性質**にする。
2. **所有権操作**: Perceus の `dup`(inc) と `drop`(dec) を値に対し明示。使用点ごとに **consume(move) / borrow / dup** を区別。
3. **アロケーション/構築**: alloc + フィールド store を明示ノードに。

**設計規律: 恒久核の量は ValueObject(magic number を型で不可能にする)。** v0 が苦しんだ原因の一つが散在する生定数。v1 の恒久 MIR 核では、量を生の数でなく**値オブジェクト**で持ち、`Repr::Scalar { width: 4 }` のような magic number を**コンパイルエラー**にする:
- 幅 = `ScalarWidth` enum(`Word`/`Double`…、`.bytes()` が唯一の数値化点 = 「Word=4」が1箇所)。生 `4` は書けない。
- `LayoutId` = フィールド private。ad-hoc な `LayoutId(0)` 不可、`PLACEHOLDER_LAYOUT` か registry 発行のみ。
- 将来の layout offset も `ByteOffset` 等の ValueObject で。

→ **ゲートは型システム自体**(常時ON・保守ゼロ・事後検出でなく構造的不可能)。文字列テンプレートの bootstrap WAT(§4.1)だけは型保護できないので、そこは「手書き routine を増やさない」機械ゲート(`handwritten_wasm_runtime_does_not_grow`)で代替する。

### 2.2 正準モデル = Perceus(これが鍵)

**Perceus の RC は、より一般的なモデルである。affine/move は「カウントが静的に 1 と分かる RC」の特殊形にすぎない。** だから MIR を Perceus 所有権(明示 dup/drop/borrow + 明示 Repr)で持てば、両ターゲットは**機械的な翻訳**になる:

| MIR(Perceus) | wasm レンダラ | Rust レンダラ |
|---|---|---|
| `dup v` | `__rc_inc(v)` | `v.clone()` |
| 最後の consume | ポインタを渡す | move(所有権移動) |
| `borrow v` | ポインタを渡す | `&v` / `&mut v` |
| `drop v`(以後未使用) | `__rc_dec(v)` | スコープ末で Drop |
| 別名可変 + 変更(AliasCow) | COW(`__cow_check`) | 変更前に `.clone()` |
| closure capture | capture を dup | move-closure に clone-in |

つまり今 **別々のパス**がやっていること — `pass_perceus`(wasm の inc/dec)、`pass_clone`/CloneInsertion(Rust の clone)、`pass_borrow_inference`(Rust の &)、`pass_capture_clone`、`pass_box_deref` — は**同じ所有権事実の別表現**。v1 はそれを **1つの Core→MIR パスが1回決め**、両レンダラが上表で描くだけにする。

**所有権の決定単位は「束縛/引数ごとの極性」**(Phase 0 ゲートで確認, §8.1-①)。同じソース式でも、その束縛/引数が **consume** なら move(Rust)/ptr 転送(RC)、**borrow** なら `.clone()`(Rust)/dup(RC) になる。これは**レンダラの再決定ではなく、MIR が各束縛・各引数について1回決める事実**。だから「`alias 返却`で payload を move」も「borrow 引数で clone」も、同じ正準形の極性違いにすぎない。

**正準形の既知の縁(Phase 1 で扱う)**: 「**共有されつつ複数 call を跨いで変更され返される**」値だけは、move/borrow の二択に収まらず明示的な共有可変(MIR の MakeUnique/`Rc<RefCell>` 相当)を要する。これは AliasCow 行(§8.1-④)の一般化で、read-only capture や単純 alias は**この縁に達しない**(ゲートで Rc ゼロで描けた)。v1 はこの縁を MIR の**明示ノード**にして、レンダラが暗黙に持ち込まないようにする。

**Lean 資産との整合**: Perceus 健全性・ClosureRc の証明は既にある。それらを MIR の小ステップ意味論に対して述べ直せば、「言語の所有権 = 形式的に証明された Perceus」が **両ターゲットに**効く(今は wasm 側だけ)。

### 2.3 形式の錨

MIR に小ステップ操作的意味論を与える(自分の IR だから出来る)。これを **wasm 形式仕様(機械検証済み)に整合**させる: MIR→wasm の忠実性は wasm spec に対して証明可能。Rust には完全な形式意味論が無いので、Rust レンダラの忠実性は**翻訳検証 + 移行中の byte gate**で担保する(下記 Phase)。

**完全性の核心 — unify は差分独立性を手放す**: 実装を1つにすると、クロスターゲット差分テスト(native↔wasm 一致)が頼っていた**独立性そのもの**が消える。Core→MIR の決定が間違えば**全ターゲットが同一に間違い**、一致ゲートはそこで盲目になる(両方 agree)。これは Phase 0 alias_cow(§8.1-③)で実証済み。したがって完全性は**各還元ステップに、検証対象から独立な oracle**を置くことで達成する。とくに **Core→MIR の正しさの oracle はクロスターゲット差分ではない** — 両者を agree させるから — **ソースレベルの意味法則(値意味論・所有権不変量)の property test + 翻訳検証(#570)**である。interp は「間違った MIR を忠実に実行すれば間違った結果に一致する」ので MIR→target の忠実性用であり、Core→MIR バグには無力。詳細と二層 oracle 構造は [v1-phase1-mir-core.md §5.1](./v1-phase1-mir-core.md)。

---

## 3. 忠実なレンダラの契約

レンダラ R が **忠実** とは、任意の MIR プログラム P について、R(P) の観測挙動が MIR の意味論 ⟦P⟧ と一致すること。具体規則:

- **レンダラは所有権/レイアウトを決定しない。** MIR の `dup/drop/borrow/Repr` を idiom に変換するだけ。再導出したら**バグ**(= #643 の再来を禁ずる lint/assert として実装)。
- **構文(SYNTACTIC)の差は吸収してよいが、意味(SEMANTIC)の決定は不可**(Phase 0 ゲート §8.1-②)。例: Rust に box-pattern が無いので boxed field の nested ctor は tag-guard + `&**deref` に描く — これは**ターゲット構文差の解決**であって所有権/レイアウト決定の再導出ではない。境界は明確: `dup/drop/borrow/Repr/MakeUnique` を**変えたら**意味の再決定=バグ、それ以外の綴り(deref の入れ方、match の展開、一時変数)は**レンダラの自由**。
- wasm レンダラ: 忠実性を wasm spec に対して証明する(長期)。
- Rust レンダラ: 忠実性を翻訳検証で validate。移行中は既存 byte gate が「新 MIR-wasm == 旧 emit_wasm」を保証するオラクル。
- **忠実性違反の観測シグネチャは形依存**(§8.1-③)。所有権再決定は #643 では RC double-free、AliasCow では**値破壊(RC はバランスのまま)**として出る。よって移行ゲートは「leak/double-free 無し」**だけでは不足**で、native↔wasm の **value 等価**(既存 byte gate が満たす)を**両方**要求する。

---

## 4. self-host runtime

`runtime/rs`(native)と emit_wasm の ~136 routine(wasm)の二重実装が、ownership 以外の drift(~72%の本体: `string.lines`/regex/libm/json…)の源。v1 では **ランタイムを Almide で書く** → 同じ Core→MIR→target を通って全ターゲットへ。drift クラスごと消滅し、dogfooding にもなる(`research/selfhost/` の方向)。alloc/RC の最小プリミティブだけ MIR 組み込み、残りは Almide。ブートストラップは段階的に(Phase 3)。

### 4.1 wasm emit の理想形 = 手書き wasm 表面を極小化して証明可能にする

**v0 の wasm emitter が地獄だった根本原因 = 2つの巨大な手書き表面**: ①emit 層が操作ごとに所有権/レイアウトを手で決める(drift・#643・is_heap 二重コピー・tag 二箇所)、②~136 routine を wasm で手書きし native の `runtime/rs` と二重保守。多すぎて正しく保てない。

**理想形の原則: 二度と苦しまない唯一の道は emitter を小さくすること。**

- **MIR が全決定を持つ → レンダラは決して決めない**(§3) → drift も #643 も構造的に消える。
- **手書き wasm = MIR プリミティブ集合の写像だけ**(~20: alloc / offset load・store / scalar 演算 / 制御流 block・loop・if・br / call / rc_inc・dec・cow)。total・網羅・**決定ゼロ**。これが唯一の「苦しむ表面」で、十分小さく**一度書いて wasm 形式仕様に対し忠実性を証明できる**(20-op は証明可能、136-routine は不可能 → 着地形の V/faithfulness の前提)。
- **それ以外は全部 Almide で書き、同じ Core→MIR→wasm を通す**。list/string/map/formatting/RC本体 = 136 routine が **Almide コード**になり、一つのソースから両ターゲットが自動で正しくなる。**手書き wasm ランタイムはゼロ**。
- **境界 = 「MIR プリミティブ op」 vs 「Call to (self-hosted) runtime fn」**。`Push`/`IndexSet`/`Print` のような操作は MIR の特殊 op に焼き込まず、Almide ランタイム関数への `Call` にする。MIR op 集合はプリミティブだけに縮む。

> ⚠ ブートストラップの近道に注意: 経路を最初に走らせるため、`almide-mir` の wasm レンダラ(`render_wasm.rs`)は当面 list_copy/itoa/print を WAT で手書きし `Push`/`IndexSet`/`Print` を op に焼いている。**これは v0 の罠そのもの** — 「走る」を示すための仮足場であり、理想形へは(a)それらを runtime fn の Call に置換、(b)op 集合をプリミティブへ縮小、(c)runtime を Almide 化、で収束させる。手書き WAT を増やさないことが規律。

---

## 5. flight-grade が v1 から「落ちてくる」

| flight-grade 目標 | v1 での実現 |
|---|---|
| #529 WasmIR(構築による構造不変条件) | **MIR そのもの** |
| #563 ALS / #530 規範仕様 / #564 interp を規範に | **MIR の小ステップ意味論 = 規範。interp = それを実行する参照** |
| #570 per-build 翻訳検証証明書(検証済 checker) | Core→MIR と MIR→target の**各1本**を検証。形式仕様付き MIR なら checker が書ける |
| #572 可読 Rust の行レベル追跡 | 単一パイプラインの provenance(.almd→Core→MIR→Rust) |
| #573 Ferrocene / #574 KCG 資格化パッケージ | **仕様された MIR →(仕様された)Rust** = 資格化できる code generator の形。Rust→機械は Ferrocene |
| #575 DO-333 形式クレジット / #576 Lean を runtime へ | Perceus 健全性を MIR 意味論に対し証明。runtime は Almide コード=検証済パイプラインを通る |
| #567 Critical subset / #568 静的メモリ / #569 WCET | **MIR の部分言語/性質**として定義。単一の仕様された lowering は WCET 解析が桁違いに容易 |

今の構造は flight-grade を**別の登山**にしているが、v1 は **flight-grade を設計から落とす**。

---

## 6. 完全グリーンフィールド + 二重オラクル(2026-06-13 確定)

**確定**: v1 は **完全グリーンフィールド**(lexer/parser 含め全部 0 から、別クレート群として fresh に建てる)。当初 §9 で「0 からの書き直しは却下(270 fixture の知識喪失・second-system risk)」としたが、これは**雑すぎた** — second-system risk の本体は「**暗黙知の喪失**」で、Almide のそれは実装コードでなく**実行可能な外部仕様**に結晶化している(270 spec fixture / wasm_cross byte-gate / 100 contracts / 137 rt-oracle / 参照 interp / 意味法則 oracle / Lean / CHEATSHEET・STDLIB-SPEC)。知識は **fixture と contract にある**。捨てなければ崩れない。

**安全な「0 から」の唯一の形 = 二重オラクル付き fresh parallel build:**

- 新パイプラインを**新クレートとして並走**で建て、**旧実装は一切触らず、parity を証明するまで差分オラクルとして温存**。
- 新実装は **(a) 外部仕様の全部(fixture/contract/interp/意味法則 oracle)** + **(b) 旧実装との差分** + **(c) §2.3 の意味法則/形式 oracle** の**三重**で judげる。
- **fixture 単位で parity が出たものから cut over**。失敗したら新パスを捨てても旧は無傷。
- **非交渉の制約(完全性の核心)**: fixture だけでは不完全(R1)。旧実装は**差分独立性**を供給する第2実装 — これを **parity 前に消さない**(§2.3 の盲点が露出する)。

> 表層を変える機能(range/protocol/where、§10)については、旧実装は**変えない意味論にだけ**差分オラクルとして使える。変える表層は新 spec を起こす。**意味法則 oracle は表層非依存なので全面的に生きる**。

**削除リスト(v1 終状態で消えるもの = 2実装選択から来た負債):**

- emit_wasm の emit 層所有権(手置き inc/dec) → MIR の dup/drop に置換
- `pass_perceus`/`pass_clone`/`pass_borrow_inference`/`pass_capture_clone`/`pass_box_deref` → **1本の Core→MIR 所有権+レイアウトパス**に統合
- byte-identity gate(wasm_cross) → レンダラ忠実性が確立したら不要(移行オラクルとして使い切って退役)
- rt-oracle registry + churn(drift 検出として) → ランタイム単一化で不要
- `runtime/rs` ↔ emit_wasm routine の二重保守 → 単一 self-host ランタイム

**⚠ 退役順序の硬性不変条件**(完全性の生命線): 上の「byte gate 退役」は**順序が命**。差分 oracle(旧 emit との差分 / native↔wasm 一致)を消してよいのは、**上位の独立 oracle(interp の規範化 + 意味法則 property test + wasm-spec 忠実性証明)が先に置き換わった後だけ**。さもないと退役の瞬間、§2.3 の盲点 — unify 後は両ターゲットが Core→MIR 決定バグで**同一に**間違う — が露出し、検出層がそれを永久に見逃す。**「差分を消すなら、その前に上位の独立 oracle を立てる」**。詳細 [v1-phase1-mir-core.md §5.3](./v1-phase1-mir-core.md)。

→ **コードは純減する。** v1 は「機能を足す」のではなく「負債を畳んで正しさを構築に変える」。

---

## 7. Phase 計画

- **Phase 0 — スパイク(1–2週、§8)**: 所有権が厄介な少数形で「単一 MIR → 両レンダラが構築で一致」を実証し、決定ゲート(RC と borrow が1つの正準形に乗るか)を通す。
- **Phase 1 — MIR コア + 二レンダラ(本体)** → 実装設計は [v1-phase1-mir-core.md](./v1-phase1-mir-core.md)(6コンポーネント精読でマップ済、5パス+emit_typed_rc_dec→1パスの対応表・canonical cut・脱リスク5段・接地済みリスク台帳 R1-R12)。要旨: 全言語に拡張。wasm を MIR から描く(emit_wasm の layout/runtime 知識を dumb レンダラに転用)。既存 byte gate + corpus を移行オラクルに旧挙動一致を確認しつつ、旧 emit 所有権・二重所有権パスを削除。次に Rust レンダラを MIR 消費に切替。**鍵となる実測: 高 IR は既に「半 MIR」**(RcInc/RcDec は既に IR ノード、LayoutRegistry は既に Repr テーブルでマジックナンバー0)。
- **Phase 2 — MIR 形式化 + interp を規範に**: 小ステップ意味論、interp = 参照(#563/#564)。Perceus 健全性を Lean で MIR に対し証明。
- **Phase 3 — runtime を self-host**: ランタイムを Almide で書き MIR 経由で全ターゲットへ。rt-oracle/churn-as-drift を削除。
- **Phase 4 — flight-grade 成果物**: Critical subset(#567)、静的メモリ(#568)、WCET(#569)を MIR 性質として。per-build 翻訳検証証明書(#570)。KCG パッケージ(#574)。

**規模感(正直)**: スパイク+Phase 1 で最初の四半期(賭けの検証)、v1 完成形まで**年級**。その間 feature/issue 作業はほぼ凍結。最大コストは**検証テール**(所有権変更ごとに全 corpus が再 green 化を要する)。

---

## 8. Phase 0 スパイク仕様(まずここ)

**目的**: §2.2 の中核テーゼ — 「Perceus を正準とすれば、単一の所有権決定から両ターゲットが構築で一致する」 — を、所有権が一番厄介な形で殺すか証明する。

**対象 5 形**(現状 cross-target で割れている/割れ得るもの):

1. **alias 返却** — `fn first(o: Option[String]) = match o { some(s) => s, none => "" }`(返り値が引数の内部を借りる)
2. **`list.get(xs, i) ?? d`**(#643 の Some-box leak の核)
3. **boxed variant の nested pattern** — `match t { Node(Leaf(a), Leaf(b)) => a + b, ... }`(#610)
4. **closure capture** — 可変キャプチャ + 戻り値クロージャ
5. **別名可変 + 変更**(AliasCow) — `var b = a; mutate(b)` で `a` が不変

**最小 MIR(スパイク範囲)**: `Repr ∈ {Scalar(w), Ptr(layout), Boxed(layout)}`、ノード `Alloc/Store/Load/Dup/Drop/Borrow/Consume/Call`、関数境界の所有権(各パラメータ own/borrow)。

**単一パス**: Core→MIR の Perceus ベース所有権解決 1本(既存 `pass_perceus` を出発点に、Rust 側決定も同じ事実から導けるよう一般化)。

**二レンダラ**: MIR→Rust(§2.2 右列)と MIR→wasm(§2.2 中列)の薄い実装。**所有権を再決定しない**ことを assert。

**忠実性チェック**: 5形を両ターゲットでビルド・実行し、(a) 観測一致、(b) #643 が消える、(c) レンダラが MIR の dup/drop 数をそのまま出している(再決定なし)。

**決定ゲート(合否基準)**:
- **合格** → RC と borrow が1つの正準形(Perceus)に綺麗に乗る。Phase 1 へ。
- **不合格(例: 5形のどれかで Rust レンダラが idiomatic move/borrow に翻訳できず、明示 Rc を吐く羽目になり「可読 Rust(#572)」を壊す)** → その事実を明文化し、(i) 正準形を見直す、(ii) Rust 側は明示 Rc を許容し可読性をトレードする、(iii) Rust レンダラを資格化専用と割り切る、のどれを取るか再設計。**安く撤退できる。**

### 8.1 決定ゲート結果 — **合格(2026-06-13, 5/5)**

5形すべてを「単一 MIR 決定 → (A)慣用 Rust と (B)手書き RC(wasm 意味論) の両レンダリング → `rustc --edition 2021 -O` でビルド・実行・比較」で検証。**全形で A=B が構築で一致し、所有権を再決定した buggy variant だけが捕捉された。conditional / fail / escape hatch は 0。** 証明・再実行は `research/spike/v1-mir/`(各形は自己完結メタハーネス、`./run-gate.sh` で一括再検証)、完全記録は `research/spike/v1-mir/GATE.md`。

| # | shape | 1つの決定 | A=B | RC clean | buggy 捕捉 | hatch | verdict |
|---|---|---|---|---|---|---|---|
| 1 | alias_return | 最後の consume = payload ptr 転送、**シェルのみ**解放 | ✓ | ✓ | double-free | none | **PASS** |
| 2 | list_get_643 | alias-inc + scope-dec、反復ヒープ temp を per-iter drop | ✓ | ✓ | leak/divergence | none | **PASS** |
| 3 | boxed_pattern_610 | boxed field を**box 越し borrow**、Leaf payload は Scalar/Copy | ✓ | ✓ | double-free | none | **PASS** |
| 4 | closure_capture | capture = env へ Dup + closure-drop で Drop、各 call は borrow | ✓ | ✓ | rustc reject / double-free | none | **PASS** |
| 5 | alias_cow | 共有され得る ref への変更は **MakeUnique 先行** | ✓ | ✓ | 値破壊(両イディオム同一) | none | **PASS** |

ゲートは「PASS」に留まらず、正準形の**境界**を4点明らかにした(§2.2/§3 に反映済み):

1. **所有権の極性は per-binding/per-parameter の MIR fact**(alias_return)。同じソースでも consume→move、borrow→clone/dup。再決定ではなく束縛/引数ごとに1回決める事実(§2.2)。
2. **SYNTACTIC な差は吸収してよい、SEMANTIC な決定は不可**(boxed_pattern_610)。box-pattern 不在は構文差 → tag-guard+`&**deref`。所有権/レイアウトは再決定しない(§3)。
3. **buggy の観測シグネチャは形ごとに異なる**(alias_cow vs 643)。#643=RC double-free、AliasCow=wrong-output(RC はバランスのまま値破壊)。→ Phase 1 検証は leak 検出だけでなく **value 等価**も要る(§3, §6 移行オラクル)。
4. **shared+mutated-across-calls+returned な capture が Rc<RefCell> 領域の境界**(closure_capture)。read-only capture は Rc ゼロで描けたが、この縁は Phase 1 で MIR の共有可変として扱う(§2.2 既知の縁)。

---

## 9. 却下した代替案

- **検出を強化し続ける(現状維持+)**: by-construction に原理的に届かず、検出に穴(#643 heisenbug)。エントロピーとの戦い。
- **Rust を唯一の真にし wasm を rustc ターゲットに**: 形式仕様の無い Rust 意味論の上に建てることになり、DO-333/Lean の土台にならない。主権も失う。(最初の検討で出たが、形式検証の北極星に反するため却下。)
- **wasm を唯一の真にし Rust を完全に捨てる**: Ferrocene 資格化の近道と native perf/成熟度を失う。→ 「MIR を真・Rust は踏み台レンダラ」に修正(§1)。
- ~~**文字通り 0 から書き直す**: 270 fixture の知識喪失、second-system risk~~ → **この却下は 2026-06-13 に覆した(§6)**。知識は実装コードでなく**実行可能な外部仕様**(fixture/contract/interp/意味法則 oracle)に結晶化しており、それを捨てなければ second-system risk の本体(暗黙知喪失)は起きない。v1 は**完全グリーンフィールド + 二重オラクル(旧実装を parity まで温存)**を採る。**ただし「旧を消してから fixture だけで検証する盲目的 rewrite」は依然却下** — 差分独立性を parity 前に失うため(§2.3)。
- **blind greenfield(旧を即削除・fixture のみで acceptance)**: fixture は不完全(R1)、旧実装という第2実装の差分独立性を失い、Core→MIR 決定バグが盲点に。→ 「旧を parity まで温存する二重オラクル」に修正(§6)。

---

## 10. v1 言語表層の決定 — mission-first(2026-06-13 確定)

グリーンフィールドで表層を進化させるが、**判断基準は Almide の唯一の指標 = LLM が最も正確に書ける(MSR)**。「Swift を参考に」は **Swift の明快さを採り、MSR を食う複雑さは採らない**で確定。

| 機能 | v1 の決定 | 理由 | 根拠 |
|---|---|---|---|
| **range** | Rust `0..5`/`0..=5` → **Swift `0..<5`/`0...5`** | `<` が「未満」を綴りで示し off-by-one の曖昧さが消える = MSR に直接プラス。`...`/`..<` トークン追加(小) | — |
| **where(generic 制約)** | **採用** `type Set[T] where T: Hash + Eq` / `fn max[T](..) where T: Ord` | 制約はエラーを**良くする**(MSR+)。test-where と同一キーワード「body が valid な前提」 | [type-where-constraints.md](./type-where-constraints.md) |
| **protocol(内部モデル)** | **Swift 風にクリーンな witness モデル可**(コンパイラ内部) | 内部表現の整理は表層複雑さを増やさない | — |
| **protocol(ユーザー表層)** | **現状維持**: 宣言的 nominal conformance + `any P`(Go のインターフェース値エルゴ) | `protocol-any-existentials.md` の意図的設計を踏襲 | [protocol-any-existentials.md](./protocol-any-existentials.md) |
| **associated types / conditional / retroactive conformance** | **構文予約のみ・v1 コア延期** | 同 roadmap が MSR を理由に明示却下(「エラーが人間にも LLM にも読めない」)。**greenfield の和解路**=「読めるエラーを最初から作り込んだ AT」は将来 mission 再重み付け時の option として留保 | 同上 §「却下」 |
| **HKT(高階型 kind の where)** | **構文予約のみ・v1 コア延期** | 同 roadmap が「complexity eats MSR」で却下。**Swift にも HKT は無い**(参考にならない、Scala/Haskell 領域)。3機能中で最大複雑・最小 MSR | 同上 |
| **`if let`**(Optional 束縛) | **採用** `if let x = opt { … }`(some-arm で unwrap 値を束縛) | 共通の「unwrap して使う」を match より明快に。Swift/Rust の prior で LLM が正確に書ける。**frontend 脱糖** = `match opt { some(x) => …, none => … }` | — |
| **`guard let … else`**(早期離脱束縛) | **採用** `guard let x = opt else { return … }`(else は必ず diverge: return/throw/exit) | golden path を平坦に保つ(ネスト削減=LLM エラー減=MSR+)。Swift の読みやすさを採る。**frontend 脱糖** = else-must-diverge を checker で強制した match。else が落ちない経路は E-診断 | — |

**確定の一言**: range=Swift化 / where 制約=採用 / **`if let`・`guard let … else` 採用(Optional 束縛、frontend 脱糖、guard の else は diverge 強制)** / protocol=内部のみ Swift風・表層は宣言的 nominal+`any P` 維持 / **associated types・HKT は構文予約して v1 コアは延期**。MSR 最優先を貫く。

`if let`/`guard let` は **frontend 脱糖**(MIR は match に落ちた後しか見ない)なので所有権/レイアウトの新規決定は無く、§3 の renderer 契約に影響しない。実装は v1 frontend 構築時(または既存 frontend の暫定拡張)。guard の **else-must-diverge** は型/制御フロー検査で強制(Swift と同じ規律、`Never` 型で表現可)。
