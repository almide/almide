<!-- description: Proposed qualification-evidence mapping and kit design; no accepted regulatory credit or qualified tool established by this document -->
# Flight Qualification — DO-178C/DO-330/DO-333 マッピング + 資格化キット(G-F5 + G-F6)

> **Goal**: [flight-profile](flight-profile.md) ラダー **G-F5**(PCC 証明書/receipt を認証目標へ
> マッピング)と **G-F6**(資格化キットの製品定義、AbsInt/SCADE-KCG モデル)の具体設計。
> 今スパインが**実際に産出するもの**に接地する。
> **関連**: [flight-profile](flight-profile.md) §7.2 / [certification-grade](certification-grade.md)(規格5メカニズム・ALS・evidence ladder)/ [receipt-logic](receipt-logic.md)(主張分類)/ [certificate-format-v1](certificate-format-v1.md)/ [flight-evidence-gaps](flight-evidence-gaps.md)(実地監査所見 — 資格化主張の現在の限界)。

---

## 0. 元の設計時点の技術資産スナップショット

以下の固定件数・版は元の計画値であり、現在の在庫ではない。現行の証明・測定・
trusted boundary は `proofs/receipt.sh` と生成済み `docs/TRUST-SPINE.md`、
ALS は [規範仕様](../../specs/als/README.md) を参照。技術資産の存在と
当局に受理された資格化は別であり、この文書は後者を示さない。

- Coq **16 ファイル / 45 定理**、全 `Print Assumptions` = "Closed under the global context"、
  `coqchk` De-Bruijn 再検査、cross-version 9.1.1 + 9.2(`proofs/check.sh`, `TRUSTED_BASE.md`)。
- 資格化対象 = `proofs/checker.ml`(現在 1,144 行 OCaml 抽出 — 実数は `scripts/gen-claims.sh` が docs/TRUST-SPINE.md に再生成)← `OwnershipChecker.v`(309)+
  `Subset.v`(88)+ `Extract.v`。核 = 符号付き Δ 左 fold + `subset_check` 包含。
- `make verify-trust` = `check.sh` + `gate.sh` + `corpus-wall.sh` + `cargo test -p almide-mir`。
  `make receipt` = `receipt.sh`。CI が両方実行(`.github/workflows/trust-spine.yml`)。
- 性質被覆 **4/8**(mem/name/cap/type ✅、leak/reuse/call-mode/byte 残)。in-profile 4083/4195。
- receipt 主張 = C-PROVEN / C-SAFE / C-FAITHFUL / C-WALL / C-REPRO(`receipt.sh`, [receipt-logic](receipt-logic.md))。

---

## 1. G-F5.1 — DO-178C Table A 目標マッピング

以下は G-F1 サブセットについて検討した**未受理の対応案**であり、DAL-A credit の
取得や規格解釈の正しさを確定する表ではない。証明書/receipt は
**source-coding & integration + verification-of-verification 行**の検証プロセス証拠であって、要求/
機能正しさの証拠ではない。

### 専門レビューに付す対応候補

| Table | 目標(意訳) | Almide 証拠 | 強度 |
|---|---|---|---|
| **A-5 obj 6** | source code が正確・一貫(stack/memory 管理・無界使用なし) | `check_all_sound`(RC 均衡→no double-free/leak)、`balanced_cert_frees_in_memory`、`StackBalance`、`FreeList.alloc_not_live`、`CowSafety`。receipt **C-SAFE/C-PROVEN** | 強・形式・毎ビルド |
| **A-5 obj 1-2** | source が LLR に適合(部分:ALS が固定する memory/ownership LLR) | `TypeConcretization`・`NameTotality`。ALS(`certification-grade.md` CG-1)が LLR を供給 | 部分(言語安全 LLR のみ、アプリ機能 LLR は不可) |
| **A-2 obj 5** | source code 開発(証明書 = MIR 束縛の開発成果物) | i/a/d/m/r/b witness + `Translation.v` op→wasm 表 | 補助 |
| **A-6(統合)部分** | EOC が robust / target で runtime error なし | `WasmExec`(実バイトが `rt_inc/dec` 実行、double-free trap)+ sentinel。**wasm target のみ** | 部分・target 注釈付き(Ferrocene pivot が flyable target に移す) |
| **A-7 obj 3-4** | **検証の検証** ── 手順/結果が正しく検証自体が健全 | **最強の主張**:soundness 定理 = 検証手法の検証。`coqchk` 独立再検査 + 公理監査 + claim-drift gate | 強・Almide の本拠 |
| **A-7 obj 1-2** | HLR/LLR テスト被覆 | 形式証拠を対応させる案（credit/代替の可否は未確定、機能 LLR は対象外） | DO-333 経由のみ |

