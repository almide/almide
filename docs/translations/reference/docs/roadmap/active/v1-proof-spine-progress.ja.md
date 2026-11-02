<!-- description: v1 proof-spine の進捗 — task #31（全V / leak-freedom / 推移的caps / 抽出穴）の証明済み範囲と正直な残作業。CapabilityReach.v の推移的caps定理を記録（2026-06-21）。 -->
<a id="v1-proof-spine--31-progress-全v--leak-freedom--推移的caps--抽出穴"></a>

# v1 Proof Spine — #31 の進捗（全V / leak-freedom / 推移的caps / 抽出穴）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-proof-spine-progress.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](v1-proof-spine-progress.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](v1-proof-spine-progress.zh-CN.md)

> 非規範的な歴史参照資料。固定原本は変更せず正本として保持する。自動同期、新たな証明・ランタイム・測定・CI 実行はしていない。進捗層と旧実装の説明は履歴として読む。cert／モデル内の証明と範囲限定の実験的確認は、普遍的な成果物安全性や航空認証ではない。決定性 belt 原文は v0 type-state 境界を廃止済みと明記する。強い旧引用と安定 sort／入力順に関する主張も、新たな保証でなく歴史主張として保持する。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-proof-spine-progress.md)

spine は成熟している。`proofs/check.sh` は `coqc` で全定理を構築し、`coqchk` で**独立に再検証**する（De Bruijn 基準）。公理を監査し（全定理で `Print Assumptions` = *Closed under the global context*）、claim-drift gate が公開主張 ⊆ 証明済みを強制する。2026-06-21 に監査した task #31 の4つの不足の状態：

<a id="-leak-freedom--already-proven"></a>

## ✅ leak-freedom — すでに証明済み
`OwnershipChecker.check_sound : check ops = true -> no_double_free ops /\ no_leak ops`。
ここで `no_leak ops := run ops = Some 0`（取得した全参照が解放される）。抽出した end-to-end の `check_cert_sound` がこれを運び、`OwnershipLoop.v` が loop-carried slot へ拡張する（任意の反復回数で leak/double-free がない）。したがって cert レベルの kernel 基盤に leak-freedom がある。（自己ホストの再帰 drop **ルーチン**、`__drop_value`、`__vdrop_obj`、生成された `DropListStr`/`DropListListStr` loop は、cert が見る単一の釣り合った `d` を出す。その**内部**の要素別正しさは信頼する底層で、leak-loop により検証する。）

<a id="-抽出穴-extraction-hole--closed"></a>

## ✅ 抽出穴（extraction hole）— 解消
`checker.ml` は**証明された `check_cert` から抽出**される（Extract.v）。`gate.sh` はその抽出から checker を構築する。tokenizer は今や証明の**内部**にあり（`check_cert` が bytes の解析と検査を行う）、「cert bytes ⟶ accept/reject」全 pipeline が抽出された証明済み関数となる。手書き checker との乖離はない。

<a id="-推移的caps--theorem-now-proven-capabilityreachv-commit-a28ccf8a"></a>

## ✅ 推移的caps — 定理を証明済み（CapabilityReach.v、commit a28ccf8a）
`CapabilityBound.v` は**関数ごと**の検査（直接使う caps ⊆ 宣言）を証明した。推移的到達（関数が callee の caps に到達すること、つまり「helper 経由でも network なし」）は、それまで**非信頼の** Rust classifier（corpus-wall の `reaches_capability_or_unknown` fold）だけが計算した。**`CapabilityReach.reaches_sound`** が、それを正当化する合成を証明する：
- model：program = 関数 node `{fallowed; fdirect; fcallees}`（callee = index）；
- `reaches` = 直接 caps ∪ ⋃ callee の推移的 caps（fuel で有界）；
- `fn_ok`（関数ごと＋edge ごと）：`direct ⊆ allowed` かつ各 callee で `allowed(g) ⊆ allowed(f)`；
- **定理**：`prog_ok prog -> ∀ fuel i, reaches prog fuel i ⊆ allowed(i)`。
`Subset.v` の共有則に基づき、公理追加なし。`check.sh` で coqc + coqchk。空虚でない demo（helper を呼ぶ main が受理され到達が有界、未宣言 network に到達する helper は拒否）を備える。

