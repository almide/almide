<!-- description: Flight-grade (DO-178C/DO-333/DO-330) gap analysis for the v1 trust spine — the 6 pillars, what the PCC spine already nails, and the two engineering-closeable keystones (lift loops into Coq for WCET-bounding; productionize MIR→Rust to bind the cert to a Ferrocene-qualified flight target) that converge on one flight stack -->
# Flight Profile — 航空品質への接地分析と2つのキーストーン

> **Goal**: [v1-proof-architecture](v1-proof-architecture.md) の PCC スパインが
> flight-grade (DO-178C Level A / DO-333 / DO-330) の**どの柱を既に刺していて、
> どこが足りないか**を実コードに接地して確定し、**工学で閉じられる2つのキーストーン**
> を定義する。航空は [trust-layer](trust-layer.md) の北極星であって近接の売り物では
> ない([v1-kgi-kpi](v1-kgi-kpi.md) の GTM ラダー参照)が、ここで定義する飛行プロ
> ファイルは ① エージェント市場の wasm 主権と**同じ認証済み MIR witness を共有**する
> ので、二者択一ではなく「同じ witness の2人目の消費者を足す」問題になる。
> **関連**: [certification-grade](certification-grade.md)(規格5メカニズムの分解)/
> [certificate-format-v1](certificate-format-v1.md)/ [v1-mir-architecture](v1-mir-architecture.md)(§1 wasm 主権・§9 却下案)。

---

## 1. 結論(一行)

> **PCC スパインは飛行品質6本柱のうち「柱①成果物安全」を世界的に珍しい深さで刺して
> いる。残り5本のうち、工学で閉じられるのは2つ ―― (あ)WCET を縛るため Coq に
> ループを持ち上げる、(い)cert を Ferrocene 資格化された飛行ターゲットに束縛するため
> MIR→Rust を本番化する。両者は独立で、どちらも"作り直し"でなく"拡張"であり、
> 1つの飛行スタックに合流する。**

---

## 2. 飛行品質の6本柱 — 持っているもの vs 足りないもの

DO-178C は「正しい成果物」ではなく「**証拠体系を生む開発プロセス**」を要求する
([certification-grade](certification-grade.md) の規格5メカニズム分解と整合)。

| 柱 | DO-178C が要求 | v1 現状 |
|---|---|---|
| **① 成果物の安全性** | メモリ安全・資源有界・型健全・capability 有界 | 🟢 **強い** — PCC 検査器 + 43 定理、毎ビルド再導出。世界的に珍しい |
| **② オブジェクトコード忠実性** | 実際に飛ぶ機械語 = ソース意味論 | 🟡 **半分** — byte 束縛は「信頼」止まり(§3契約)で未証明。しかも証明対象は **wasm**(飛行機は wasm で飛ばない) |
| **③ 機能要求トレーサビリティ** | 要求 ↔ コード ↔ 検証の双方向 + MC/DC 構造被覆 | 🔴 **ゼロ** — 安全は証明するが「正しい制御則を計算するか」の追跡機構が無い |
| **④ 実行環境の資格** | RTOS / HW / エンジンも DAL-A、WCET 解析 | 🔴 wasmtime は信頼基底に居るが未資格。WCET 解析ゼロ |
| **⑤ 認証ドシエ + ツール資格化** | PSAC〜SAS・71目標・DO-330 TQL | 🔴 証明書は技術成果物であって認証成果物ではない。目標へのマッピング無し |
| **⑥ 当局受理 + 実績** | DER / SOI 監査・service history | 🔴 「検査器を証明 = コンパイラ資格化の代替」を受理した当局はまだ存在しない。飛行実績ゼロ |

**この文書のスコープは ② と ④ の工学で閉じられる部分**(柱③⑤⑥は規格プロセス・多年の
当局作業で、[certification-grade](certification-grade.md) と GTM の領域)。

---

## 3. フロント(あ) — WCET / 静的確保

### 3.1 驚き(朗報):アロケータは既に WCET フレンドリー

