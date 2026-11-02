<a id="almide-v1-trust-kernel-coqrocq-proof-spine"></a>

# Almide v1 信頼カーネル（Coq/Rocq 証明の中核）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)

> [正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/README.md)の非規範な読者用スナップショット。固定コミット：35297a612fb29e3f78f6587aba253aa91d51f466。自動同期されない。証明、性能、実装、適格性確認の主張は原文時点の記述であり、ここで独立に再検証していない。検証コマンドは実行していない。正本を優先する。この資料はツールの適格性確認や安全認証の成立を示すものではない。

証明と認証証拠の中核（docs/roadmap/active/v1-mir-architecture.md §5、および tier-1 スタック）。**コンパイラを証明するのではなく、CHECKER を証明する。** 大規模で信頼しないコンパイラが成果物と証明書を出力し、小さなチェッカー `K` が各ビルドを検証する。その `K` の健全性をここで証明する：

```
check(certificate, artifact) = accept  ⟹  P(artifact)
```

これにより信頼対象は約 100k 行のコンパイラから数百行のチェッカー（DO-330 qualification の対象）へ縮小する。コンパイラとチェッカーの接点は証明器ではなく、証明書の形式である。

**証明器の選択は Lean ではなく Coq/Rocq（単一の TB）。** 日常的な証明（RC バランス、スタックバランス）は Lean Perceus belt（`crates/almide-perceus-belt/`）に残す。しかし飛行品質の中核には、(a) 安全認証の前例と、(b) チェッカーを機械語にするための同じ論理内の検証済みコンパイラが必要である。原文時点で両方を備えるのは Coq/Rocq + CompCert/CertiCoq だけである（CompCert は安全性が重要な環境で適格性確認を受けた使用実績を持つ唯一の検証済みコンパイラ）。

<a id="status"></a>

## 状況

- **brick 1（ここ）：`OwnershipChecker.v` — カーネル検証済み。** MIR 所有権操作列に対する RC バランス／所有権チェッカー `K` と、その健全性定理 `check_sound : check ops = true → no_double_free ops ∧ no_leak ops`。これ以上分解できない単一オブジェクトの核。3 通りで検証済み（Rocq 9.1.1）：
  - カーネル検査済み（`coqc`）、`Qed`、admit は `0`；
  - **追加公理なし** — `Print Assumptions check_sound` = *Closed under the global context*（追加公理なし。「Print Assumptions ⊆ standard」ゲート）；
  - **独立した再検査**を `coqchk` で実施（De Bruijn 基準）。

- **brick 4 の開始：`ALS.v` — 規範としての ALS と、最初の正直な翻訳検証。** `exec` を規範的な所有権意味論と定める。`eager_copy_refines_safety` はインクリメントのみの（eager な）トレースが二重解放できないことを証明し、`dec_free_leaks` はそのようなトレースが解放しないこと（release を出力しないこと）を記録する。その後 wasm renderer は RC 方式へ移行した（A1.1b：`Drop → call
  $rc_dec`）。安全性の基礎 `balanced_cert_no_memory_fault` により、セル単位でのリーク不在（`balanced_cert_frees_in_memory`）を実現している。eager 定理は二重 oracle の基準として残る。カーネル検査済み、追加公理なし、coqchk 確認済み。（完全な V = 出力された wasm バイト列 ⊒ ALS には Coq の wasm モデルが必要であり、後の brick で扱う。）

- **brick 2：証明済みチェッカーが実際のバイト列で動作する。** `Extract.v` はカーネルで証明した `check` を OCaml に抽出する。`driver.ml` と `build-checker.sh` がそれを `checker` バイナリへリンクし、**証明書形式 v0**（Metamath のように単純なトークン列：`i`/`I` は所有権 +1、`d`/`D` は −1、空白は無視）を検証する。`./build-checker.sh` はバランスした証明書を受理し、二重解放／リークを拒否する。これにより proof-carrying-code の連鎖が実際に動作する。証明は「モデルについてのもの」にとどまらず、「健全性証明を伴い、実際の出力を判定する動作するバイナリ」になる。