### 触れない目標(正直なギャップ・DER に明言必須)

| 目標 | 理由 | 誰の仕事 |
|---|---|---|
| A-3/A-4 HLR&LLR 正しさ・完全・トレーサビリティ | スパインは安全性質を証明、「正しい制御則を計算」は別。柱③=ゼロ | 顧客のドメイン工程 |
| A-7 obj 5-8 MC/DC・構造被覆 | 被覆は*checker の*性質、アプリ判定論理でない | 顧客(or サブセットの DO-333 credit) |
| A-4 obj 13 WCET | `Termination.v` は loop-free、ループ no-op。WCET=キーストーン(あ)、未 | open キーストーン |
| A-6 完全 EOC/target テスト | wasm は証明済みだが「飛行機は wasm で飛ばない」 | Ferrocene pivot(G-F3) |
| A-1 PSAC/計画適合 | 認証計画がまだ無い | G-F6 ドシエテンプレ |

**外部説明の境界**:「Almide は、明示したモデル上の検査器定理と fixture の
再検証結果を提供する。これらをどの認証目標の証拠として採用できるかは、
用途・構成・未証明部分を含めた専門評価が必要であり、ここでは受理済みと主張しない。」

---

## 2. G-F5.2 — DO-330 ツール資格化論証(「checker を証明する」)

**提案**: コンパイラ出力を独立に検査する構成の証拠を整理し、何を資格化対象と
すべきか評価に付す。検査器の定理があることだけで compiler を TQL-5 に降格したり、
checker を「形式証明により資格化済み」としたりはできない。この repo はその
当局受理を示していない。具体的な TQL/Criteria/credit の判断は未確定として扱う。

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
  checker の資格化済み状態も renderer の TQL もここからは確定しない。
  Rust/Ferrocene 方針は Almide→Rust の忠実性を自動的に証明しない。
  `.github/workflows/ferrocene-lane.yml` は現在、契約待ちの stock rustc 使用を明記している。
- **抽出が信頼**:証明は Coq 項上、走る checker は `ocamlopt` 経由 OCaml(Thompson 穴)。
  CertiCoq+CompCert で閉じる(未)。
- **検査実装の構成**: 独立性・共通原因故障・版管理を評価資料に含める。単一の規格一般論で十分性を決めない。
- **受理の証拠なし**: このリポジトリは Almide の当局受理を示していない。
  世界全体の前例の不存在や審査所要年数を、この資料だけから断定しない。
- **ALS 妥当性は経験的・never proven**:「形式意味論が意図を捉えるか」が irreducible floor。

---

## 3. G-F5.3 — DO-333 形式手法 credit マッピング

以下は、健全性・実装との対応・要求との対応を検討するための元の作業表である。
DO-333 credit の受理、検証活動の代替可否、規格条項の充足は未確定であり、
各行に定理があるだけで自動的に成立するものではない。
| 対応を検討する DO-178C 目標案 | 証明性質(ファイル) | J1 健全 | J2 ツール正しさ | J3 性質が要求を捉える(弱点) |
|---|---|---|---|---|
| A-5 no double-free/leak | `check_all_sound`/`balanced_cert_frees_in_memory` | `accept ⟹ no_double_free ∧ no_leak`、axiom-clean | `coqchk`+`Print Assumptions`+drift gate | RC 均衡 = memory-safety LLR。✓ サブセット上 |
| A-5 no dangling reference | `check_names_cert_sound` | 証明済み subset law | 同上 | name-totality = 使用名は全て定義 |
| A-5 bounded resource/no undeclared effect | `check_caps_cert_sound`(`reachable⊆declared`、transitive) | 証明済み | 同上 | **正直な scope:Stdout のみ**。他 host effect 未モデル ── 開示必須 |
| A-5 type soundness | `TypeConcretization` | 証明済み | 同上 | LLR = Unknown 型が codegen に到達しない |
| A-6 EOC robustness(wasm) | `WasmExec`/`rc_dec_bytes_trap_on_zero` | 実バイト上で証明(wat2wasm grounded) | wat2wasm + wasmtime 差分 | rc 安全 op のみ、機能 list-op は bootstrap runtime で未証明 |