`$alloc` は **単一プローブ free-list + bump の O(1)**(`crates/almide-mir/src/render_wasm.rs:610-628`):
free-list 先頭1ブロックを**バイトサイズ完全一致のときだけ**再利用、外れたら `$bump` を
加算。**探索ループ・coalesce・split・size-class が一切ない**。さらに **64 KiB 固定1ページ・
`memory.grow` なし**(`render_wasm.rs:601`)で総メモリは**既に物理上限**(ただし「設計され
た上限」ではなく枯渇でトラップ/破損)。free-list 再利用は証明済み(`proofs/FreeList.v`、
`reuse_is_validated_and_safe`)。

→ **「動的確保は WCET 敵対的」という前提は Almide のアロケータには半分しか当たらない。**
確保プリミティブの WCET はほぼ自明に出る。

### 3.2 本当の WCET ギャップは**アロケータではなくループ**

WCET は本質的に**ループ反復回数**の話。ここに構造的な穴がある:

- **Coq モデルはまだ「ループ無し」断片**。`proofs/Termination.v:16-18` が明言し、
  `OwnershipChecker.Op` にループ構築子が無い。Rust 側 MIR には loop marker
  (`crates/almide-mir/src/lib.rs:240` `LoopStart`/`LoopBreakUnless`/`LoopEnd`)が
  あるのに、**証明スパインはループを一切知らない**(証明書はループ marker を no-op
  扱い)。
- `LoopBreakUnless` は**任意のランタイム条件**(`lib.rs:242`)── 反復回数の静的上限が
  どこにも表現されていない。
- RC 文字 i/a/d/m は**バイト量を抽象化して捨てている**(`Op::Alloc` = 中身に関係なく
  `'i'` 1つ、`lib.rs:129`)── だから**バイト基準の WCET は今のモデルでは表現不能**、
  count 基準なら可。

### 3.3 キーストーン:ループを Coq `Op` 型に持ち上げる

これ1つが複数の飛行性質を**同時に**解錠する:

1. ループの**停止性証明**(今は `Termination.v` の loop-free 断片のみ)
2. **no-alloc-in-loop**(`proofs/Subset.v` と同型の構造述語で証明可。今は lowering の
   `scalar_loop_depth` ゲート `crates/almide-mir/src/lower/control.rs:446` で
   **コンパイラ側 = 未証明**にしかできない)
3. **counted-loop の反復上限 → 総確保 count 上限**(`try_lower_scalar_for_range`
   `control.rs:589` の desugar が反復数を既に知っている ── そこで捕まえる)

### 3.4 スロットイン箇所(自然な順)

| # | 場所 | 性質 | グレード |
|---|---|---|---|
| 1 | lowering の loop-subset ゲート(`control.rs:426,589`、`scalar_loop_depth` 既存) | no-alloc-in-loop を**拒否時点で**強制 | コンパイラ側(未証明)・最安 |
| 2 | 新 cert 性質 + Coq 証明(`NoAllocInLoop.v` / `AllocBound.v`、`Subset.v` を雛形) | 検査器で**毎ビルド再検証** | flight-grade 正・**前提: ループを Coq Op 型へ** |
| 3 | 新 MIR pass(counted loop 限定 + 反復上限導出) | per-loop / 全プログラム確保 count 上限 | WCET 上限の本体 |

`Repr`/`LayoutId` がサイズ情報を既に運ぶ(`lib.rs`)ので、static pool / arena-reset は
`render_wasm.rs` の `$alloc` 差し替えで載る。**フラグメンテーション注意**: exact-fit-only
free-list は混在サイズの churn で bump 膨張に退化する → 飛行プロファイルは size-class
pool か arena-reset を要する。

### 3.5 飛行プロファイル = サブセット(弱みではない)

counted range のみ・no-alloc-in-loop・static pool ── これは **DO-178C 現場の C
(再帰禁止・動的確保禁止・ループ有界)が既にやっている規律そのもの**。違いは
「レビューで守る」を「証明書で機械強制する」に変えること。**プロファイル制約は
弱みではなく、flight code が既に書いている形を proof で enforce する売りになる。**

→ **具体実装設計(Op 拡張・Coq 定理文・接合点・de-risking 順)**: [flight-wcet-loops](flight-wcet-loops.md)

---

## 4. フロント(い) — Rust → Ferrocene 忠実性

### 4.1 驚き(戦略):証明の約75%は既にターゲット非依存

