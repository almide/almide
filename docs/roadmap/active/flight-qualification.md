<!-- description: Flight gates G-F5 + G-F6 — how the PCC certificate/receipt maps to DO-178C Table A objectives, the DO-330 "prove-the-checker" tool-qualification argument (compiler→TQL-5 output-verified, checker→qualified-by-proof), the DO-333 formal-methods credit per proven property, and the G-F6 qualification kit as a product (AbsInt/SCADE-KCG model): what's in the kit, the boundary (kit provides vs customer's domain process), and the customer integration story. -->
# Flight Qualification — DO-178C/DO-330/DO-333 マッピング + 資格化キット(G-F5 + G-F6)

> **Goal**: [flight-profile](flight-profile.md) ラダー **G-F5**(PCC 証明書/receipt を認証目標へ
> マッピング)と **G-F6**(資格化キットの製品定義、AbsInt/SCADE-KCG モデル)の具体設計。
> 今スパインが**実際に産出するもの**に接地する。
> **関連**: [flight-profile](flight-profile.md) §7.2 / [certification-grade](certification-grade.md)(規格5メカニズム・ALS・evidence ladder)/ [receipt-logic](receipt-logic.md)(主張分類)/ [certificate-format-v1](certificate-format-v1.md)/ [flight-evidence-gaps](flight-evidence-gaps.md)(実地監査所見 — 資格化主張の現在の限界)。

---

## 0. 接地事実(今スパインが産出するもの)

- Coq **16 ファイル / 45 定理**、全 `Print Assumptions` = "Closed under the global context"、
  `coqchk` De-Bruijn 再検査、cross-version 9.1.1 + 9.2(`proofs/check.sh`, `TRUSTED_BASE.md`)。
- 資格化対象 = `proofs/checker.ml`(**683 行 OCaml 抽出**)← `OwnershipChecker.v`(309)+
  `Subset.v`(88)+ `Extract.v`。核 = 符号付き Δ 左 fold + `subset_check` 包含。
- `make verify-trust` = `check.sh` + `gate.sh` + `corpus-wall.sh` + `cargo test -p almide-mir`。
  `make receipt` = `receipt.sh`。CI が両方実行(`.github/workflows/trust-spine.yml`)。
- 性質被覆 **4/8**(mem/name/cap/type ✅、leak/reuse/call-mode/byte 残)。in-profile 4083/4195。
- receipt 主張 = C-PROVEN / C-SAFE / C-FAITHFUL / C-WALL / C-REPRO(`receipt.sh`, [receipt-logic](receipt-logic.md))。

---

## 1. G-F5.1 — DO-178C Table A 目標マッピング

DO-178C credit は**飛行サブセット(G-F1)に対する DAL-A のみ**で主張。正直な枠 ── 証明書/receipt は
**source-coding & integration + verification-of-verification 行**の検証プロセス証拠であって、要求/
機能正しさの証拠ではない。

### 主張できる目標

| Table | 目標(意訳) | Almide 証拠 | 強度 |
|---|---|---|---|
| **A-5 obj 6** | source code が正確・一貫(stack/memory 管理・無界使用なし) | `check_all_sound`(RC 均衡→no double-free/leak)、`balanced_cert_frees_in_memory`、`StackBalance`、`FreeList.alloc_not_live`、`CowSafety`。receipt **C-SAFE/C-PROVEN** | 強・形式・毎ビルド |
| **A-5 obj 1-2** | source が LLR に適合(部分:ALS が固定する memory/ownership LLR) | `TypeConcretization`・`NameTotality`。ALS(`certification-grade.md` CG-1)が LLR を供給 | 部分(言語安全 LLR のみ、アプリ機能 LLR は不可) |
| **A-2 obj 5** | source code 開発(証明書 = MIR 束縛の開発成果物) | i/a/d/m/r/b witness + `Translation.v` op→wasm 表 | 補助 |
| **A-6(統合)部分** | EOC が robust / target で runtime error なし | `WasmExec`(実バイトが `rt_inc/dec` 実行、double-free trap)+ sentinel。**wasm target のみ** | 部分・target 注釈付き(Ferrocene pivot が flyable target に移す) |
| **A-7 obj 3-4** | **検証の検証** ── 手順/結果が正しく検証自体が健全 | **最強の主張**:soundness 定理 = 検証手法の検証。`coqchk` 独立再検査 + 公理監査 + claim-drift gate | 強・Almide の本拠 |
| **A-7 obj 1-2** | HLR/LLR テスト被覆 | **DO-333 形式 credit で代替**(安全性質のみ。機能 LLR は不可) | DO-333 経由のみ |