- **#4013：`MemorySafe.v` — メモリ安全性の唯一の定義。** ヒープトレース（alloc / store / load / inc / dec）上で、`MemorySafe` は 4 条件からなる。SPATIAL（境界内）、TEMPORAL（解放後使用、二重解放、復活がない）、INITIALIZATION（未書込みバイトを読まない）、TYPE（表現の混同がない）。`memory_safe_iff_clauses` は、それが正確にこれらの条件であることを証明する。トレースチェッカーはこの性質に対して健全かつ完全であり、各条件に陰性対照がある。`rc_check_gives_temporal` は brick 1 をここへ対応付ける。ガード付き RC 検査が与えるのは TEMPORAL であり、それだけである。トレースを持つ証明書はまだないため、これは出荷バイト列について証明するのではなく、`K(c, a) accepts ⟹ MemorySafe a` の右辺を定める。

- **#4005：`SpatialCert.v` — 静的証明書から SPATIAL 条件を得る。** リストブロックの要素読書きのための証明書言語（read、write、境界検査、添字の再代入、loop、branch。各添字スロットが自分のブロックを指す）、アクセスまでのすべての経路が同じ添字スロットの検査を通り、その後再代入されていないときだけアクセスを受理するチェッカー、そして `accepts_spatial`。受理された証明書のすべての実行は、ループ回数、分岐、添字に書かれる i64 の値、検査失敗による途中終了にかかわらず `Spatial`（かつ `WellTyped`）なトレースになる。この検査は emitter の単一の符号なし比較であり、`unsigned_ge_excludes` は負の添字も除外することを示す。省略された反復検査（#3345）とループ外へ移動された検査（#2319）は信頼せず、チェッカーが再導出する。`rejects_*` 対照は、古くなった検査、再代入をまたいで移動された検査、一方の分岐にしかない検査が拒否されることを示す。`unchecked_run_not_spatial` は拒否された形が実際に境界外を読むことを示す。`BoundsCert.v` はバイト形式とパーサーを証明の内側に加える。`check_bounds_cert` を抽出し（driver mode `bounds`）、`check_bounds_cert_sound` が ACCEPT の意味を述べる。不正な文字列は拒否し、接頭辞として読まない。structural wasm 経路は、#3345 によって検査が省略され得る各アクセスについて、ビルドごとに `bounds` witness（crates/almide-wasm/src/witness_bounds.rs）としてこれを出力する。`structural-wall.sh` は 3 通り（抽出されたチェッカー、カーネル、almide-verify）で判定する。witness のイベントがバイト列に含まれるアクセスと一致することは、所有権 witness と同じく信頼に置いている。

- **#4014：`CoreSafe.v` — 正しく型付けされた core プログラムは MemorySafe。** ソース言語の型付き core は、整数、真偽値、値意味論の整数リスト、box（1 スロットにリストや box を含む任意の値を保持するブロック）、`let`（新しい名前）、代入、要素 store（新しいブロックへコピー）、`if`、`while`、`make`、境界検査付き添字アクセス、`len`、`if` 式、および再帰を伴う 1 引数・2 引数関数を含む（呼出しはパラメーターのみを持つ新しい環境で本体を実行する。2 引数の名前は異なり、復帰時に各引数を順に返す）。big-step 意味論はコンパイラのレイアウト（ヘッダーの後に要素ごとに 1 スロット）で MemorySafe.v のヒープトレースを出力する。ブロックは参照カウントされ、解放される。リスト変数を読むとブロックを共有し（`EInc`）、消費されたオペランド、上書きされた変数、復帰する呼出しのパラメーターは参照を返す（`EDec`）。ゼロになるとブロックを解放し、box を解放するとスロットが保持していた参照を返す（その判断のため意味論はヒープで添字付けされる）。不変条件は、各ブロックのカウントが、プログラムがまだ保持している参照をすべてカバーすること。生存する box のスロット内の参照も含む（ghost store が記録する）。`typed_core_memory_safe`：終了する、または検査失敗で終了させられる正しく型付けされたプログラムのすべての実行は、空間・時間・初期化・型の 4 条件すべてについて `MemorySafe` である。`unchecked_index_not_safe` は空間的安全性が検査に依存することを示し、`shared_list_survives_mutation` は別名参照を変更と解放に通し、`missing_inc_is_use_after_free` は同じ実行から共有読出しの `EInc` を除くと解放済みメモリからロードすることを示す。`box_frees_and_keeps_content` は box を解放しつつ、それが持っていたリストを生存させる。`box_store_without_share_is_use_after_free` は `EInc` のない box store（#2133 の形）が解放済みメモリへ到達することを示す。`count_down_ok` は自己再帰関数を型付けする。未対応：クロージャ、複数フィールドのレコード、variant、文字列、多引数呼出し、スコープ終端での解放（束縛は上書き時に解放され、残りはリークする）、発散する実行、およびコンパイラの lowering とのあらゆる接続。