スパインは wasm 境界で綺麗に割れる:

| 区分 | ファイル | 飛行への含意 |
|---|---|---|
| **ターゲット非依存(そのまま再利用)** | `OwnershipChecker.v`(`check_sound`)/ 証明書 i/a/d/m/r/b(= **MIR 所有権事実**、wasm ではない、`OwnershipChecker.v:155-168`)/ `certificate.rs`(target 認識ゼロ)/ `RuntimeModel.v`(抽象 `Mem = Z→Z`)/ `NameTotality` / `CapabilityBound` / `TypeConcretization` / `Subset` / `Termination` / `FreeList` / `CowSafety` / `ALS` | **健全性の核は全部 Rust ターゲットにタダで移る** |
| **wasm 固有(作り直し)** | `Translation.v`(op→wasm 表)/ `WasmEncode.v` / `WasmExec.v` / `WasmRcDec.v` + `translation_validation.rs` の V validator | 4/16 ファイル・**最も薄く最も後回しの層** |

### 4.2 含意:Rust → Ferrocene は最難関の Gap 1 を丸ごと迂回する

- 飛行では **Rust 実行を証明しない**(`RustExec.v` は要らないし作るべきでない)。信頼は
  2つに割れる:
  - **Almide → Rust 翻訳忠実性** = `rust_pattern` 表 + per-build V。`Translation.v` の
    **eager instance は既に証明済み**(`eager_translation_refines_safety`)で、同じ shape。
    綺麗なマッピング(`Dup→.clone()` / `Drop→scope-end` / `Consume→move` /
    `MakeUnique→no-op`)は `render_rust.rs:1-21` に「§3.2 faithful-renderer 契約」
    として既に書かれている。
  - **Rust ソース → 機械語** = **Ferrocene に信頼**(ISO 26262 ASIL D / IEC 61508 資格化)。
    SCADE KCG 型で、`certification-grade.md:135` が既にこのアナロジーを書いている。
- **だから飛行成果物には wasm byte 束縛(Gap 1 = 今一番難しい未証明の本丸、
  [v1-proof-architecture](v1-proof-architecture.md))を勝つ必要がない。** Ferrocene が
  オブジェクトコードの信頼を肩代わりし、必要なのは**証明の中で一番安い層(翻訳忠実性)
  だけ**。**証明コストとしては wasm 経路より飛行経路の方が安い。**

### 4.3 本当のコストは証明ではなくレンダラ(エンジニアリング) — **🔄 進行中、実質的な建て込み済み (2026-07-19 更新)**

**この節が書かれた時点(368行のデモ・CLI未接続)からは前進した。** `crates/almide-mir/src/render_rust.rs`
は今 **`src/cli/build.rs`(~66行目)/ `src/cli/run.rs`(~71行目)が `try_render_rust_source`
を直接呼ぶ形で CLI に配線済み** — 「デモ」ではなく `--verified` フラグの下で実行される
本番経路になった。[native-trust-spine](native-trust-spine.md) が **4本の出荷済み rung**
(commit 519b67df, 574af24b, 2780f429, e088c25d — 全て 2026-07-14/15)を記録している:
rung 1(scalar・String literal・制御フロー)、rung 2(動的 String 演算 shim)、rung 3(String
境界面の全面拡張)、rung 4(target-neutral `ListLit`/`ListGetScalar`/`ListSetScalar` op による
scalar list、wasm レンダラと共有 MIR op)。各 rung は `tests/native_v1_differential_test.rs`
の v0-differential ゲート(stdout/stderr/exit の byte 比較)付きで出荷 — **証明の心配だった
Rust 経路が、今は「まだ全部ではないが、ゲート付きで確実に育っている」実装**に変わった。

依然として旧来の**2本分裂**は残る:

- **v1 所有権保存レンダラ** = `render_rust.rs`(現 409 行、成長中)。上記4 rung 経由で
  scalar/String/List を CLI 配線・差分ゲート付きで生産カバー。**まだ全 MIR op を覆っていない**
  (WCET キーストーン(あ)未着手のループ形など)。