形式証拠は **memory-safety/name/type/capability** の明示したモデル範囲で提出候補になる。
それを認証 credit として受理できるかは別の判断である。**不可**:機能正しさ(柱③、制御則の形式仕様なし)、WCET(`Termination.v` でループ no-op、
キーストーン(あ)要)、value-semantics サブセット外(C-WALL が境界:in-profile 外は明示 `Unsupported`、
silent 誤 credit しない)。DO-333 の J3 規律は receipt の per-claim「scope(honest)」列そのもの ──
receipt は技術的な主張・根拠・scope の表であり、DO-333 適合性を自動的に成立させるものではない。

---

## 4. G-F6 — 資格化キット(製品内容:EXIST vs BUILD)

AbsInt/SCADE-KCG モデル:**箱でなくキットを売る。** キット = 顧客のツール資格化コストを潰す入力を
package + version + 署名したもの。

| # | キット成果物 | 状態 | 在処 / 欠 |
|---|---|---|---|
| 1 | checker 技術資産（資格化済みの意味ではない） | 技術資産あり | `checker.ml` + `OwnershipChecker.v`/`Subset.v`/`Extract.v` + `build-checker.sh` |
| 2 | 証明スパイン(16 `.v`/45 定理)+ 公理監査 + coqchk 再現 | ✅ EXISTS | `proofs/*.v`, `check.sh` |
| 3 | 検証ハーネス(1 コマンドで全再導出) | ✅ EXISTS | `make verify-trust`、CI mirror |
| 4 | receipt(schema + sample)= 型付き主張バンドル | ✅ EXISTS | `make receipt`→`receipt.sh`、[receipt-logic](receipt-logic.md) |
| 5 | 信頼基底台帳(toolchain pin + irreducible floor + 既知制限) | ✅ EXISTS | `TRUSTED_BASE.md` |
| 6 | 証明書形式仕様(i/a/d/m/r/b + side-table + サイズ不変条件) | ✅ EXISTS | `certificate-format-v1.md` |
| 7 | corpus-wall 証拠(totality + accept⟹safe over 実コーパス) | ✅ EXISTS | `corpus-wall.sh`、C-WALL |
| 8 | ALS 規範仕様 | 仕様あり、資格化受理とは別 | [ALS](../../specs/als/README.md)、実装対応と scope は各章 |
| 9 | トレーサビリティ行列(ALS § ↔ C-NNN ↔ fixture) | ◑ PARTIAL | `contracts.toml` 双方向 gate 在、`spec="ALS §"` keying は CG-1 |
| 10 | 飛行プロファイル仕様(counted-loop/no-alloc/bounded、checker 強制) | 📐 DESIGNED | [flight-subset-spec](flight-subset-spec.md)。キーストーン(あ)未 |
| 11 | WCET-bound 証拠(ループ Coq 持ち上げ・alloc-count 天井) | ⬜ BUILD | キーストーン(あ)G-F2 |
| 12 | ネイティブ翻訳・ツールチェーンの証拠 | renderer は実在、全翻訳忠実性と資格化は未確立 | `render_native.rs`、stock-rustc の Ferrocene lane；G-F3 の課題と分離 |
| 13 | ドシエテンプレ(PSAC/SVP/SAS 骨格 → G-F5 目標表へ写像) | ⬜ BUILD | §1 のマッピングが種、`certification-grade.md` CG-5 |

**要約**:技術核(1-7)は**今 EXISTS**で CI 強制。認証向け packaging(8-13)が build-out ──
特に**13 ドシエテンプレは §1 の G-F5 マッピングの直接の産物**。G-F6 は「定義のみ」で 🔴→近い に flip 可
(フルビルド不要)。

---

## 5. キット境界 + 顧客統合

### 境界(AbsInt/SCADE-KCG アナロジー)

この製品境界は原設計の類推であり、SCADE/AbsInt の資格化条件や source review
免除を本書が確認したものではない。Almide に同じ扱いが認められるとは推論しない。

| 評価へ提出する技術資産の案 | 別途判断・検証を要する範囲 |
|---|---|
| checker の健全性証明と対象 witness/model の説明 | ツール資格化、適用 TQL、意図した用途での受理は未確定 |
| mem/name/caps/type の scoped receipt と declared subset | source→artifact の対応、未証明 runtime/host、アプリの機能要求 |
| 検証ハーネス、traceability、ドシエのテンプレート案 | 実際の対象構成・証拠の充足性と顧客/専門家の評価 |
| WCET に関するモデル/測定案（完成を主張しない） | 実機・環境の前提、system-level timing/safety case |

キットは証拠を整理して評価を助けるための案であり、生成コードの安全を無償で
保証したり、proof だけで checker を資格化したり、コンパイラを自動的に TQL-5
へ変更するものではない。当局受理と世界的前例の有無を本書では確定しない。

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