- **#4013：`Certified.v` — 2 つの証明書の合成。** RC 証明書は TEMPORAL（`rc_check_gives_temporal`）を与え、境界証明書は各アクセスをそのブロックのレイアウト内に置く（`run_sound`）。解放を含むトレースに対しては、どちらか一方だけでは SPATIAL を与えない。アクセスにはブロックの存在が必要で、それを述べるのは RC 側だけだからである。`certified_with_bounds_cert` は、新規割当てを行う allocator とレイアウトに一致した割当ての下で、単一の交錯トレース上の組が与えるものを述べる：`Spatial`、`Temporal`、`WellTyped`。対照は `access_after_free_not_temporal`。解放済みブロックのレイアウト内の読出しは安全ではない。`certified_memory_safe` は追加の仮定 `init_disc`、つまり各割当ての直後に領域全体を初期化する store が続くこと（constructor discipline）によって 4 条件すべてに到達する。その後 `InitCert.v` は初期化／レイアウトの仮定を受理済み構築箇所に置き換える。structural emitter はリストリテラルと範囲リストについて `init` witness を記録するが、すべての割当て箇所ではない。`read_before_init_not_initialized` がその対照である。未到達：証明書のイベントとバイト列をつなぐこと。

<a id="verify-the-third-party-make-verify-trust"></a>

## 検証（第三者用の `make verify-trust`）

```
cd proofs
./check.sh           # coqc (+ axiom audit) then coqchk — the proof
./build-checker.sh   # extract → link → run the proven checker on certificates
```

コードコメントの説明：最初のスクリプトは coqc（公理監査を含む）の後に coqchk を実行して証明を検査する。2 つ目は抽出、リンク、証明書上での証明済みチェッカーの実行を行う。

<a id="known-limitations-the-honest-base--config-management-7"></a>

## 既知の制限（正直な基盤 — 構成管理 §7）

残りを「証明済み」と呼べるよう、信頼する基盤を記録する：
- **カーネル**（Coq/Rocq）は信頼の根として残る。抽出は高速経路であり、ゲート判定の唯一の根ではない。ゲートはカーネル内で witness を独立に再評価し、`almide-verify` と比較する。
- ~~`driver.ml` の tokenizer~~ — **Coq 内へ取り込み済み**（`parse` + `check_cert`、`check_cert_sound` で証明）。*バイト列 ⟶ accept/reject* のパイプライン全体が、現在はカーネル検査済み。信頼する接着部分はファイル I/O のみに縮小している。
- 所有権、名前、capability、呼出しモード、境界、初期化にはチェッカー構成要素がある。`Extract.v` と `structural-wall.sh` を参照。それらの受理済み witness は、出荷されるすべてのプログラムの完全な `MemorySafe` 述語を証明しない。記録、カバレッジ、トレースとバイト列の対応に関する仮定が残る。

Rocq/Coq は `coqc`/`coqchk` を提供する。現在のツールチェーン固定値、公理台帳、厳密なモデル／実装の境界は [TRUSTED_BASE.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/TRUSTED_BASE.md) にある。

<a id="byte-grounding-two-runtimes-one-retired-2753-2761"></a>