END-TO-END 接続 — 完了（commits be98af34、1485a2bc、a774500d）：fold は証明内に置かれ、build ごとに使われる。
- `check_prog_cert`（CapabilityReach）は call-graph witness を証明**内部**で解析する（関数を `;` で区切り、各関数は `declared|direct|callee-indices`）。`prog_within` は推移的到達を**計算**し、関数ごとに `reach ⊆ declared` を検査する。健全性は `check_prog_cert_sound`、公理追加なし。（`prog_ok`/`reaches_sound` の edge ごとの合成則は別の結果として残す。gate は直接の `prog_within` を使い、過大宣言する callee も受け入れる。）
- OCaml へ抽出（`Extract.v` → `checker.ml`）し、`caps-transitive` driver mode を追加。`build-checker.sh` は有界 graph の ACCEPT と未宣言 network の REJECT を実演する。
- `program_cap_graph_witness`（certificate.rs）が MIR から graph を**出力**する。不明／別 file／省略された callee は sentinel UNIVERSE node に向け、それに到達する caller はすべて**拒否**する（保守的で、今は証明が決定）。単体テストと `./checker` を通す end-to-end 検証済み。
- `classify_corpus` は `caps_graph.cert` を書く（完全解析可能かつ境界内の file ごとに1行）。`corpus-wall.sh` がそれに `check_prog_cert` を実行する。**v0 corpus 全体で gate 合格：ownership/names/caps と並んで198 program witness を ACCEPT**。完全解析可能な program の到達可能性 fold は、Rust を信頼せず kernel 証明済み checker が再導出する。

残作業（小さいもの）：部分的に解析可能な file は、なお関数ごとの `caps.cert` を使う（到達は Rust fold が計算）。そこへ graph witness を広げるには、UNIVERSE 汚染を強制拒否でなく gate の正直な範囲の「未検証」とする必要がある（方針の精緻化）。

<a id="全v--cert-level-complete-40-byte-binding-now-grounded-on-both-sides"></a>

## 全V — cert レベルで完了。#40 byte-binding は両側で裏付け済み
flight-grade とする性質はすべて cert レベルで証明済み：ownership（double-free + leak）、names（dangling なし）、caps（関数ごと＋推移的合成）、stack balance、termination、COW safety、loop ownership、rc_inc/rc_dec の **byte-binding**。

<a id="40-byte-binding--encoder--executor-both-grounded-commit-9a82b61d"></a>

## #40 byte-binding — ENCODER と EXECUTOR の両方を裏付け（commit 9a82b61d）
rc byte-binding には model と現実の間に2つの不足がある。両方を実際の tool と照合し、`check.sh` が build ごとに再確認する。
- **ENCODER**（`WasmEncode.v`：命令木 → bytes、`encode_body = rc_inc_bytes`）：`check-wasm-bytes.sh` で裏付ける。wat2wasm が rc primitive を**同じ** bytes に再 assemble する。
- **EXECUTOR**（`WasmExec.v`：bytes の memory effect を証明する専用 interpreter `run_g`）：今は `check-wasm-exec.sh` で裏付ける。wasmtime が**同じ** bytes を実行し、証明した effect と**完全一致**する。rc_inc 4→5、rc_dec rc=1 は0へ解放し `$freelist` へ回収（leak-freedom の回収）、rc_dec rc=0 は TRAP（double-free sentinel）。したがって rc op に対して `run_g` は実 engine に忠実で、byte **実行**の対応付けは循環していない。

<a id="40-wasmcert-coq-port--the-architecture-brought-in-tree-wasmisav-commit-6f2dfda8"></a>