- **本番 Rust(v0/legacy)** = `crates/almide-codegen/src/walker/` + `codegen/templates/rust.toml`。
  イディオマティックで**レビュー可・DO-178C「ソースは可読」を満たす**が、**MIR-op ↔ Rust 片の
  対応オブジェクトが無く・v1 MIR で駆動されていない・未証明**のまま(v1 経路が育つまでの
  被覆先)。

→ pivot の残コスト = **v1 MIR→Rust レンダラのカバレッジを rung 5+ で伸ばし続ける**こと
(`render_wasm.rs` の全言語版・約5倍規模、という当初見積りに対し**4 rung ぶんは既に着地**)。
**同じ認証済み MIR witness が両ターゲットを駆動**する形は、scalar/String/List の範囲では
**既に実現している**。**まだ Ferrocene には束縛されていない** — `rust_pattern` 表や
Ferrocene 参照はリポジトリのどこにも見つからない(確認: `grep -rn "Ferrocene\|rust_pattern"
proofs/ crates/ | grep -v flight-` は無ヒット)。その最終区間は genuinely open のまま。

### 4.4 v1-mir-architecture §9 との和解

[v1-mir-architecture](v1-mir-architecture.md) §1 は「wasm = 主権ある検証経路、Rust =
ほぼ自明に下る踏み台」、§9 は「Rust を唯一の真にする案 = 形式仕様の無い Rust 意味論の
上に建てる → 却下」。**飛行プロファイルはこの却下と矛盾しない**:

> 飛行は **Rust 意味論の上に建てない**。認証済み **MIR 事実**に束縛し、Rust は依然
> ただのレンダラ、Rust→機械語の信頼は Ferrocene が肩代わりする。MIR は唯一の真の
> まま。変わるのは「Rust レンダラを**証明束縛された2人目の本番ターゲット**に格上げ
> する」ことだけ ── 主権は MIR に残る。

§9 が却下したのは「Rust を**真**にする」案。本プロファイルは「Rust を**証明束縛された
レンダラ**にする」案で、別物。

→ **具体実装設計(rust_pattern 表・忠実性定理・信頼基底台帳・Ferrocene 実証経路)**: [flight-rust-ferrocene](flight-rust-ferrocene.md)

---

## 5. 合流 — 一貫した飛行スタック

```
Almide 飛行サブセット            counted loop / no-alloc-in-loop / static pool  ← (あ)§3.5
   ↓  lower + checker
認証済み MIR(唯一の真)          既存4性質 + 新: bounded-alloc / counted-loop    ← (あ)§3.3
   ↓  render_rust(本番版を新設)                                                 ← (い)§4.3
可読 Rust                        rust_pattern 忠実性(eager instance 既証明)     ← (い)§4.2
   ↓  Ferrocene(資格化済み)      Rust→機械語の信頼を肩代わり = Gap 1 迂回         ← (い)§4.2
飛行 HW で走る機械語 + WCET 有界
```

各層の信頼が会計されている。**戦略的解放**: 健全性の核がターゲット非依存だから、
**wasm 主権(① エージェント市場)と Rust 飛行を二者択一にしなくていい** ── 両方が
**同じ認証済み MIR witness の別コンシューマ**になる。決定は「どっちを捨てるか」では
なく「**同じ witness に Rust 飛行レンダラを2人目の消費者として足すか**」。

---

## 6. 2つのキーストーン

| | キーストーン | 種類 | 解錠するもの | 前提 |
|---|---|---|---|---|
| **(あ) WCET** | ループを Coq `Op` 型に持ち上げる + counted-loop 上限 | 証明拡張 | 停止性 / no-alloc-in-loop / 確保 count 上限 ── 飛行性質3つ同時 | なし(Rust 決定に非依存) |
| **(い) Ferrocene** | 本番 v1 MIR→Rust レンダラ + `rust_pattern` 忠実性層 | エンジニアリング(証明は安い) | Gap 1 迂回 / 飛行成果物 / Rust 市場 | §4.4 の方針確定 |

**先後**: 両者は独立。順序をつけるなら **(あ)のループ持ち上げが先** ── 飛行サブセットの
「形」を確定し、停止性という単独で価値ある証明も解錠する。**(い)は §4.4 の戦略決定
(wasm 主権と Rust 飛行の和解)を先に刻んでから本番レンダラ着手。**

---