## バイト列との対応付け：2 つのランタイム、1 つは廃止（#2753、#2761）

2 つのゲートが、証明済みバイトリストをコンパイラが実際に出力するバイト列へ対応付けていた。対象は異なるランタイムである：

| ゲート | 対応付ける証明 | 比較対象のバイト列 |
|---|---|---|
| `check-wasm-bytes.sh`（#2761 で廃止）（+ `check-wasm-exec.sh`） | `WasmEncode.v`、`WasmExec.v`（`WasmRcDec.v`、`WasmDecode.v`、`WasmIsa.v` とともに：FORMAL-CREDIT FC-4） | **旧来の** WAT renderer の `$rc_inc` / `$rc_dec`。`render_program` の出力から抽出 |
| `check-structural-bytes.sh` | `StructuralDecode.v`（`StructuralRuntime.v`、`StructuralRun.v`、`StructuralAlloc.v` とともに） | **structural** emitter の `$inc` / `$dec_flat` / `$free` / `$alloc` のコード本体（`almide-wasm` `dump_runtime_bytes`） |

structural emitter は既定の `--target wasm` 経路なので、そのランタイムバイト列はすでに対応付けられている。旧来のランタイムのバイトリストをそこへ再対応付けすることはできない。2 つのランタイムは別のコードであり、structural の本体にはすでに独自の証明済みリストがある。

したがって `check-wasm-bytes.sh` は、旧来の renderer を削除した変更（#2761、#1696 step 5）で、**renderer とともに廃止**された。それより前ではない。それまでは旧来の renderer もプログラムを出荷しており、ゲートはそのバイト列についての真の証拠だった。削除時に：

- `check-wasm-bytes.sh`、`proofs/check.sh` 内の呼出し、ゲート検証の行を削除した。`check-wasm-exec.sh` は引き続き実エンジンに対して `run_g` を実行するが、その rc 本体はもはや「renderer のもの」ではない；
- FC-4 と C-PROVEN の receipt 行は、モデル化されたランタイム（廃止済みの旧来のもの）についての定理として記述し直し、出荷バイト列との対応付けには `check-structural-bytes.sh` を指定する。

<a id="original-brick-plan-historical-ordering-not-current-pending-status"></a>

## 当初の brick 計画（歴史的な順序であり、現在の未完了状態ではない）

1. ✅ 最初の性質（RC バランス）のチェッカーと健全性 — このファイル。
2. 証明書形式 v0（Metamath のように単純な witness 言語）— 接点。
3. コンパイラが witness を出力するよう接続し、CI で「K がすべてのビルドを受理する」をゲートする。
4. ALS core（Coq の small-step 意味論）と翻訳 validator `V`（出力された wasm バイト列 ⊒ 検証済みモデル）をビルドごとに使用する。
5. 一度に 1 つの性質：スタックバランス／名前の全域性／capability 上限／型の具体化／停止性。それぞれ ALS に対して定義し、K の健全性を証明し、witness を出力し、CI ゲートを設け、receipt に加える。
6. CertiCoq + CompCert：`K`/`V` を機械語に抽出して Thompson hole を閉じる。多様性：独立チェッカー ≥2。構成管理：ツールチェーン固定値、公理台帳、既知の制限（これ以上削れない基盤：Coq カーネル、CompCert/CertiCoq TB、ハードウェア、ALS の妥当性）。

---

原文に関する注意：当初の brick 計画は歴史的なもので、現在の完了チェックリストではない。core 定理は記載された型付き部分言語と、終了または検査で終了する実行を対象とし、任意のコンパイラ出力を対象としない。原文は 1・2 引数呼出しを含める一方、多引数呼出しを未対応に挙げる。この表現は定理を拡張せず保持した。証明書とバイト列の対応、割当て箇所のカバレッジは明示的な制限として残る。抽出が動作するとの説明、証明器・コンパイラの唯一性の主張は原文時点の主張であり、新たな適格性確認や市場評価ではない。この資料は例示コマンドの実行を承認せず、実行もしていない。
