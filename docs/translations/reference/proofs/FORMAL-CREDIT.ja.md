<a id="formal-credit-claims-table-575-do-333-shape"></a>

# 形式手法クレジットの主張表（#575、DO-333 形式）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](FORMAL-CREDIT.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](FORMAL-CREDIT.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](FORMAL-CREDIT.zh-CN.md)

> [正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/FORMAL-CREDIT.md)（固定コミット 35297a612fb29e3f78f6587aba253aa91d51f466）の非規範的な履歴参照用スナップショット。正本が優先され、この写しは自動同期されず、生成器や証明の入力ではない。コマンド、証明、測定、資格認定評価を独立に実行していない。出典参照、生成マーカー、日付付き数値は履歴資料として保持し、認証・資格認定・当局の受理・公開の証拠とはしない。

形式手法の補足規定では、通常ならテストで充足する検証目標を、証明によって充足できる。クレジットの対象となり得るには、各証明成果物を、仮定とカバレッジの境界を伴う、目標に即した主張として記述しなければならない。本表は証明の系統ごとにその記述を行う。認証状態は一切主張しない。特定の標準のもとで主張にクレジットを与えるかは、申請者と認定当局が判断する。本表が行うのは、各主張を評価できるだけの精度にすることである。

境界列の根拠：[契約境界マップ](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/README.md)（地図）および [proofs/TRUSTED_BASE.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/TRUSTED_BASE.md)（現地。ツールチェーンの固定、除去できない信頼の根、改ざん訓練）。機械化された証明は Rocq（`proofs/*.v`。2026-08-28 に数えた Theorem/Lemma/Corollary 宣言は 153 個で、歴史的スナップショット）。別の監査済み仮定数は `proofs/check.sh` により導出・検査され、すべての theorem、lemma、corollary 宣言の総数ではない。別枠の Lean「belt」crate は設計の鏡であり、末尾に分けて列挙する。実装の証明ではなく、そのように主張することもない。

<a id="the-claims"></a>

## 主張