## #40 WasmCert-Coq 移植 — 構造を tree 内へ導入（WasmIsa.v、commit 6f2dfda8）
外部 WasmCert-Coq library の import はここでは非現実的（opam がなく、library は古い Coq 向けで Rocq 9.1 でない）。得られるものは構造、すなわち関係的 ISA **仕様**とそれを精緻化すると証明された実行可能 interpreter であり、それを自前で備えた。
- `istep`：wasm-spec 形式で opcode ごとに1規則を持つ関係的小ステップ意味論（信頼する ISA **仕様**）。`irun` が命令列上で順次接続する。
- `estep`/`erun`：仕様を**精緻化**すると証明した実行可能 evaluator。`erun_sound`（各結果は実際の reduction）、`erun_complete`、`istep_det`（決定的）。
- `rc_inc` の effect（cell → cell+1）を、精緻化によって関係**経由**で証明する。
これにより、精緻化対象の仕様を持たない WasmExec.v の専用 `run_g` を、rc subset の実際の意味論関係に置き換える。公理追加なし、`check.sh`（coqc + coqchk）内。

構造化制御 — 完了（commit e1d2112c）：`istep`/`irun` は `IIf`（block `if/end`、本体を実行）と `IUnreachable`（trap は規則のない stuck 状態）も扱う。Coq の構造 guard は入れ子の単一／相互 fixpoint（list-inside-instr）を拒否するため、実行可能な `erun` は **fuel 有界**とし、fuel 帰納法で関係に対する**健全性**を証明した。**rc_dec も関係経由**にした：
- `rc_dec_isa_traps_on_zero`：double-free は完了しない（全 fuel で interpreter は None。sentinel が発火）。安全性の向き。
- `rc_dec_isa_frees_when_one`：rc=1 ⟹ 0へ decrement し、かつ `$freelist` へ回収（leak-freedom）。仕様内で到達可能（`erun_sound` 経由）。
`check-wasm-exec.sh` が**実験的**に裏付ける double-free trap と回収は、関係的 ISA **定理**にもなった。

FORALL 拡張 — 完了。全 rc 定理は `forall`／関係的になった（commits 8aaaba53、cf438a37）：
- `isa_det`：ISA 関係は**決定的**（config の次状態は高々1つ。結合相互帰納法）。よって program reduction の最終状態は**一意**。
- `erun_complete`：irun ⟹ ∃ fuel, erun = Some（結合帰納法。内側の `erun` を畳んだままにする `erun_S_cons` の1段展開と、`erun_mono_add` の fuel 単調性で IIf 本体と継続を共通 fuel に合わせる。`cbn` が `erun` を展開してしまう問題を解決）。
- `rc_inc_isa_effect`、`rc_dec_isa_frees_when_one`：**`forall`**（「すべての reduction がその effect に到達」）。interpreter が一つを計算（`erun_sound`）し、`isa_det` がそれを唯一にする。
- `rc_dec_isa_traps_rel`：**`~ irun`**。double-free の reduction は一つも完了しない（仕様自身が stuck）。`erun_complete` と実行可能側の `forall`-fuel trap から導く関係的安全性定理。
したがって `erun` は仕様を**精緻化**する検証済み interpreter（健全＋完全＋決定的）で、rc の3事実（inc effect、解放／回収、double-free trap）は**すべての reduction**で成り立つ。公理追加なし。

重い残作業（#40 の最大形）：完全な正準 WasmCert-Coq ISA（完全な検証済み wasm 実行意味論）。これにより対応付けが rc subset だけでなく、抽象的に**全 wasm program**に成立する。数週間の library 移植で、proofs は「延期した重い経路」とする。**実用的** byte-binding（生成 rc bytes ↔ 実 encode ↔ 実 execute）は解消し裏付け済み。WasmIsa.v は rc subset に仕様関係に基づく意味論を与え、完全移植が拡張する構造上の基盤となる。