### 触れない目標(正直なギャップ・DER に明言必須)

| 目標 | 理由 | 誰の仕事 |
|---|---|---|
| A-3/A-4 HLR&LLR 正しさ・完全・トレーサビリティ | スパインは安全性質を証明、「正しい制御則を計算」は別。柱③=ゼロ | 顧客のドメイン工程 |
| A-7 obj 5-8 MC/DC・構造被覆 | 被覆は*checker の*性質、アプリ判定論理でない | 顧客(or サブセットの DO-333 credit) |
| A-4 obj 13 WCET | `Termination.v` は loop-free、ループ no-op。WCET=キーストーン(あ)、未 | open キーストーン |
| A-6 完全 EOC/target テスト | wasm は証明済みだが「飛行機は wasm で飛ばない」 | Ferrocene pivot(G-F3) |
| A-1 PSAC/計画適合 | 認証計画がまだ無い | G-F6 ドシエテンプレ |

**DER 向け一文**:*「Almide 証明書は飛行サブセットの A-5 source 適合と A-7 検証の検証行の
robustness/accuracy 証拠で、毎ビルド再導出可能。A-3/A-4 機能要求やアプリ MC/DC には一切主張しない。」*

---

## 2. G-F5.2 — DO-330 ツール資格化論証(「checker を証明する」)

**ツールと TQL**:DO-330 で、出力が独立検証されないコード生成器は **Criteria-1 / TQL-1**(誤りを
airborne software に注入し得る)── 最も過酷。

**Almide の手 = PCC 非対称性**:10万行のコンパイラを資格化せず、**出力を毎ビルド独立再検証**する
小さな kernel-proven checker を置く。これが資格化対象を**降格**:

- **未信頼コンパイラ**(`almide-mir`+renderer)→ **出力が検証される TQL-5/Criteria-3 ツール**。
  DO-178C §6.0 + DO-330 が明示的に許す:ツールの出力が別プロセスで検証されるなら資格化レベルが
  下がる。ここでの verifier = 証明済み checker。
- **checker**(`checker.ml` 683 行 ← `OwnershipChecker.v` 309 + `Subset.v` 88)= 残る資格化対象 ──
  ただし**テストでなく形式証明**で資格化(DO-330 + DO-333 結合)。

**資格化証拠パッケージ(全て実在、`proofs/`)**:
1. checker は小さく監査可能。核 = 符号付き Δ 左 fold + `subset_check`。サイズ ∝ #event 文字 +
   #subset + #表エントリ、**プログラム/言語/コンパイラ規模に非依存**(`certificate-format-v1.md` の
   tripwire:checker が callee を開く/CFG walk/推論を禁止)。
2. checker の soundness が機械証明(`check=accept ⟹ P`):45 定理、`Qed`、0 `Admitted`。
3. **独立再検査** `coqchk`(De Bruijn 基準・第2の小 kernel)。
4. **公理監査** `Print Assumptions` = closed、claim-drift gate で公開台帳が `coqchk` 検証を超えてドリフト不可。
5. **毎ビルド再導出** `make verify-trust` を顧客マシンで。**CI は信頼基底に無い**(「我々の CI を信じよ」でなく「顧客が再導出可」)。
6. **cross-version 再導出**(9.1.1 + 9.2)= toolchain 版への頑健性。

**DER がなお challenge する点(先回りで明言)**:
- **witness ⟹ emitted-bytes の縫い目は信頼で未証明**。checker は MIR-level witness を検証、emitted
  wasm/Rust バイトがそれを実現するのは §3 renderer 契約(`Translation.v` は presence 表照合)。
  Gap 1 / 柱②=半分。**「checker は資格化したが、accepted MIR と飛ぶビットの間の renderer はなお
  TQL-1」**。Ferrocene pivot がこれに答える(Rust→機械語を Ferrocene の資格化責任にし、残りを安い
  翻訳忠実層に縮小)。
- **抽出が信頼**:証明は Coq 項上、走る checker は `ocamlopt` 経由 OCaml(Thompson 穴)。
  CertiCoq+CompCert で閉じる(未)。