## 7. Flight-Readiness Ladder ── 🔴→近い の exit criteria

[v1-kgi-kpi](v1-kgi-kpi.md) / GTM の A・B層ステータスは現在 🔴(まだ無理)。これを
**近い** に flip するための gate を順に定義する。「flight-grade 達成」(柱①〜⑥の全充足・
当局受理)の手前に、**営業が成立する技術的前提が揃った状態**という中間ゴールを置く。

### 7.1 「近い」の定義(誇張しない線)

> **近い** = A・B層の認証・安全技術者に「資格化可能な実装基板」として admissible で、
> **パイロット/提携が現実的**な技術的前提が揃った状態。
> **≠** 実機の完全認証・当局(FAA/EASA)受理・量産飛行。それらは顧客側の多年工程で、
> 本ラダーの外(§8)。

### 7.2 Gate(🔴→近い、順序つき)

| Gate | 中身 | なぜ信頼性を gate するか | 紐付く設計 |
|---|---|---|---|
| **G-F0 実行パリティ** | 実プログラムが端まで走り v0/oracle と byte 一致(Gap 3 完遂) | 走らない言語は門前払い。全ての床 | self-host runtime(進行中) |
| **G-F1 飛行プロファイル定義+強制** | 飛行サブセット(counted loop / no-alloc-in-loop / bounded static alloc)を仕様化、checker が境界を壁として強制 | 安全技術者に見せる「形」が無いと話が始まらない | §3 キーストーン(あ)前半 |
| **G-F2 WCET 有界の証明化** | ループを Coq に持ち上げ + counted-loop 上限 + 総確保上限を証明済み性質に | 「WCET by construction」は A・B層の table-stakes | §3.3 キーストーン(あ)本体 |
| **G-F3 飛行ターゲット決定+忠実性束縛** | Rust→Ferrocene を飛行ターゲットと決定、本番 MIR→Rust + `rust_pattern` 忠実性を飛行サブセット上で証明、Ferrocene コンパイル実証 | オブジェクトコード信頼(柱②④)の着地が無いと机上 | §4 キーストーン(い) |
| **G-F4 リファレンスアプリが make verify を端まで通る** | 安全臨界形の小モジュール(制御則カーネル/状態機械/watchdog)を飛行プロファイルで: 走る→oracle 一致→証明書発行→可読 Rust→Ferrocene | 認証技術者が前のめりになる**弾**。スライドを成果物に変える | `make verify` キラーデモ([v1-kgi-kpi](v1-kgi-kpi.md)) |
| **G-F5 証明書↔認証目標マッピング(ドシエの種)** | 証明書/receipt が DO-178C 目標 / DO-330 ツール資格論証 / DO-333 形式手法クレジットのどれを覆うかをドラフト文書化、DER がレビュー可能な形 | 「cert は後で」では「資格化可能」と言えない | [certification-grade](certification-grade.md) CG 領域 |
| **G-F6 資格化キットの製品定義(AbsInt モデル)** | 売り物=キットの中身(資格化済み checker・検証ハーネス・トレーサビリティ・ドシエテンプレ)をスコープ。フルビルド不要、定義でよい | 売る対象が無いと「近い」は営業にならない | GTM([v1-kgi-kpi](v1-kgi-kpi.md)) |

**G-F0..G-F6 が揃った時点で A・B層ステータスを 🔴→近い に flip。**

### 7.3 現在地(正直なマーカー)

- **G-F0**: 🔄 進行中 ── self-host stdlib で実行パリティを登攀中(string/list/math を Almide 化)
- **G-F1**: 📐 規範仕様あり・未強制 ── [flight-subset-spec](flight-subset-spec.md)(特徴分類・open question 決着・`@flight` 強制設計)
- **G-F2**: 📐 設計済み・未実装 ── 本書 §3 のキーストーン(あ)が**最大の技術的山**([flight-wcet-loops](flight-wcet-loops.md))
- **G-F3**: 🔄 **進行中、実質的な建て込みあり (2026-07-19 更新)** ── キーストーン(い)の
  「本番 v1 MIR→Rust レンダラ」半分は [native-trust-spine](native-trust-spine.md) の
  4 rung(scalar/String/List、CLI 配線済み、v0 差分ゲート付き)で前進済み。**残る山** =
  「`rust_pattern` 忠実性層」と「Ferrocene コンパイル実証」── この2つはまだ 📐 設計済み・
  未着手のまま([flight-rust-ferrocene](flight-rust-ferrocene.md))。G-F3 全体としては
  📐→🔄 に上がったが完了ではない