| # | 目標に即した主張 | 証明成果物 | 仮定（信頼基盤） | カバレッジの境界 |
|---|---|---|---|---|
| FC-1 | 受理された所有権証明書は、証明対象の関数が二重解放、リーク、ダングリングな名前の使用を行わないことを含意する | `OwnershipChecker.v` (17)、`OwnershipFilter.v` (3)、`OwnershipLoop.v` (4)、`Subset.v` (5)、`Termination.v` (2)、`StackBalance.v` (1) | Rocq カーネル（coqchk 再検査、9.1.1/9.2 のバージョン横断）。抽出用の接着コード `driver.ml`（信頼対象。両方向の否定テスト済み。gate-verification の `proofs/build-checker.sh` を参照）。証明書が MIR を忠実に記述すること（生成者は信頼しない。チェッカーが `proofs/gate.sh` を通してビルドごとに再検証） | 証明書モデルの記載された所有権／名前の性質を証明するのであって、出荷バイト列に対する `MemorySafe` の 4 条項すべてではない。lowering の意味的正しさについては何も証明しない。IR→MIR 行は信頼対象で、証明ではなく `mir_wellformed.rs` と差分検証経路で守る |
| FC-2 | copy-on-write のエイリアスは値意味論を保つ。書き込み前に取得したスナップショットは書き込み前の値を観測する | `CowSafety.v` (3)、`CoownCompose.v` (3)、`CoownLoop.v` (5) | 同じカーネル／抽出の信頼の根。ランタイムがモデル化された COW 規律を実装すること（C-033 の fixture 群と差分 fuzz が橋渡しする） | 証明されるのはモデルの COW ルール。実装がモデルに準拠することは fixture＋差分の証拠分類であり、証明ではない |
| FC-3 | モデル化された free-list アロケータはエイリアスされたブロックや解放済みブロックを払い出さず、RC 遷移はヒープ不変条件を保つ | `FreeList.v` (29)、`FreeListRc.v` (27)、`RuntimeModel.v` (5)、`StructuralRuntime.v` (19)、`StructuralAlloc.v` (6)、`StructuralDecode.v` (4)、`StructuralRun.v` (42)、`LargeList.v` (42)、`LargeTree.v` (40) | カーネル／抽出。`StructuralRuntime.v` は structural 経路で生成された `$inc`/`$dec_flat`/`$free` 命令木が `rt_inc`/`rt_dec` とサイズクラス・リストへの push を実現することを、クラス計算の要石（ceil-class のカバレッジ、file/take の一致。必要量を下回る払い出しによる破損の形は表現不能）とともに証明する。木↔バイト列の対応は 4 つすべてのランタイム関数についてカーネル検査される。`StructuralDecode.v` のシンボリックスタック・デコーダは、生成バイト列（実行ごとに `check-structural-bytes.sh` で根拠付け）を証明済みの木へ直接写像する。`$alloc` も含む（tee の脱糖衣、第 2 global、`memory.size`、複数バイト SLEB。grow 分岐本体はそのまま照合し、宣言した 1 ステップの意味へ抽象化する）。Rust 側のハッシュ固定を高速な検知線とする | structural 経路の inc/dec/free に対する木レベルの実現。`StructuralAlloc.v` は `$alloc` の take/bump 経路を追加する（pop が `dec_unique_files_take_class` に対する再利用サイクルを閉じる。grow 本体は no-grow 仮定のもとで 1 抽象ステップのまま）。バイトデコード側は inc/dec/free/alloc について完結し、`StructuralRun.v` は実行全体の合成を閉じる。具体的な木のメモリ作用の任意列は、ghost アロケータと結合すると具体的メモリ上の `KINV` を保つ（`structural_runs_preserve_KINV`）。13 サイズクラスの RINV、64 KiB 超の厳密サイズ大ブロックリストの LINV（#3348：`LargeList.v` はアドレス順 split/coalesce リストをモデル化し、`LargeTree.v` は生成された `$lfree`/`$ltake` の LOOP 木がメモリ内リスト上でそれを計算することを証明）、および仮定した静的条件ではなく証明した状態不変条件としての小／大ブロック分離を含む。F-class は、名指しされた 2 つのレイアウト前提（heap floor が少なくとも 16、class table はその下）のもと、生成コードの補題に対する違反として記述される（`structural_no_aliased_handout`、`structural_counts_stay_honest`、`structural_large_handout_is_free`）。grow の再試行は slice 2 が宣言した抽象化境界に残る。structural の `$dec_flat` は #4023 以降ゼロセンチネルを持つ。実現定理は上流 Perceus 証明書の `1 <= rc` 前提を維持し、`dec_freed_block_traps` は rc 0 での dec がいかなる store よりも前に停止することを証明する（trap による中断自体はエンジンが担う） |
| FC-4 | 廃止された旧ランタイムに残された `rc_inc`/`rc_dec` のバイト木は、デコード・実行するとモデル化した RC 遷移となる | `WasmIsa.v` (13)、`WasmExec.v` (9)、`WasmDecode.v` (6)、`WasmEncode.v` (1)、`WasmRcDec.v` (2) | カーネル。旧 `proofs/check-wasm-bytes.sh` の根拠付けゲートは旧 renderer とともに廃止（#2761） | そのモデル化されたランタイムについての定理であって、現在出荷されるランタイムバイト列についてではない。出荷される structural-runtime の根拠付けは `proofs/check-structural-bytes.sh` 経由の FC-3。どちらの行もモジュール全体の意味論を証明しない |
| FC-5 | 関数の到達可能な能力集合は宣言した effect により制限され、検査済み呼び出しグラフを通して未宣言能力には到達しない | `CapabilityBound.v` (2)、`CapabilityReach.v` (3)、`CallModes.v` (6) | カーネル／抽出。チェッカーの E006/E007 境界がソース境界でモデル化したルールを強制する（これらの境界は MC/DC safety set に含まれる、#566） | モデル化した呼び出しグラフ上の証明。モデル外の動的ディスパッチ（計算された呼び出し）は壁で拒否し、証明していない |
| FC-6 | 名前解決は受理される部分集合上で全域的であり、型の具体化は出力位置に抽象型を残さない | `NameTotality.v` (2)、`TypeConcretization.v` (1) | カーネル／抽出。`assert_names_resolvable` と `ConcretizeTypes` ゲートが対応する実装チェックをビルドごとに実行する | 同じモデル／実装の分離。定理はモデルについてであり、ビルド内ゲートが実装側を担う |
| FC-7 | 証明書言語の変換は、モデル化された構文要素について ALS が規定する観測可能な意味論を保つ | `Translation.v` (2)、`ALS.v` (2) | カーネル／抽出。ALS 台帳の契約行が構文要素ごとの fixture を保持する | 意図的に狭い範囲、すなわちモデル化された構文要素だけ。広さを示す手段は現在の契約台帳 |

抽出したバイナリは高速経路である。`proofs/gate.sh` とコーパスのゲートは、その判定を Rocq カーネルとポータブル検証器に照らして比較する。これによって表中のモデルと実装を結ぶ仮定がなくなるわけではない。

ビルドごとの検査（すべての証明系統について完全な意味的結合を行うものではない）：`proofs/gate.sh`（抽出されたカーネル証明済みチェッカーが各ビルドの証明書を再検証。ツールチェーン stamp の不一致は FATAL）、`proofs/corpus-wall.sh`（チェッカー段階の ACCEPT はカーネルで証明される。実際の REJECT が via_if 誤コンパイルを検出し、実際のバグで失敗方向が作動した）、`proofs/structural-wall.sh`（出荷 structural 成果物、証明書 bundle、バイト anchor、明示的に信頼する記録境界）。

<a id="the-lean-belts-design-mirrors-not-credit-claims"></a>

## Lean belt（設計の鏡であり、クレジットの主張ではない）

`crates/almide-perceus-belt`、`almide-race-belt`、`almide-edit-belt` は、設計規律（RC 挿入ルール、race determinism の論証、edit-locality 定理）を映す実行可能な Lean の鏡である。これらが固定するのは設計であり、Rust 実装が参照している。しかし、機械的に出荷コードへ結び付けるものがないため、上表のどの主張もこれらに依存しない。

<a id="dossier-status"></a>

## Dossier の状態

本表は #575 の継続的な成果物である。リリースごとの組み立て器は `scripts/gen-dossier.sh` で、タグ付き Trust Spine workflow が呼び出す。特定リリースの添付 dossier と attestation はリリース成果物として確認しなければならない。seal（`proofs/releases/*.toml`）とこのファイルだけでは公開を証明しない。