- **単一 checker**:DO-330 は多様性を好む、今 checker は1つ。
- **前例ゼロ**:「checker 証明 = ツール資格化」を受理した当局はまだ無い。DER エンゲージメント
  (SOI レビュー)自体が多年作業。
- **ALS 妥当性は経験的・never proven**:「形式意味論が意図を捉えるか」が irreducible floor。

---

## 3. G-F5.3 — DO-333 形式手法 credit マッピング

DO-333 は**形式解析が特定検証目標のテストを代替**するのを許す ── 3 つの正当化:(J1)手法が健全
(偽性質を真と主張しない)、(J2)実装ツールが正しい、(J3)形式化性質が要求を捉える。

| 代替される DO-178C 目標 | 証明性質(ファイル) | J1 健全 | J2 ツール正しさ | J3 性質が要求を捉える(弱点) |
|---|---|---|---|---|
| A-5 no double-free/leak | `check_all_sound`/`balanced_cert_frees_in_memory` | `accept ⟹ no_double_free ∧ no_leak`、axiom-clean | `coqchk`+`Print Assumptions`+drift gate | RC 均衡 = memory-safety LLR。✓ サブセット上 |
| A-5 no dangling reference | `check_names_cert_sound` | 証明済み subset law | 同上 | name-totality = 使用名は全て定義 |
| A-5 bounded resource/no undeclared effect | `check_caps_cert_sound`(`reachable⊆declared`、transitive) | 証明済み | 同上 | **正直な scope:Stdout のみ**。他 host effect 未モデル ── 開示必須 |
| A-5 type soundness | `TypeConcretization` | 証明済み | 同上 | LLR = Unknown 型が codegen に到達しない |
| A-6 EOC robustness(wasm) | `WasmExec`/`rc_dec_bytes_trap_on_zero` | 実バイト上で証明(wat2wasm grounded) | wat2wasm + wasmtime 差分 | rc 安全 op のみ、機能 list-op は bootstrap runtime で未証明 |

形式 credit は飛行サブセットの **memory-safety/name/type/capability LLR** に主張可、**検証の検証**も
形式。**不可**:機能正しさ(柱③、制御則の形式仕様なし)、WCET(`Termination.v` でループ no-op、
キーストーン(あ)要)、value-semantics サブセット外(C-WALL が境界:in-profile 外は明示 `Unsupported`、
silent 誤 credit しない)。DO-333 の J3 規律は receipt の per-claim「scope(honest)」列そのもの ──
**receipt は構造的に DO-333 手法正当化表**。

---

## 4. G-F6 — 資格化キット(製品内容:EXIST vs BUILD)

AbsInt/SCADE-KCG モデル:**箱でなくキットを売る。** キット = 顧客のツール資格化コストを潰す入力を
package + version + 署名したもの。

| # | キット成果物 | 状態 | 在処 / 欠 |
|---|---|---|---|
| 1 | 資格化済み checker(binary + Coq source + 抽出レシピ) | ✅ EXISTS | `checker.ml` + `OwnershipChecker.v`/`Subset.v`/`Extract.v` + `build-checker.sh` |
| 2 | 証明スパイン(16 `.v`/45 定理)+ 公理監査 + coqchk 再現 | ✅ EXISTS | `proofs/*.v`, `check.sh` |
| 3 | 検証ハーネス(1 コマンドで全再導出) | ✅ EXISTS | `make verify-trust`、CI mirror |
| 4 | receipt(schema + sample)= 型付き主張バンドル | ✅ EXISTS | `make receipt`→`receipt.sh`、[receipt-logic](receipt-logic.md) |
| 5 | 信頼基底台帳(toolchain pin + irreducible floor + 既知制限) | ✅ EXISTS | `TRUSTED_BASE.md` |
| 6 | 証明書形式仕様(i/a/d/m/r/b + side-table + サイズ不変条件) | ✅ EXISTS | `certificate-format-v1.md` |
| 7 | corpus-wall 証拠(totality + accept⟹safe over 実コーパス) | ✅ EXISTS | `corpus-wall.sh`、C-WALL |
| 8 | ALS(Almide 言語仕様 = 性質を述べる規範意味論) | ◑ PARTIAL(CG-1) | `certification-grade.md` CG-1、interp-as-normative 進行中 |
| 9 | トレーサビリティ行列(ALS § ↔ C-NNN ↔ fixture) | ◑ PARTIAL | `contracts.toml` 双方向 gate 在、`spec="ALS §"` keying は CG-1 |
| 10 | 飛行プロファイル仕様(counted-loop/no-alloc/bounded、checker 強制) | 📐 DESIGNED | [flight-subset-spec](flight-subset-spec.md)。キーストーン(あ)未 |
| 11 | WCET-bound 証拠(ループ Coq 持ち上げ・alloc-count 天井) | ⬜ BUILD | キーストーン(あ)G-F2 |
| 12 | 飛行ターゲット束縛(MIR→Rust 本番 renderer + rust_pattern 忠実 → Ferrocene) | ⬜ BUILD(証明 ~75% 非依存済) | キーストーン(い)G-F3 |
| 13 | ドシエテンプレ(PSAC/SVP/SAS 骨格 → G-F5 目標表へ写像) | ⬜ BUILD | §1 のマッピングが種、`certification-grade.md` CG-5 |