- **G-F4**: 📐 設計済み・未実装 ── [flight-reference-app](flight-reference-app.md)(PID 制御則カーネル + make verify 7 段 + receipt。Slice 0 は今 green)
- **G-F5/G-F6**: 📐 設計済み・未着手 ── [flight-qualification](flight-qualification.md)(DO-178C/330/333 マッピング + 資格化キット)

→ ラダー上、**まだ G-F0 の途中**(全 gate に設計はついた)。クリティカルパス = G-F0 完遂 → キーストーン(あ)(い) → リファレンスアプリ → ドシエ種 + キット定義。**全 gate が「未着手」から「設計済み・未実装」に上がった**(7 gate 全てに具体設計文書)。

### 7.4 「近い」に**不要**(誇張防止・後回し)

§8 と重複するがラダーの線引きを明示する。以下は「近い」に**積まない**(積むと過剰建設):

- 実機の完全 DO-178C 認証(顧客側・多年)
- 当局による「PCC = 資格化代替」論証の受理(前例ゼロ・規制)
- **全言語**の MC/DC 構造被覆(飛行サブセットの DO-333 形式手法クレジットで代替)
- **全言語**の WCET(飛行サブセットのみ要する)
- service history / 飛行実績

---

## 8. この文書が変えないもの(正直な範囲)

工学で閉じられるのは柱②④の一部だけ。**以下は依然 flight-grade に遠く、本プロファイル
の対象外**:

- **柱③ 機能要求トレーサビリティ + MC/DC** ── 安全証明では代替不能(アプリの責務 +
  追跡・被覆の機構が必要)。[certification-grade](certification-grade.md) CG 領域。
- **柱⑤ 認証ドシエ + DO-330 目標マッピング** ── 認証形式の成果物が未生成。
- **柱⑥ 当局受理 + service history** ── 「PCC で資格化を代替」は前例ゼロ・多年の規制作業。
  飛行実績ゼロ。

→ だから [v1-kgi-kpi](v1-kgi-kpi.md) / GTM の通り**航空は北極星・信用の錨であって
近接の売り物ではない**が正しい。① エージェント・② CRA 市場は本プロファイルの
キーストーンを**閉じずに**解錠する。本プロファイルは「航空に**本気で近づく**ための
工学的最短路2本」を定義するもので、「航空を取る」工程全体ではない。

---

## 9. 関連

- [flight-subset-spec](flight-subset-spec.md) ── **G-F1 規範仕様**(飛行サブセットの特徴 IN/OUT・open question 決着・`@flight` 強制)
- [flight-wcet-loops](flight-wcet-loops.md) ── **キーストーン(あ) G-F2 の具体実装設計**(counted loop + Coq 持ち上げ + 確保上限)
- [flight-rust-ferrocene](flight-rust-ferrocene.md) ── **キーストーン(い) G-F3 の具体実装設計**(本番 MIR→Rust + rust_pattern 忠実性)
- [flight-reference-app](flight-reference-app.md) ── **G-F4**(PID 制御則カーネル + make verify 7 段 + receipt)
- [flight-qualification](flight-qualification.md) ── **G-F5/G-F6**(DO-178C/330/333 マッピング + 資格化キット)
- [v1-proof-architecture](v1-proof-architecture.md) ── PCC スパインの着地形、Gap 1/2
- [certificate-format-v1](certificate-format-v1.md) ── i/a/d/m/r/b 所有権アルファベット
- [v1-mir-architecture](v1-mir-architecture.md) ── §1 wasm 主権・§9 却下案(§4.4 で和解)
- [certification-grade](certification-grade.md) ── 規格5メカニズム・柱③⑤の領域
- [v1-kgi-kpi](v1-kgi-kpi.md) ── 性質被覆 4/8・Gap 台帳・GTM ラダー
- [trust-layer](trust-layer.md) ── L0-L4、航空 = 北極星