**要約**:技術核(1-7)は**今 EXISTS**で CI 強制。認証向け packaging(8-13)が build-out ──
特に**13 ドシエテンプレは §1 の G-F5 マッピングの直接の産物**。G-F6 は「定義のみ」で 🔴→近い に flip 可
(フルビルド不要)。

---

## 5. キット境界 + 顧客統合

### 境界(AbsInt/SCADE-KCG アナロジー)

**SCADE KCG** = 資格化済みコード生成器を売り「生成コードは検証済みモデルを忠実に実装するので
生成コードの source review を省ける」。**AbsInt aiT** = 資格化済み WCET 解析器を売るが顧客が system
safety case を所有。Almide のキットは同カテゴリ。

| キットが PROVIDE(ベンダ責任) | 顧客のドメイン工程がなお行う |
|---|---|
| **ツール信頼** ── checker が proof で資格化、コンパイラが TQL-5(出力検証済)に降格 | アプリの箱の **DO-178C 認証**(PSAC〜SAS、DER、SOI) |
| **コード安全証拠** ── mem/name/caps/type、飛行サブセット、毎ビルド再導出(A-5/A-7 行) | **機能要求**(A-3/A-4):正しい制御則を計算 ── 柱③、キット外 |
| **毎ビルド再導出証明書 + receipt** ── C-SAFE/C-PROVEN/C-WALL | アプリ判定論理の **MC/DC**(DAL-A 主張時) |
| **WCET-bound 証拠**(キーストーン(あ)着地後)飛行サブセット | **system safety assessment**(FHA/PSSA/SSA, ARP4761) |
| **資格化ドシエテンプレ**(各成果物 → DO-178C/330/333 目標) | **当局受理**(「prove-the-checker」は前例ゼロ) |

**load-bearing 一文**:*キットは生成コードの安全をタダにしツールの信頼を安くする。顧客の
システムを正しく/認証済みにはしない。* SCADE-KCG split そのもの(`certification-grade.md`「何を
しないか」)。

### 顧客の使い方

1. **飛行プロファイル Almide を書く** ── 制御則カーネル/状態機械/watchdog を飛行サブセットで。
2. **`make verify` が毎ビルド証明書を再導出** ── 顧客マシンで(CI は信頼基底外)。in-profile 外は
   明示 `Unsupported` 壁、silent でない。
3. **飛行ターゲットへ render** ── MIR → 可読 Rust(`rust_pattern` 忠実)→ **Ferrocene**(ASIL-D/
   IEC 61508 資格化)で Rust→機械語、wasm byte 束縛 Gap 1 を迂回。
4. **receipt + キットのドシエを顧客の認証パッケージに畳む** ── PSAC が DO-330 論証(§2)と DO-333
   正当化(§3)を引用、receipt が A-5/A-7 検証結果証拠に、台帳が顧客のツール資格化信頼基底開示に。
   顧客は A-3/A-4・MC/DC・SSA を自工程で。

**正味**:Almide は**資格化キットベンダ**(Ferrocene/AdaCore/AbsInt 席)であって avionics 統合者でない。
価値 = **顧客の資格化作業開始前にドシエが存在し、資格化コストが下がる**こと ── Ferrocene FLS の手口を、
一度きりのコンパイラ資格化でなく毎ビルド PCC 証明書に適用したもの。
